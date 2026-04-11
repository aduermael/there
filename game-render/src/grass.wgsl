// Grass-specific: instanced blades with wind animation and distance fade.
// Uniforms, lighting, fog, and shadow bindings provided by common.wgsl prefix.
// Instance data from GPU compute shader via storage buffer.

struct GrassInstanceData {
    pos_scale: vec4<f32>,
    color_rotation: vec4<f32>,
};
@group(2) @binding(0) var<storage, read> instances: array<GrassInstanceData>;

struct VertexInput {
    @builtin(instance_index) instance_id: u32,
    @location(0) position: vec3<f32>,
    @location(1) bend: f32,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) bend_factor: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let inst = instances[in.instance_id];
    let scale = inst.pos_scale.w;
    let angle = inst.color_rotation.w;
    let base_pos = inst.pos_scale.xyz;

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
    out.color = inst.color_rotation.xyz;
    out.bend_factor = in.bend;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Bias normal upward for lighting: grass should catch sunlight like terrain,
    // not be dark from horizontal face normals. Blend 60% up + 40% face normal.
    let face_n = compute_flat_normal(in.world_pos);
    let n = normalize(mix(face_n, vec3(0.0, 1.0, 0.0), 0.6));

    // Soft base-to-tip gradient: subtle root shadow, bright sunlit tips
    let grad = smoothstep(0.0, 0.3, in.bend_factor);
    var blade_color = in.color * (0.90 + 0.20 * grad);  // base at 90%, tip at 110%
    // Gate tip color on sun elevation: noon = green tips, dawn/dusk = warm amber tips
    let sun_elev = smoothstep(0.1, 0.4, u.sun_dir.y);  // 0 at horizon, 1 at high sun
    let sun_warmth = dot(u.sun_color, vec3(0.333));
    let day_tip = smoothstep(0.3, 0.8, sun_warmth);
    let tip_glow = smoothstep(0.3, 1.0, in.bend_factor);
    // At noon (high sun): more green, less red. At dawn/dusk (low sun): more red, less green.
    let tip_green = mix(0.03, 0.10, sun_elev);  // dawn: subtle, noon: bright green
    let tip_red = mix(0.04, 0.02, sun_elev);     // dawn: warm amber, noon: minimal red
    blade_color.g += tip_glow * tip_green * day_tip;
    blade_color.r += tip_glow * tip_red * day_tip;

    let shadow = sample_shadow(in.world_pos);
    let lit = hemisphere_lighting(n, blade_color, shadow, in.world_pos);

    // Translucency: backlit blades glow warm when sun is behind them (dawn/dusk)
    // Gate on sun elevation — no translucency at night
    let sun_up = smoothstep(-0.05, 0.1, u.sun_dir.y);
    let view_dir = normalize(in.world_pos - u.camera_pos);
    let backlit = max(dot(view_dir, u.sun_dir), 0.0);
    let translucency = backlit * backlit * in.bend_factor * sun_up * 0.5;
    let trans_color = blade_color * u.sun_color * translucency;

    // Slight brightness lift so blades stand out from terrain under heavy atmosphere
    let lift = blade_color * 0.04 * sun_up;

    let color = apply_fog(in.world_pos, lit + trans_color + lift);
    return vec4(color, 1.0);
}
