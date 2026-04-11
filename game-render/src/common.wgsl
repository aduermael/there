// Shared lighting and fog functions.
// Concatenated as a prefix to all geometry + sky shaders via Rust-side format!().
// Uniforms struct from uniforms.wgsl, noise functions from noise.wgsl (prepended before this).

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

fn compute_flat_normal(world_pos: vec3<f32>) -> vec3<f32> {
    let dx = dpdx(world_pos);
    let dy = dpdy(world_pos);
    return normalize(cross(dx, dy));
}

fn sample_shadow(world_pos: vec3<f32>) -> f32 {
    let dist = length(world_pos - u.camera_pos);

    // Select cascade based on distance from camera
    var cascade: i32;
    var light_vp: mat4x4<f32>;

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
        return 1.0; // beyond shadow distance
    }

    let light_clip = light_vp * vec4(world_pos, 1.0);
    let light_ndc = light_clip.xyz / light_clip.w;
    let shadow_uv = vec2(light_ndc.x * 0.5 + 0.5, 1.0 - (light_ndc.y * 0.5 + 0.5));

    // Out of shadow map bounds = fully lit
    if shadow_uv.x < 0.0 || shadow_uv.x > 1.0 || shadow_uv.y < 0.0 || shadow_uv.y > 1.0 {
        return 1.0;
    }

    let current_depth = light_ndc.z;
    let bias = 0.003;
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
    let angle = ign(screen_pos) * 6.283185;
    let cs = cos(angle);
    let sn = sin(angle);

    let texel = 1.0 / 1024.0;
    let radius = texel * 2.5;

    var shadow = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let offset = vec2(
            POISSON[i].x * cs - POISSON[i].y * sn,
            POISSON[i].x * sn + POISSON[i].y * cs,
        ) * radius;
        shadow += textureSampleCompare(shadow_map, shadow_sampler, shadow_uv + offset, cascade, d);
    }
    return shadow * 0.125;
}

fn cloud_shadow(world_pos: vec3<f32>) -> f32 {
    // Project world position onto cloud plane along sun direction
    let cloud_altitude = 120.0;
    let t = (cloud_altitude - world_pos.y) / max(u.sun_dir.y, 0.001);
    let cloud_xz = world_pos.xz + u.sun_dir.xz * t;

    // Same noise parameters as sky.wgsl clouds for visual coherence
    let cloud_scale = 500.0;
    let drift = vec2(u.time * 6.0, u.time * 2.0);
    let sample_pos = (cloud_xz + drift) / cloud_scale;

    var density = fbm3(sample_pos);
    let coverage = 0.35;
    density = smoothstep(coverage, coverage + 0.25, density);

    // Soften cloud shadows — they shouldn't be as harsh as geometry shadows
    return 1.0 - density * 0.45;
}

fn hemisphere_lighting(n: vec3<f32>, base_color: vec3<f32>, shadow: f32, world_pos: vec3<f32>) -> vec3<f32> {
    // Compressed hemisphere blend: every surface gets some ground bounce (warm fill)
    let hemi_t = dot(n, vec3(0.0, 1.0, 0.0)) * 0.35 + 0.5;
    let ambient = mix(u.ground_ambient, u.sky_ambient, hemi_t);
    let ndl = max(dot(n, u.sun_dir), 0.0);
    // Shadow map + cloud shadow combined on direct sun
    let cloud_s = cloud_shadow(world_pos);
    let sun_shadow = mix(0.05, 1.0, shadow * cloud_s);
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
