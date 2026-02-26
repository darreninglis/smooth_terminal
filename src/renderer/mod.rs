pub mod background;
pub mod cell_bg;
pub mod cursor;
pub mod text_renderer;

use crate::animation::scroll::ScrollSpring;
use crate::config::{parse_hex_color, Config};
use crate::pane::layout::Rect;
use crate::pane::PaneTree;
use crate::renderer::background::BackgroundRenderer;
use crate::renderer::cell_bg::CellBgRenderer;
use crate::renderer::cursor::CursorAnimator;
use crate::renderer::text_renderer::{
    build_span_buffers, to_glyphon_color, PaneTextRenderer, SpanBuffer,
};
use glyphon::{TextArea, TextBounds};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::SurfaceError;
use winit::window::Window;

pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,

    pub cell_bg_renderer: CellBgRenderer,
    pub text_renderer: PaneTextRenderer,
    pub background_renderer: Option<BackgroundRenderer>,

    pub cursor_animators: HashMap<usize, CursorAnimator>,
    pub scroll_springs: HashMap<usize, ScrollSpring>,
    /// Per-pane span-buffer cache.  Key = pane_id, Value = (grid generation, buffers).
    /// Rebuilt only when the grid generation changes; reused every other frame.
    text_cache: HashMap<usize, (u64, Vec<SpanBuffer>)>,

    pub cell_w: f32,
    pub cell_h: f32,
    pub font_size_px: f32,
    pub scale_factor: f32,
    pub app_config: Config,
}

impl Renderer {
    pub fn new(window: Arc<Window>, app_config: Config) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).expect("create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("request adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            },
        ))
        .expect("request device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let alpha_mode = surface_caps
            .alpha_modes
            .iter()
            .find(|m| **m == wgpu::CompositeAlphaMode::PreMultiplied)
            .copied()
            .unwrap_or(surface_caps.alpha_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let cell_bg_renderer = CellBgRenderer::new(&device, surface_format);
        let mut text_renderer = PaneTextRenderer::new(&device, &queue, surface_format);

        // Load background image if configured
        let background_renderer = app_config.background.image_path.as_ref().and_then(|path| {
            let opacity = app_config.background.image_opacity.unwrap_or(0.3);
            match image::open(path) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    Some(BackgroundRenderer::new(
                        &device,
                        &queue,
                        surface_format,
                        rgba.as_raw(),
                        w,
                        h,
                        opacity,
                    ))
                }
                Err(e) => {
                    log::warn!("Failed to load background image: {}", e);
                    None
                }
            }
        });

        // Scale font to physical pixels (Retina = 2.0x, standard = 1.0x)
        let scale_factor = window.scale_factor() as f32;
        let font_size_px = app_config.font.size * scale_factor;
        let line_height = app_config.font.line_height;
        let cell_h = font_size_px * line_height;

        // Measure the actual advance width of a monospace character
        let cell_w = measure_cell_width(
            &mut text_renderer.font_system,
            font_size_px,
            cell_h,
            &app_config.font.family,
        );

        Self {
            surface,
            device,
            queue,
            config,
            surface_format,
            cell_bg_renderer,
            text_renderer,
            background_renderer,
            cursor_animators: HashMap::new(),
            scroll_springs: HashMap::new(),
            text_cache: HashMap::new(),
            cell_w,
            cell_h,
            font_size_px,
            scale_factor,
            app_config,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn ensure_pane_state(&mut self, pane_id: usize) {
        let cursor_freq = self.app_config.animation.cursor_spring_frequency;
        let scroll_freq = self.app_config.animation.scroll_spring_frequency;
        let cursor_color = parse_hex_color(&self.app_config.colors.cursor)
            .unwrap_or([0.96, 0.76, 0.91, 1.0]);
        let trail = self.app_config.animation.cursor_trail_enabled;

        self.cursor_animators.entry(pane_id).or_insert_with(|| {
            CursorAnimator::new(cursor_freq, cursor_color, self.cell_w, self.cell_h, trail)
        });
        self.scroll_springs.entry(pane_id).or_insert_with(|| {
            ScrollSpring::new(scroll_freq)
        });
    }

    pub fn tick_animations(&mut self, dt: f32) {
        for anim in self.cursor_animators.values_mut() {
            anim.tick(dt);
        }
        for spring in self.scroll_springs.values_mut() {
            spring.tick(dt);
        }
    }

    pub fn render(
        &mut self,
        pane_tree: &PaneTree,
        window_rect: Rect,
    ) -> Result<(), SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let surface_w = self.config.width as f32;
        let surface_h = self.config.height as f32;

        let bg_color = parse_hex_color(&self.app_config.colors.background)
            .unwrap_or([0.118, 0.118, 0.18, 1.0]);
        let window_opacity = self.app_config.window.opacity;
        let fg_color = parse_hex_color(&self.app_config.colors.foreground)
            .unwrap_or([0.8, 0.84, 0.96, 1.0]);
        let palette = self.app_config.colors.ansi_palette();

        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });

        // Clear pass
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: (bg_color[0] * window_opacity) as f64,
                            g: (bg_color[1] * window_opacity) as f64,
                            b: (bg_color[2] * window_opacity) as f64,
                            a: window_opacity as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // Background image pass
        if let Some(bg) = &self.background_renderer {
            bg.render(&mut encoder, &view);
        }

        // Compute layout rects
        let layout_rects = pane_tree.layout.compute_rects(window_rect);

        // Ensure animation state for all panes
        for (pane_id, _) in &layout_rects {
            self.ensure_pane_state(*pane_id);
        }

        // ---- Phase 1: Build cell backgrounds ----
        // Cell background colors are not rendered (no colored highlighting behind text).
        let bg_vertices: Vec<crate::renderer::cell_bg::CellBgVertex> = Vec::new();

        let quad_count = bg_vertices.len() / 4;
        if quad_count > 0 {
            self.cell_bg_renderer.render(
                &mut encoder, &view, &self.queue, &bg_vertices, quad_count,
            );
        }

        // ---- Phase 2: Cursor pass ----
        let focused_id = pane_tree.focused_id;
        if let Some(anim) = self.cursor_animators.get(&focused_id) {
            anim.render(
                &mut encoder,
                &view,
                &self.queue,
                &self.cell_bg_renderer,
                surface_w,
                surface_h,
            );
        }

        // ---- Phase 3: Build text (generation-cached span buffers) ----
        let cell_w = self.cell_w;
        let cell_h = self.cell_h;
        let font_size_px = self.font_size_px;
        let font_family = self.app_config.font.family.clone();

        // Phase 3a: Refresh the span-buffer cache for any pane whose grid has
        // changed since the last frame.  On idle frames (cursor animating but
        // no new PTY output) this loop does zero shaping work.
        for (pane_id, _) in &layout_rects {
            let pane = match pane_tree.panes.iter().find(|p| p.id == *pane_id) {
                Some(p) => p,
                None => continue,
            };
            let grid = pane.terminal.grid.lock();
            let current_gen = grid.generation;
            // Skip rebuild if cache is fresh.
            if self.text_cache.get(pane_id).map_or(false, |(g, _)| *g == current_gen) {
                continue;
            }
            let span_buffers = build_span_buffers(
                &mut self.text_renderer.font_system,
                &grid,
                cell_h,
                font_size_px,
                &font_family,
                cell_w,
                fg_color,
                &palette,
            );
            drop(grid);
            self.text_cache.insert(*pane_id, (current_gen, span_buffers));
        }

        // Phase 3b: Build TextAreas from the (possibly just-refreshed) cache.
        // Scroll offset is applied here so smooth-scroll animation still works
        // even when the grid itself hasn't changed.
        let default_color = to_glyphon_color(fg_color);
        let mut text_areas: Vec<TextArea> = Vec::new();
        for (pane_id, pane_rect) in &layout_rects {
            let scroll_offset = self.scroll_springs
                .get(pane_id)
                .map(|s| s.pixel_offset())
                .unwrap_or(0.0);
            if let Some((_, span_buffers)) = self.text_cache.get(pane_id) {
                for sb in span_buffers {
                    let y = pane_rect.y + sb.row_idx as f32 * cell_h - scroll_offset;
                    if y + cell_h < pane_rect.y || y > pane_rect.y + pane_rect.height {
                        continue;
                    }
                    let x = pane_rect.x + sb.col_start as f32 * cell_w + sb.x_offset;
                    text_areas.push(TextArea {
                        buffer: &sb.buffer,
                        left: x,
                        top: y,
                        scale: 1.0,
                        bounds: TextBounds {
                            left: pane_rect.x as i32,
                            top: pane_rect.y as i32,
                            right: (pane_rect.x + pane_rect.width) as i32,
                            bottom: (pane_rect.y + pane_rect.height) as i32,
                        },
                        default_color,
                        custom_glyphs: &[],
                    });
                }
            }
        }

        if !text_areas.is_empty() {
            let _ = self.text_renderer.prepare(
                &self.device,
                &self.queue,
                self.config.width,
                self.config.height,
                text_areas,
            );

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("text_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            let _ = self.text_renderer.render(&mut pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.text_renderer.trim_atlas();
        Ok(())
    }

    pub fn update_cursor_for_pane(
        &mut self,
        pane_id: usize,
        col: usize,
        row: usize,
        pane_rect: Rect,
    ) {
        let scroll_offset = self.scroll_springs
            .get(&pane_id)
            .map(|s| s.pixel_offset())
            .unwrap_or(0.0);
        self.ensure_pane_state(pane_id);
        if let Some(anim) = self.cursor_animators.get_mut(&pane_id) {
            anim.set_cell_size(self.cell_w, self.cell_h);
            if anim.is_warming_up() {
                anim.snap_to(col, row, pane_rect.x, pane_rect.y, scroll_offset);
            } else if anim.target_col != col || anim.target_row != row {
                anim.move_to(col, row, pane_rect.x, pane_rect.y, scroll_offset);
            }
        }
    }

    /// Apply updated config values and/or DPI scale changes. Returns true if
    /// cell metrics changed (caller must then resize panes).
    pub fn apply_config(&mut self, new_config: Config, scale_factor: f32) -> bool {
        let font_changed = new_config.font.family != self.app_config.font.family
            || (new_config.font.size - self.app_config.font.size).abs() > 0.01
            || (new_config.font.line_height - self.app_config.font.line_height).abs() > 0.01;
        let scale_changed = (scale_factor - self.scale_factor).abs() > 0.001;
        let metrics_changed = font_changed || scale_changed;

        self.app_config = new_config;

        if metrics_changed {
            let font_size_px = self.app_config.font.size * scale_factor;
            let cell_h = font_size_px * self.app_config.font.line_height;
            let cell_w = measure_cell_width(
                &mut self.text_renderer.font_system,
                font_size_px,
                cell_h,
                &self.app_config.font.family,
            );
            self.font_size_px = font_size_px;
            self.cell_h = cell_h;
            self.cell_w = cell_w;
            self.scale_factor = scale_factor;
            for anim in self.cursor_animators.values_mut() {
                anim.set_cell_size(cell_w, cell_h);
            }
        }

        // Always clear text cache — forces re-shaping with new colors and/or font
        self.text_cache.clear();

        metrics_changed
    }

    pub fn snap_cursor_for_pane(
        &mut self,
        pane_id: usize,
        col: usize,
        row: usize,
        pane_rect: Rect,
    ) {
        let scroll_offset = self.scroll_springs
            .get(&pane_id)
            .map(|s| s.pixel_offset())
            .unwrap_or(0.0);
        self.ensure_pane_state(pane_id);
        if let Some(anim) = self.cursor_animators.get_mut(&pane_id) {
            anim.set_cell_size(self.cell_w, self.cell_h);
            anim.snap_to(col, row, pane_rect.x, pane_rect.y, scroll_offset);
        }
    }
}

/// Shape a single "M" character at the given physical-pixel font size and return
/// its advance width. This gives us the true cell width for the loaded font.
fn measure_cell_width(font_system: &mut glyphon::FontSystem, font_size_px: f32, cell_h: f32, font_family: &str) -> f32 {
    use glyphon::{Attrs, Buffer, Family, Metrics, Shaping};
    let family = if font_family.is_empty() { Family::Monospace } else { Family::Name(font_family) };
    let metrics = Metrics::new(font_size_px, cell_h);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(font_system, None, None);
    buffer.set_text(font_system, "M", &Attrs::new().family(family), Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);
    buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.iter())
        .map(|g| g.w)
        .next()
        .unwrap_or(font_size_px * 0.6)
}
