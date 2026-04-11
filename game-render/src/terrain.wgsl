// Terrain-specific: heightmap sampling, height-based coloring, procedural noise, slope detail.
// Uniforms, noise, lighting, fog, and shadow bindings provided by common.wgsl prefix.

struct ChunkOffset {
    offset: vec2<f32>,
};

@group(2) @binding(0) var heightmap: texture_2d<f32>;
@group(2) @binding(1) var atlas: texture_2d_array<f32>;
@group(2) @binding(2) var atlas_sampler: sampler;
@group(3) @binding(0) var<uniform> chunk: ChunkOffset;

// Material layer indices (must match Rust MAT_* constants)
const MAT_GRASS: i32 = 0;
const MAT_DIRT: i32 = 1;
const MAT_SAND: i32 = 2;
const MAT_ROCK: i32 = 3;

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

/// Sample a material tile from the atlas using world-space tiling.
fn sample_material(world_xz: vec2<f32>, layer: i32) -> vec3<f32> {
    // ~1 tile per 2 world units → each 16px tile covers 2m
    let tile_uv = fract(world_xz * 0.5);
    return textureSample(atlas, atlas_sampler, tile_uv, layer).rgb;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let h = in.world_pos.y;
    let world_xz = in.world_pos.xz;

    // Normal
    let n = normalize(in.normal);
    let slope = 1.0 - n.y;

    // --- Material selection based on height and slope ---
    // Biome blend weights (same transitions as before)
    let sg = smoothstep(3.0, 8.0, h);    // sand → grass
    let gr = smoothstep(18.0, 24.0, h);  // grass → rock
    let slope_factor = smoothstep(0.15, 0.7, slope);

    // Sample material textures
    let tex_sand  = sample_material(world_xz, MAT_SAND);
    let tex_grass = sample_material(world_xz, MAT_GRASS);
    let tex_rock  = sample_material(world_xz, MAT_ROCK);
    let tex_dirt  = sample_material(world_xz, MAT_DIRT);

    // Height-based base material blend
    var tex_color = mix(mix(tex_sand, tex_grass, sg), tex_rock, gr);

    // Steep slopes blend toward dirt/rock
    let steep_blend = mix(tex_dirt, tex_rock, gr);
    tex_color = mix(tex_color, steep_blend, slope_factor * 0.6);

    // --- Large-scale procedural variation (biome patches) ---
    let n_large = value_noise(world_xz * 0.12) * 2.0 - 1.0;
    let n_med = value_noise(world_xz * 0.25 + vec2(37.0, 91.0)) * 2.0 - 1.0;

    // Per-biome hue shifts from large-scale noise
    let grass_hue_shift = vec3(0.04, -0.02, -0.03) * n_large + vec3(-0.02, 0.03, -0.01) * n_med;
    let sand_hue_shift = vec3(0.03, 0.01, -0.04) * n_large + vec3(-0.02, -0.01, 0.03) * n_med;
    let rock_hue_shift = vec3(0.02, -0.01, 0.03) * n_large + vec3(-0.01, 0.02, -0.02) * n_med;

    let hue_shift = mix(mix(sand_hue_shift, grass_hue_shift, sg), rock_hue_shift, gr);
    let brightness = (n_large * 0.6 + n_med * 0.25) * mix(mix(0.10, 0.12, sg), 0.08, gr);

    var base_color = tex_color + hue_shift + tex_color * brightness;

    // Grass-root green tint for distance blending with grass blades
    let grass_zone = sg * (1.0 - gr) * (1.0 - slope_factor);
    let cam_d = length(in.world_pos - u.camera_pos);
    let grass_tint_strength = grass_zone * smoothstep(30.0, 70.0, cam_d) * 0.06;
    base_color = base_color + vec3(-0.01, grass_tint_strength, -0.005);

    base_color = max(base_color, vec3(0.02));

    let shadow = sample_shadow(in.world_pos);
    let lit = hemisphere_lighting(n, base_color, shadow, in.world_pos);
    let rim = rim_light(n, in.world_pos);
    let color = apply_fog(in.world_pos, lit + rim);

    return vec4(color, 1.0);
}
