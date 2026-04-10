struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_dir: vec3<f32>,
    fog_color: vec3<f32>,
    fog_density: f32,
    world_size: f32,
    hm_res: f32,
    fog_height_falloff: f32,
    time: f32,
    sun_color: vec3<f32>,
    sky_zenith: vec3<f32>,
    sky_horizon: vec3<f32>,
    inv_view_proj: mat4x4<f32>,
    sky_ambient: vec3<f32>,
    ground_ambient: vec3<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

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
    // Trunk verts have brown color (sum < 2.0), foliage verts are near-white (sum >= 2.0).
    let is_foliage = step(2.0, in.vert_color.r + in.vert_color.g + in.vert_color.b);
    let height_factor = saturate(in.position.y / 2.5); // 0 at base, 1 at crown tip
    let sway_strength = is_foliage * height_factor * height_factor; // quadratic falloff

    // Per-tree phase offset from world position
    let tree_pos = in.inst_pos_scale.xyz;
    let phase = tree_pos.x * 0.73 + tree_pos.z * 1.37;

    // Two-frequency wind: slow primary sway + faster secondary flutter
    let wind_x = sin(u.time * 0.8 + phase) * 0.2 + sin(u.time * 1.9 + phase * 2.1) * 0.07;
    let wind_z = sin(u.time * 0.6 + phase * 1.5) * 0.08;

    local_pos.x += wind_x * sway_strength * scale;
    local_pos.z += wind_z * sway_strength * scale;

    let world_pos = local_pos + in.inst_pos_scale.xyz;

    // Modulate white foliage verts by instance color; trunk verts pass through
    let color = in.vert_color * in.inst_foliage_color.rgb;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Flat shading via screen-space derivatives
    let dx = dpdx(in.world_pos);
    let dy = dpdy(in.world_pos);
    let n = normalize(cross(dx, dy));

    // Hemisphere ambient
    let hemi_t = dot(n, vec3(0.0, 1.0, 0.0)) * 0.5 + 0.5;
    let ambient = mix(u.ground_ambient, u.sky_ambient, hemi_t);

    let ndl = max(dot(n, u.sun_dir), 0.0);
    let lit = in.color * (ambient + ndl * u.sun_color);

    // Rim/fresnel lighting for silhouette definition
    let view_dir = normalize(u.camera_pos - in.world_pos);
    let fresnel = pow(1.0 - max(dot(n, view_dir), 0.0), 3.0);
    let rim = fresnel * u.sky_ambient * 0.8;

    // Exponential height fog
    let dist = length(in.world_pos - u.camera_pos);
    let avg_height = (in.world_pos.y + u.camera_pos.y) * 0.5;
    let height_atten = exp(-u.fog_height_falloff * max(avg_height, 0.0));
    let fog = clamp(1.0 - exp(-dist * u.fog_density * height_atten), 0.0, 1.0);

    // Atmospheric color shift: far objects fade toward sky blue
    let far_blend = smoothstep(0.3, 0.9, fog);
    let atmo_fog_color = mix(u.fog_color, u.sky_zenith, far_blend * 0.35);
    let color = mix(lit + rim, atmo_fog_color, fog);

    return vec4(color, 1.0);
}
