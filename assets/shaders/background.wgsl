struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Full-screen triangle
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var texcoords = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var out: VertexOutput;
    out.clip_position = vec4<f32>(positions[idx], 0.0, 1.0);
    out.tex_coord = texcoords[idx];
    return out;
}

@group(0) @binding(0) var bg_texture: texture_2d<f32>;
@group(0) @binding(1) var bg_sampler: sampler;

struct Uniforms {
    opacity: f32,
    _pad: vec3<f32>,
};
@group(0) @binding(2) var<uniform> uniforms: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(bg_texture, bg_sampler, in.tex_coord);
    return vec4<f32>(color.rgb, color.a * uniforms.opacity);
}
