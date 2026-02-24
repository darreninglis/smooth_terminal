use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CellBgVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl CellBgVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CellBgVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct CellBgRenderer {
    pub pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    max_quads: usize,
}

impl CellBgRenderer {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cell_bg_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/cell_bg.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cell_bg_layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cell_bg_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[CellBgVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let max_quads = 8192;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell_bg_vb"),
            size: (max_quads * 4 * std::mem::size_of::<CellBgVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Pre-build index buffer for quads: 0,1,2, 0,2,3 per quad
        let indices: Vec<u32> = (0..max_quads as u32)
            .flat_map(|i| {
                let base = i * 4;
                [base, base + 1, base + 2, base, base + 2, base + 3]
            })
            .collect();
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell_bg_ib"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self { pipeline, vertex_buffer, index_buffer, max_quads }
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        queue: &wgpu::Queue,
        vertices: &[CellBgVertex],
        quad_count: usize,
    ) {
        if quad_count == 0 || vertices.is_empty() {
            return;
        }
        let quad_count = quad_count.min(self.max_quads);
        let verts = &vertices[..quad_count * 4];
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(verts));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cell_bg_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..(quad_count * 6) as u32, 0, 0..1);
    }
}

/// Convert cell rect (in physical pixels) to NDC quad vertices
pub fn cell_quad_vertices(
    x: f32, y: f32,
    w: f32, h: f32,
    color: [f32; 4],
    surface_w: f32,
    surface_h: f32,
) -> [CellBgVertex; 4] {
    let to_ndc_x = |px: f32| (px / surface_w) * 2.0 - 1.0;
    let to_ndc_y = |py: f32| 1.0 - (py / surface_h) * 2.0;

    let x0 = to_ndc_x(x);
    let x1 = to_ndc_x(x + w);
    let y0 = to_ndc_y(y);
    let y1 = to_ndc_y(y + h);

    [
        CellBgVertex { position: [x0, y0], color },
        CellBgVertex { position: [x1, y0], color },
        CellBgVertex { position: [x1, y1], color },
        CellBgVertex { position: [x0, y1], color },
    ]
}
