// Rock-specific: instanced deformed icosphere.
// Uniforms, lighting, and fog provided by common.wgsl prefix.

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) inst_pos_scale: vec4<f32>,
    @location(2) inst_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let scale = in.inst_pos_scale.w;
    let world_pos = in.position * scale + in.inst_pos_scale.xyz;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = in.inst_color.rgb;
    return out;
}

@vertex
fn vs_shadow(
    @location(0) position: vec3<f32>,
    @location(1) inst_pos_scale: vec4<f32>,
    @location(2) inst_color: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    let world_pos = position * inst_pos_scale.w + inst_pos_scale.xyz;
    return u.sun_view_proj * vec4(world_pos, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = compute_flat_normal(in.world_pos);
    let lit = hemisphere_lighting(n, in.color);
    let rim = rim_light(n, in.world_pos);
    let color = apply_fog(in.world_pos, lit + rim);
    return vec4(color, 1.0);
}
