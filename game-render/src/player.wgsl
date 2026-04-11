// Player-specific: instanced capsule with yaw rotation.
// Uniforms, lighting, fog, and shadow bindings provided by common.wgsl prefix.

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
    let n = compute_flat_normal(in.world_pos);
    let shadow = sample_shadow(in.world_pos);
    let lit = hemisphere_lighting(n, in.color, shadow, in.world_pos);
    let color = apply_fog(in.world_pos, lit);
    return vec4(color, 1.0);
}
