// Shared fullscreen triangle vertex shader for post-processing passes.
// Generates a single triangle covering the entire screen from vertex_index.
// Prepended to ssao, postprocess, and future fullscreen passes.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let uv_x = f32((vi << 1u) & 2u);
    let uv_y = f32(vi & 2u);
    var out: VertexOutput;
    out.position = vec4(uv_x * 2.0 - 1.0, uv_y * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2(uv_x, 1.0 - uv_y);
    return out;
}
