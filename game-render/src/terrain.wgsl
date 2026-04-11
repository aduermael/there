// Terrain-specific: heightmap sampling, height-based coloring, procedural noise, slope detail.
// Uniforms, noise, lighting, and fog provided by common.wgsl prefix.

struct ChunkOffset {
    offset: vec2<f32>,
};

@group(1) @binding(0) var heightmap: texture_2d<f32>;
@group(2) @binding(0) var<uniform> chunk: ChunkOffset;
@group(3) @binding(0) var shadow_map: texture_depth_2d;
@group(3) @binding(1) var shadow_sampler: sampler_comparison;

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

@vertex
fn vs_shadow(@location(0) local_xz: vec2<f32>) -> @builtin(position) vec4<f32> {
    let pos_xz = local_xz + chunk.offset;
    let uv = pos_xz / u.world_size;
    let tc = vec2<i32>(uv * u.hm_res);
    let h = get_height(tc.x, tc.y);
    return u.sun_view_proj * vec4(pos_xz.x, h, pos_xz.y, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let h = in.world_pos.y;
    let world_xz = in.world_pos.xz;

    // Height-based coloring: sand -> grass -> rock
    let sand  = vec3(0.38, 0.42, 0.22);
    let grass = vec3(0.28, 0.52, 0.18);
    let rock  = vec3(0.48, 0.43, 0.38);

    let sg = smoothstep(4.0, 10.0, h);
    let gr = smoothstep(18.0, 24.0, h);
    var base_color = mix(mix(sand, grass, sg), rock, gr);

    // Procedural color noise: large patches + fine detail
    let n_large = value_noise(world_xz * 0.12) * 2.0 - 1.0;
    let n_fine = value_noise(world_xz * 0.45) * 2.0 - 1.0;
    let n_med = value_noise(world_xz * 0.25 + vec2(37.0, 91.0)) * 2.0 - 1.0;

    let noise = n_large * 0.6 + n_med * 0.25 + n_fine * 0.15;

    // Per-biome hue/brightness shifts
    let grass_hue_shift = vec3(0.04, -0.02, -0.03) * n_large + vec3(-0.02, 0.03, -0.01) * n_med;
    let grass_bright = noise * 0.12;

    let sand_hue_shift = vec3(0.03, 0.01, -0.04) * n_large + vec3(-0.02, -0.01, 0.03) * n_med;
    let sand_bright = noise * 0.10;

    let rock_hue_shift = vec3(0.02, -0.01, 0.03) * n_large + vec3(-0.01, 0.02, -0.02) * n_fine;
    let rock_bright = noise * 0.08;

    let hue_shift = mix(mix(sand_hue_shift, grass_hue_shift, sg), rock_hue_shift, gr);
    let brightness = mix(mix(sand_bright, grass_bright, sg), rock_bright, gr);

    base_color = base_color + hue_shift + base_color * brightness;

    // Slope-based darkening and color shift
    let n = normalize(in.normal);
    let slope = 1.0 - n.y;
    let slope_factor = smoothstep(0.15, 0.7, slope);

    let steep_grass = mix(base_color, vec3(0.28, 0.36, 0.18), slope_factor * 0.6 * sg * (1.0 - gr));
    let steep_sand = mix(base_color, vec3(0.55, 0.50, 0.42), slope_factor * 0.4 * (1.0 - sg));
    let steep_rock = mix(base_color, vec3(0.38, 0.34, 0.32), slope_factor * 0.5 * gr);
    base_color = steep_grass + steep_sand + steep_rock - base_color * 2.0;

    let flat_boost = (1.0 - slope_factor) * 0.08 * sg * (1.0 - gr);
    base_color = base_color * (1.0 + flat_boost) + vec3(-0.01, 0.02, -0.01) * flat_boost;

    base_color = max(base_color, vec3(0.02));

    let shadow = sample_shadow(in.world_pos);
    let lit = hemisphere_lighting(n, base_color, shadow);
    let rim = rim_light(n, in.world_pos);
    let color = apply_fog(in.world_pos, lit + rim);

    return vec4(color, 1.0);
}
