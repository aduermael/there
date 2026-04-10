// Shared uniforms, lighting, fog, and noise functions.
// Concatenated as a prefix to all geometry + sky shaders via Rust-side format!().

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
    sun_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

// --- Hash-based value noise ---

fn hash2(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, vec3(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33));
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);

    let a = hash2(i);
    let b = hash2(i + vec2(1.0, 0.0));
    let c = hash2(i + vec2(0.0, 1.0));
    let d = hash2(i + vec2(1.0, 1.0));

    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

fn fbm3(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    // 3 octaves
    val += amp * value_noise(pos); pos *= 2.03; amp *= 0.5;
    val += amp * value_noise(pos); pos *= 2.03; amp *= 0.5;
    val += amp * value_noise(pos);
    return val;
}

// --- Shared lighting and fog (fragment-only) ---

fn compute_flat_normal(world_pos: vec3<f32>) -> vec3<f32> {
    let dx = dpdx(world_pos);
    let dy = dpdy(world_pos);
    return normalize(cross(dx, dy));
}

fn sample_shadow(world_pos: vec3<f32>) -> f32 {
    let light_clip = u.sun_view_proj * vec4(world_pos, 1.0);
    let light_ndc = light_clip.xyz / light_clip.w;
    let shadow_uv = vec2(light_ndc.x * 0.5 + 0.5, 1.0 - (light_ndc.y * 0.5 + 0.5));

    // Out of shadow map bounds = fully lit
    if shadow_uv.x < 0.0 || shadow_uv.x > 1.0 || shadow_uv.y < 0.0 || shadow_uv.y > 1.0 {
        return 1.0;
    }

    let current_depth = light_ndc.z;
    let bias = 0.003;
    let d = current_depth - bias;

    // 4-tap PCF: each comparison sample gets hardware bilinear, giving ~4x4 coverage
    let texel = 1.0 / 1024.0;
    let s = texel * 1.2;
    let shadow = (
        textureSampleCompare(shadow_map, shadow_sampler, shadow_uv + vec2(-s, -s), d)
        + textureSampleCompare(shadow_map, shadow_sampler, shadow_uv + vec2( s, -s), d)
        + textureSampleCompare(shadow_map, shadow_sampler, shadow_uv + vec2(-s,  s), d)
        + textureSampleCompare(shadow_map, shadow_sampler, shadow_uv + vec2( s,  s), d)
    ) * 0.25;
    return shadow;
}

fn hemisphere_lighting(n: vec3<f32>, base_color: vec3<f32>, shadow: f32) -> vec3<f32> {
    let hemi_t = dot(n, vec3(0.0, 1.0, 0.0)) * 0.5 + 0.5;
    let ambient = mix(u.ground_ambient, u.sky_ambient, hemi_t);
    let ndl = max(dot(n, u.sun_dir), 0.0);
    // Shadow only affects direct sun light, not ambient
    let sun_shadow = mix(0.05, 1.0, shadow); // shadowed areas keep 5% sun
    return base_color * (ambient + ndl * u.sun_color * sun_shadow);
}

fn rim_light(n: vec3<f32>, world_pos: vec3<f32>) -> vec3<f32> {
    let view_dir = normalize(u.camera_pos - world_pos);
    let fresnel = pow(1.0 - max(dot(n, view_dir), 0.0), 3.0);
    return fresnel * u.sky_ambient * 0.8;
}

fn apply_fog(world_pos: vec3<f32>, lit_color: vec3<f32>) -> vec3<f32> {
    let dist = length(world_pos - u.camera_pos);
    let avg_height = (world_pos.y + u.camera_pos.y) * 0.5;
    let height_atten = exp(-u.fog_height_falloff * max(avg_height, 0.0));
    let fog = clamp(1.0 - exp(-dist * u.fog_density * height_atten), 0.0, 1.0);

    let far_blend = smoothstep(0.3, 0.9, fog);
    let atmo_fog_color = mix(u.fog_color, u.sky_zenith, far_blend * 0.35);
    return mix(lit_color, atmo_fog_color, fog);
}
