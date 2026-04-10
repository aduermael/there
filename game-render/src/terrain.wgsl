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

struct ChunkOffset {
    offset: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var heightmap: texture_2d<f32>;
@group(2) @binding(0) var<uniform> chunk: ChunkOffset;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

fn get_height(ix: i32, iz: i32) -> f32 {
    let res = i32(u.hm_res);
    return textureLoad(heightmap, clamp(vec2(ix, iz), vec2(0), vec2(res - 1)), 0).r;
}

@vertex
fn vs_main(@location(0) local_xz: vec2<f32>) -> VertexOutput {
    let pos_xz = local_xz + chunk.offset;
    let uv = pos_xz / u.world_size;
    let tc = vec2<i32>(uv * u.hm_res);

    let h = get_height(tc.x, tc.y);
    let world_pos = vec3(pos_xz.x, h, pos_xz.y);

    // Normal from finite differences (4-neighbor)
    let hL = get_height(tc.x - 1, tc.y);
    let hR = get_height(tc.x + 1, tc.y);
    let hD = get_height(tc.x, tc.y - 1);
    let hU = get_height(tc.x, tc.y + 1);
    let step = u.world_size / u.hm_res;
    let normal = normalize(vec3(hL - hR, 2.0 * step, hD - hU));

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.normal = normal;
    return out;
}

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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let h = in.world_pos.y;
    let world_xz = in.world_pos.xz;

    // Height-based coloring: sand → grass → rock
    let sand  = vec3(0.76, 0.70, 0.50);
    let grass = vec3(0.32, 0.54, 0.22);
    let rock  = vec3(0.50, 0.45, 0.40);

    let sg = smoothstep(8.0, 14.0, h);
    let gr = smoothstep(18.0, 24.0, h);
    var base_color = mix(mix(sand, grass, sg), rock, gr);

    // Procedural color noise: large patches + fine detail
    // Large scale: patches of color variation (~8m wavelength)
    let n_large = value_noise(world_xz * 0.12) * 2.0 - 1.0;
    // Fine scale: per-meter variation
    let n_fine = value_noise(world_xz * 0.45) * 2.0 - 1.0;
    // Medium scale: in-between patches (~3m)
    let n_med = value_noise(world_xz * 0.25 + vec2(37.0, 91.0)) * 2.0 - 1.0;

    // Combined noise: large patches dominate, fine adds texture
    let noise = n_large * 0.6 + n_med * 0.25 + n_fine * 0.15;

    // Per-biome hue/brightness shifts
    // Grass: warm green ↔ cool green, hints of yellow/brown
    let grass_hue_shift = vec3(0.04, -0.02, -0.03) * n_large + vec3(-0.02, 0.03, -0.01) * n_med;
    let grass_bright = noise * 0.12;

    // Sand: warm sand ↔ cooler grey-sand
    let sand_hue_shift = vec3(0.03, 0.01, -0.04) * n_large + vec3(-0.02, -0.01, 0.03) * n_med;
    let sand_bright = noise * 0.10;

    // Rock: grey ↔ brown ↔ slight blue-grey
    let rock_hue_shift = vec3(0.02, -0.01, 0.03) * n_large + vec3(-0.01, 0.02, -0.02) * n_fine;
    let rock_bright = noise * 0.08;

    // Blend shifts based on biome zone
    let hue_shift = mix(mix(sand_hue_shift, grass_hue_shift, sg), rock_hue_shift, gr);
    let brightness = mix(mix(sand_bright, grass_bright, sg), rock_bright, gr);

    base_color = base_color + hue_shift + base_color * brightness;

    // Slope-based darkening and color shift
    let n = normalize(in.normal);
    let slope = 1.0 - n.y; // 0 = flat, 1 = vertical
    let slope_factor = smoothstep(0.15, 0.7, slope);

    // Steep grass → darker, browner (exposed soil/dirt)
    let steep_grass = mix(base_color, vec3(0.28, 0.36, 0.18), slope_factor * 0.6 * sg * (1.0 - gr));
    // Steep sand → slightly darker, more grey
    let steep_sand = mix(base_color, vec3(0.55, 0.50, 0.42), slope_factor * 0.4 * (1.0 - sg));
    // Steep rock → darker crevices
    let steep_rock = mix(base_color, vec3(0.38, 0.34, 0.32), slope_factor * 0.5 * gr);
    base_color = steep_grass + steep_sand + steep_rock - base_color * 2.0;

    // Flat areas: slightly brighter, more saturated (lush growth)
    let flat_boost = (1.0 - slope_factor) * 0.08 * sg * (1.0 - gr);
    base_color = base_color * (1.0 + flat_boost) + vec3(-0.01, 0.02, -0.01) * flat_boost;

    base_color = max(base_color, vec3(0.02));

    // Hemisphere ambient: sky from above, ground bounce from below
    let hemi_t = dot(n, vec3(0.0, 1.0, 0.0)) * 0.5 + 0.5;
    let ambient = mix(u.ground_ambient, u.sky_ambient, hemi_t);

    // Directional light (sun)
    let ndl = max(dot(n, u.sun_dir), 0.0);
    let lit = base_color * (ambient + ndl * u.sun_color);

    // Rim/fresnel lighting for silhouette definition
    let view_dir = normalize(u.camera_pos - in.world_pos);
    let fresnel = pow(1.0 - max(dot(n, view_dir), 0.0), 3.0);
    let rim = fresnel * u.sky_ambient * 0.8;

    // Exponential height fog: denser in valleys, thinner at altitude
    let dist = length(in.world_pos - u.camera_pos);
    let avg_height = (in.world_pos.y + u.camera_pos.y) * 0.5;
    let height_atten = exp(-u.fog_height_falloff * max(avg_height, 0.0));
    let fog = clamp(1.0 - exp(-dist * u.fog_density * height_atten), 0.0, 1.0);

    // Atmospheric color shift: near fog matches horizon, far fog shifts toward zenith blue
    let far_blend = smoothstep(0.3, 0.9, fog);
    let atmo_fog_color = mix(u.fog_color, u.sky_zenith, far_blend * 0.35);
    let color = mix(lit + rim, atmo_fog_color, fog);

    return vec4(color, 1.0);
}
