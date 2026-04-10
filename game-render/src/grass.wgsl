// Grass-specific: instanced blades with wind animation and distance fade.
// Uniforms, lighting, and fog provided by common.wgsl prefix.

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) bend: f32,
    @location(2) inst_pos_scale: vec4<f32>,
    @location(3) inst_color_rotation: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) bend_factor: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let scale = in.inst_pos_scale.w;
    let angle = in.inst_color_rotation.w;
    let base_pos = in.inst_pos_scale.xyz;

    // Rotate blade around Y axis
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let rotated = vec3(
        in.position.x * cos_a - in.position.z * sin_a,
        in.position.y,
        in.position.x * sin_a + in.position.z * cos_a,
    );

    // Distance fade: shrink blades between 50-80 units from camera
    let cam_dist = length(base_pos - u.camera_pos);
    let fade = 1.0 - smoothstep(50.0, 80.0, cam_dist);

    var local = rotated * scale * fade;

    // Wind animation: wider blades sway more dramatically
    let wind_phase = base_pos.x * 0.15 + base_pos.z * 0.1;
    let wind_base = sin(u.time * 1.8 + wind_phase) * 0.22;
    let wind_detail = sin(u.time * 3.7 + wind_phase * 2.3) * 0.08;
    let wind_gust = sin(u.time * 0.7 + wind_phase * 0.3) * 0.06; // slow gusts
    let wind = (wind_base + wind_detail + wind_gust) * in.bend * fade;
    local.x += wind;
    local.z += wind * 0.4;

    let world_pos = local + base_pos;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = in.inst_color_rotation.rgb;
    out.bend_factor = in.bend;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = compute_flat_normal(in.world_pos);

    // Soft base-to-tip gradient: subtle root shadow, bright sunlit tips
    let grad = smoothstep(0.0, 0.3, in.bend_factor);
    var blade_color = in.color * (0.85 + 0.25 * grad);  // base at 85%, tip at 110%
    // Tips luminous — brighter and greener (sunlit translucent feel)
    let tip_glow = smoothstep(0.3, 1.0, in.bend_factor);
    blade_color.g += tip_glow * 0.18;
    blade_color.r += tip_glow * 0.05;
    blade_color.b += tip_glow * 0.03;

    let shadow = sample_shadow(in.world_pos);
    let lit = hemisphere_lighting(n, blade_color, shadow);
    let color = apply_fog(in.world_pos, lit);
    return vec4(color, 1.0);
}
