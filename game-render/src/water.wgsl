// Water surface rendering.
// Uniforms, noise, lighting, fog, and shadow bindings provided by common.wgsl prefix.

@group(2) @binding(0) var heightmap: texture_2d<f32>;

const WATER_LEVEL: f32 = 8.0;
const SHALLOW_COLOR: vec3<f32> = vec3(0.08, 0.45, 0.42);
const DEEP_COLOR: vec3<f32> = vec3(0.02, 0.06, 0.18);
const FOAM_COLOR: vec3<f32> = vec3(0.8, 0.85, 0.9);
const DEPTH_MAX: f32 = 8.0;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
};

@vertex
fn vs_main(@location(0) local_xz: vec2<f32>) -> VertexOutput {
    // Gentle vertex displacement for wave geometry
    let wave_pos = local_xz * 0.12 + vec2(u.time * 0.6, u.time * 0.35);
    let wave = fbm3(wave_pos) * 0.3 - 0.15;

    let world_pos = vec3(local_xz.x, WATER_LEVEL + wave, local_xz.y);

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Water column depth from heightmap (avoids depth texture read-back issues on Safari)
    let tc = vec2<i32>(in.world_pos.xz / u.world_size * u.hm_res);
    let res = i32(u.hm_res);
    let terrain_h = textureLoad(heightmap, clamp(tc, vec2(0), vec2(res - 1)), 0).r;
    let water_depth = max(in.world_pos.y - terrain_h, 0.0);
    let depth_factor = clamp(water_depth / DEPTH_MAX, 0.0, 1.0);

    // Animated surface normal from FBM derivatives (single layer, varied offset)
    let eps = 0.3;
    let base = in.world_pos.xz;
    let ws = 0.20;
    let wo = vec2(u.time * 0.7, u.time * 0.4);

    let h_c = fbm3(base * ws + wo);
    let h_r = fbm3((base + vec2(eps, 0.0)) * ws + wo);
    let h_u = fbm3((base + vec2(0.0, eps)) * ws + wo);

    let dx = h_c - h_r;
    let dz = h_c - h_u;
    let n = normalize(vec3(dx, eps * 1.5, dz));

    // View direction
    let view_dir = normalize(u.camera_pos - in.world_pos);

    // Fresnel (Schlick, F0 = 0.04 for water)
    let n_dot_v = max(dot(n, view_dir), 0.0);
    let fresnel = 0.04 + 0.96 * pow(1.0 - n_dot_v, 5.0);

    // Sky reflection
    let reflect_dir = reflect(-view_dir, n);
    let sky_t = pow(1.0 - max(reflect_dir.y, 0.0), 2.0);
    let sky_reflect = mix(u.sky_zenith, u.sky_horizon, sky_t);

    // Sun specular (Blinn-Phong)
    let half_vec = normalize(view_dir + u.sun_dir);
    let spec = pow(max(dot(n, half_vec), 0.0), 256.0);
    let shadow = sample_shadow(in.world_pos, n);
    let cloud_s = sample_cloud_shadow(in.world_pos);
    let sun_vis = shadow * cloud_s;
    let sun_spec = u.sun_color * spec * 3.0 * sun_vis;

    // Depth-based water body color
    let water_body = mix(SHALLOW_COLOR, DEEP_COLOR, depth_factor);

    // Lit water body
    let ndl = max(dot(n, u.sun_dir), 0.0);
    let hemi_t = dot(n, vec3(0.0, 1.0, 0.0)) * 0.3 + 0.5;
    let ambient = mix(u.ground_ambient, u.sky_ambient, hemi_t);
    let lit_water = water_body * (ambient + ndl * u.sun_color * mix(0.1, 1.0, sun_vis));

    // Combine reflection and refraction via Fresnel
    let surface_color = mix(lit_water, sky_reflect, fresnel) + sun_spec;

    // Shoreline foam (single noise sample)
    let foam_uv = in.world_pos.xz * 0.5 + vec2(u.time * 0.8, u.time * 0.5);
    let foam_noise = fbm3(foam_uv);
    let foam_edge = smoothstep(0.0, 0.6, water_depth);
    let foam_mask = (1.0 - foam_edge) * smoothstep(0.25, 0.5, foam_noise);
    let foamy_surface = mix(surface_color, FOAM_COLOR * (ambient + u.sun_color * sun_vis * 0.5), foam_mask * 0.8);

    // Fog
    let final_color = apply_fog(in.world_pos, foamy_surface);

    // Alpha: more transparent in shallows, opaque in deep water
    let alpha = mix(0.55, 0.95, smoothstep(0.0, 1.0, depth_factor));

    return vec4(final_color, alpha);
}
