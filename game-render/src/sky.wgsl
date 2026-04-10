struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_dir: vec3<f32>,
    fog_color: vec3<f32>,
    fog_far: f32,
    world_size: f32,
    hm_res: f32,
    ambient_intensity: f32,
    sun_color: vec3<f32>,
    sky_zenith: vec3<f32>,
    sky_horizon: vec3<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Fullscreen triangle: 3 vertices cover the screen without a vertex buffer
@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VertexOutput {
    // Generates a fullscreen triangle from vertex_index 0,1,2
    let x = f32(i32(id & 1u)) * 4.0 - 1.0;
    let y = f32(i32(id >> 1u)) * 4.0 - 1.0;

    var out: VertexOutput;
    out.clip_pos = vec4(x, y, 1.0, 1.0); // z=1.0 (far plane)
    // UV: (0,0) at top-left to (1,1) at bottom-right
    out.uv = vec2(x * 0.5 + 0.5, -y * 0.5 + 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Reconstruct view direction from screen UV and inverse view-proj
    // Simpler approach: use UV.y as vertical factor (0=top, 1=bottom)
    // Top of screen → zenith color, bottom → horizon color
    let t = pow(in.uv.y, 1.5); // curve for more natural gradient
    let color = mix(u.sky_zenith, u.sky_horizon, t);

    return vec4(color, 1.0);
}
