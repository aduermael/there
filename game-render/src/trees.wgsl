// Tree-specific: instanced cone+cylinder with wind sway.
// Uniforms, lighting, and fog provided by common.wgsl prefix.

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) vert_color: vec3<f32>,
    @location(2) inst_pos_scale: vec4<f32>,
    @location(3) inst_foliage_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let scale = in.inst_pos_scale.w;
    var local_pos = in.position * scale;

    // Wind sway: foliage vertices displaced based on height above ground.
    let is_foliage = step(2.0, in.vert_color.r + in.vert_color.g + in.vert_color.b);

    // Crown shape variation: narrow-to-wide per instance
    let shape = in.inst_foliage_color.a;
    let crown_w = mix(0.75, 1.4, shape);
    local_pos.x *= mix(1.0, crown_w, is_foliage);
    local_pos.z *= mix(1.0, crown_w, is_foliage);

    let height_factor = saturate(in.position.y / 2.5);
    let sway_strength = is_foliage * height_factor * height_factor;

    let tree_pos = in.inst_pos_scale.xyz;
    let phase = tree_pos.x * 0.73 + tree_pos.z * 1.37;

    let wind_x = sin(u.time * 0.8 + phase) * 0.2 + sin(u.time * 1.9 + phase * 2.1) * 0.07;
    let wind_z = sin(u.time * 0.6 + phase * 1.5) * 0.08;

    local_pos.x += wind_x * sway_strength * scale;
    local_pos.z += wind_z * sway_strength * scale;

    let world_pos = local_pos + in.inst_pos_scale.xyz;

    let color = in.vert_color * in.inst_foliage_color.rgb;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = color;
    return out;
}

@vertex
fn vs_shadow(
    @location(0) position: vec3<f32>,
    @location(1) vert_color: vec3<f32>,
    @location(2) inst_pos_scale: vec4<f32>,
    @location(3) inst_foliage_color: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    let scale = inst_pos_scale.w;
    var local_pos = position * scale;

    // Wind sway (must match scene VS so shadows align)
    let is_foliage = step(2.0, vert_color.r + vert_color.g + vert_color.b);

    // Crown shape variation (must match scene VS)
    let shape = inst_foliage_color.a;
    let crown_w = mix(0.75, 1.4, shape);
    local_pos.x *= mix(1.0, crown_w, is_foliage);
    local_pos.z *= mix(1.0, crown_w, is_foliage);

    let height_factor = saturate(position.y / 2.5);
    let sway_strength = is_foliage * height_factor * height_factor;
    let tree_pos = inst_pos_scale.xyz;
    let phase = tree_pos.x * 0.73 + tree_pos.z * 1.37;
    let wind_x = sin(u.time * 0.8 + phase) * 0.2 + sin(u.time * 1.9 + phase * 2.1) * 0.07;
    let wind_z = sin(u.time * 0.6 + phase * 1.5) * 0.08;
    local_pos.x += wind_x * sway_strength * scale;
    local_pos.z += wind_z * sway_strength * scale;

    let world_pos = local_pos + inst_pos_scale.xyz;
    return u.sun_view_proj * vec4(world_pos, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = compute_flat_normal(in.world_pos);
    let shadow = sample_shadow(in.world_pos);
    let lit = hemisphere_lighting(n, in.color, shadow);
    let rim = rim_light(n, in.world_pos);
    let color = apply_fog(in.world_pos, lit + rim);
    return vec4(color, 1.0);
}
