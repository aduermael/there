struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_dir: vec3<f32>,
    fog_color: vec3<f32>,
    fog_far: f32,
    world_size: f32,
    hm_res: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) inst_pos_yaw: vec4<f32>,
    @location(2) inst_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let yaw = in.inst_pos_yaw.w;
    let c = cos(yaw);
    let s = sin(yaw);

    // Rotate capsule around Y axis by yaw
    let rotated = vec3(
        in.position.x * c - in.position.z * s,
        in.position.y,
        in.position.x * s + in.position.z * c,
    );

    let world_pos = rotated + in.inst_pos_yaw.xyz;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = in.inst_color.rgb;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Flat shading via screen-space derivatives
    let dx = dpdx(in.world_pos);
    let dy = dpdy(in.world_pos);
    let n = normalize(cross(dx, dy));

    // Directional light + ambient (same as terrain)
    let ndl = max(dot(n, u.sun_dir), 0.0);
    let lit = in.color * (0.3 + 0.7 * ndl);

    // Distance fog
    let dist = length(in.world_pos - u.camera_pos);
    let fog = clamp(dist / u.fog_far, 0.0, 1.0);
    let color = mix(lit, u.fog_color, fog);

    return vec4(color, 1.0);
}
