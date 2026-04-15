// Shared lighting and fog functions.
// Concatenated as a prefix to all geometry + sky shaders via Rust-side format!().
// Uniforms struct from uniforms.wgsl, noise functions from noise.wgsl (prepended before this).

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
@group(1) @binding(2) var cloud_shadow_tex: texture_2d<f32>;
@group(1) @binding(3) var cloud_shadow_samp: sampler;

// Shared cloud layer parameters — used by sky rendering and cloud shadow computation.
// Each layer: altitude (world Y), scale (domain size), coverage (noise threshold),
//             opacity (visual density), drift_mult (wind speed multiplier).
const CLOUD_HIGH_ALTITUDE: f32 = 220.0;
const CLOUD_HIGH_SCALE: f32 = 700.0;
const CLOUD_HIGH_COVERAGE: f32 = 0.38;
const CLOUD_HIGH_OPACITY: f32 = 0.5;
const CLOUD_HIGH_DRIFT: f32 = 1.3;

const CLOUD_MID_ALTITUDE: f32 = 120.0;
const CLOUD_MID_SCALE: f32 = 500.0;
const CLOUD_MID_COVERAGE: f32 = 0.35;
const CLOUD_MID_OPACITY: f32 = 1.0;
const CLOUD_MID_DRIFT: f32 = 1.0;

const CLOUD_LOW_ALTITUDE: f32 = 80.0;
const CLOUD_LOW_SCALE: f32 = 350.0;
const CLOUD_LOW_COVERAGE: f32 = 0.42;
const CLOUD_LOW_OPACITY: f32 = 0.85;
const CLOUD_LOW_DRIFT: f32 = 0.7;

fn cloud_drift(drift_mult: f32) -> vec2<f32> {
    return vec2(u.time * 6.0, u.time * 2.0) * drift_mult;
}

fn sample_shadow(world_pos: vec3<f32>, normal: vec3<f32>) -> f32 {
    let dist = length(world_pos - u.camera_pos);

    // Select cascade based on distance from camera
    // Avoid early returns so textureSampleCompare stays in uniform control flow.
    var cascade: i32 = 0;
    var light_vp: mat4x4<f32> = u.cascade_vp0;
    var in_range = true;

    if dist < u.cascade_splits.x {
        cascade = 0;
        light_vp = u.cascade_vp0;
    } else if dist < u.cascade_splits.y {
        cascade = 1;
        light_vp = u.cascade_vp1;
    } else if dist < u.cascade_splits.z {
        cascade = 2;
        light_vp = u.cascade_vp2;
    } else {
        in_range = false; // beyond shadow distance
    }

    let light_clip = light_vp * vec4(world_pos, 1.0);
    let light_ndc = light_clip.xyz / light_clip.w;
    let shadow_uv = vec2(light_ndc.x * 0.5 + 0.5, 1.0 - (light_ndc.y * 0.5 + 0.5));

    // Out of shadow map bounds = fully lit
    if shadow_uv.x < 0.0 || shadow_uv.x > 1.0 || shadow_uv.y < 0.0 || shadow_uv.y > 1.0 {
        in_range = false;
    }

    let current_depth = light_ndc.z;
    // Slope-scaled bias: steeper angles to the sun get more bias to prevent acne
    let ndotl = max(dot(normal, u.sun_dir), 0.0);
    let bias = max(0.005 * (1.0 - ndotl), 0.001);
    let d = current_depth - bias;

    // 8-tap rotated Poisson disk PCF
    // Poisson disk sample offsets within unit circle
    const POISSON: array<vec2<f32>, 8> = array(
        vec2(-0.326, -0.406),
        vec2(-0.840, -0.074),
        vec2(-0.696,  0.457),
        vec2(-0.203,  0.621),
        vec2( 0.962, -0.195),
        vec2( 0.473, -0.480),
        vec2( 0.519,  0.767),
        vec2( 0.185, -0.893),
    );

    // Per-pixel rotation angle via IGN (breaks up regular pattern for smoother edges)
    let clip = u.view_proj * vec4(world_pos, 1.0);
    let screen_pos = clip.xy / clip.w * 512.0;
    let angle = ign(screen_pos) * TAU;
    let cs = cos(angle);
    let sn = sin(angle);

    let texel = 1.0 / 1024.0;
    let radius = texel * 2.5;

    // Always sample (uniform control flow), select result afterward
    var shadow = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let offset = vec2(
            POISSON[i].x * cs - POISSON[i].y * sn,
            POISSON[i].x * sn + POISSON[i].y * cs,
        ) * radius;
        shadow += textureSampleCompare(shadow_map, shadow_sampler, shadow_uv + offset, cascade, d);
    }

    // If out of range, return fully lit; otherwise return PCF result
    return select(shadow * 0.125, 1.0, !in_range);
}

fn sample_cloud_shadow(world_pos: vec3<f32>) -> f32 {
    let uv = world_pos.xz / u.world_size;
    return textureSampleLevel(cloud_shadow_tex, cloud_shadow_samp, uv, 0.0).r;
}

fn hemisphere_lighting(n: vec3<f32>, base_color: vec3<f32>, shadow: f32, world_pos: vec3<f32>) -> vec3<f32> {
    // Compressed hemisphere blend: every surface gets some ground bounce (warm fill)
    let hemi_t = dot(n, vec3(0.0, 1.0, 0.0)) * 0.35 + 0.5;
    let ambient = mix(u.ground_ambient, u.sky_ambient, hemi_t);
    let ndl = max(dot(n, u.sun_dir), 0.0);
    // Shadow map + baked cloud shadow combined on direct sun
    let cloud_s = sample_cloud_shadow(world_pos);
    let sun_shadow = mix(0.05, 1.0, shadow * cloud_s);
    return base_color * (ambient + ndl * u.sun_color * sun_shadow);
}

fn apply_fog(world_pos: vec3<f32>, lit_color: vec3<f32>) -> vec3<f32> {
    let dist = length(world_pos - u.camera_pos);
    let avg_height = (world_pos.y + u.camera_pos.y) * 0.5;
    let height_atten = exp(-u.fog_height_falloff * max(avg_height, 0.0));
    let raw_fog = clamp(1.0 - exp(-dist * u.fog_density * height_atten), 0.0, 1.0);

    // Power curve: preserves material colors at near/mid range, fog only strong at distance
    let fog = pow(raw_fog, 1.5);

    // View-dependent fog: warm haze toward sun, cool away
    let view_dir = normalize(world_pos - u.camera_pos);
    let sun_align = max(dot(view_dir, u.sun_dir), 0.0);
    let sun_haze = pow(sun_align, 3.0) * 0.45;
    let base_fog_color = mix(u.fog_color, u.sun_color, sun_haze);

    let far_blend = smoothstep(0.3, 0.9, fog);
    let atmo_fog_color = mix(base_fog_color, u.sky_zenith, far_blend * 0.35);
    return mix(lit_color, atmo_fog_color, fog);
}
