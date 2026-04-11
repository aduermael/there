// GPU-driven grass instance generation.
// Dispatched per-frame to populate a storage buffer with visible grass blade instances.
// Uses camera-centered world-aligned grid: only generates blades near the camera.

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

struct GrassInstance {
    pos_scale: vec4<f32>,
    color_rotation: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var heightmap: texture_2d<f32>;
@group(2) @binding(0) var<storage, read_write> instances: array<GrassInstance>;
@group(2) @binding(1) var<storage, read_write> draw_args: array<atomic<u32>, 5>;

const MAX_INSTANCES: u32 = 64000u;
const GRID_EXTENT: u32 = 384u; // 192 units at 0.5 spacing
const TAU: f32 = 6.28318530;

// --- Hash functions (matches scatter.rs / common.wgsl) ---

fn cell_hash(x: u32, z: u32, seed: u32) -> u32 {
    var h = seed;
    h = h + x * 0x9e3779b9u;
    h = h ^ (h >> 16u);
    h = h + z * 0x85ebca6bu;
    h = h ^ (h >> 13u);
    h = h * 0xc2b2ae35u;
    h = h ^ (h >> 16u);
    return h;
}

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

// --- Heightmap access ---

fn get_height(tc: vec2<i32>) -> f32 {
    let res = i32(u.hm_res);
    return textureLoad(heightmap, clamp(tc, vec2(0), vec2(res - 1)), 0).r;
}

fn get_height_world(wx: f32, wz: f32) -> f32 {
    let uv = vec2(wx, wz) / u.world_size;
    let tc = vec2<i32>(vec2<f32>(uv * u.hm_res));
    return get_height(tc);
}

fn compute_slope(tc: vec2<i32>) -> f32 {
    let hL = get_height(tc + vec2(-1, 0));
    let hR = get_height(tc + vec2(1, 0));
    let hD = get_height(tc + vec2(0, -1));
    let hU = get_height(tc + vec2(0, 1));
    let texel_size = u.world_size / u.hm_res;
    let dx = (hR - hL) / (2.0 * texel_size);
    let dz = (hU - hD) / (2.0 * texel_size);
    return sqrt(dx * dx + dz * dz);
}

// --- Terrain color (matches terrain.wgsl + scatter.rs) ---

fn terrain_color_at(h: f32, wx: f32, wz: f32) -> vec3<f32> {
    let sand = vec3(0.34, 0.29, 0.16);
    let grass_c = vec3(0.28, 0.46, 0.20);
    let rock = vec3(0.48, 0.43, 0.38);

    let sg = smoothstep(3.0, 8.0, h);
    let gr = smoothstep(18.0, 24.0, h);
    var base = mix(mix(sand, grass_c, sg), rock, gr);

    let p = vec2(wx, wz);
    let n_large = value_noise(p * 0.12) * 2.0 - 1.0;
    let n_med = value_noise(p * 0.25 + vec2(37.0, 91.0)) * 2.0 - 1.0;
    let n_fine = value_noise(p * 0.45) * 2.0 - 1.0;
    let noise = n_large * 0.6 + n_med * 0.25 + n_fine * 0.15;

    let grass_hue = vec3(0.04, -0.02, -0.03) * n_large + vec3(-0.02, 0.03, -0.01) * n_med;
    let grass_bright = noise * 0.12;
    let sand_hue = vec3(0.03, 0.01, -0.04) * n_large + vec3(-0.02, -0.01, 0.03) * n_med;
    let sand_bright = noise * 0.10;

    let hue = mix(sand_hue, grass_hue, sg);
    let brightness = mix(sand_bright, grass_bright, sg);
    base = base + hue + base * brightness;

    let flat_boost = 0.07 * sg * (1.0 - gr);
    base = base * (1.0 + flat_boost) + vec3(-0.01, 0.02, -0.01) * flat_boost;

    return max(base, vec3(0.02));
}

// --- Main compute kernel ---

@compute @workgroup_size(16, 16, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= GRID_EXTENT || gid.y >= GRID_EXTENT) { return; }

    let texel_size = u.world_size / u.hm_res;

    // Camera-centered, world-aligned grid origin (in heightmap texels)
    let half_extent = f32(GRID_EXTENT) * texel_size * 0.5; // 96 units
    let origin_x = u32(clamp((u.camera_pos.x - half_extent) / texel_size, 0.0, u.hm_res - 1.0));
    let origin_z = u32(clamp((u.camera_pos.z - half_extent) / texel_size, 0.0, u.hm_res - 1.0));
    let cell_x = origin_x + gid.x;
    let cell_z = origin_z + gid.y;

    // Bounds check
    if (cell_x >= u32(u.hm_res) || cell_z >= u32(u.hm_res)) { return; }

    // Deterministic hash for this world cell
    let hash = cell_hash(cell_x, cell_z, 0xCAFEu);

    // Jittered world position (fixed for a given cell)
    let jx = f32((hash >> 8u) & 0xFFu) / 255.0;
    let jz = f32((hash >> 16u) & 0xFFu) / 255.0;
    let wx = (f32(cell_x) + jx) * texel_size;
    let wz = (f32(cell_z) + jz) * texel_size;

    // Distance cull (cheap early-out)
    let cam_dist = length(vec2(wx, wz) - u.camera_pos.xz);
    if (cam_dist > 85.0) { return; }

    // Height from heightmap
    let tc = vec2<i32>(i32(cell_x), i32(cell_z));
    let h = get_height(tc);
    if (h < 1.0 || h > 17.0) { return; }

    // Slope filter
    let slope = compute_slope(tc);
    if (slope > 0.3) { return; }

    // Graduated zone density: very sparse scrub on sand, full meadow in grass zone
    // h 1-3: rare dried scrub sticking through sand
    // h 3-6: sparse transition
    // h 6-17: full density grass meadow
    let zone_density = smoothstep(1.0, 7.0, h); // 0 at h=1, 1 at h=7+

    // Patch-based density via low-frequency noise
    let patch_noise = value_noise(vec2(wx, wz) * 0.1);
    let in_patch = patch_noise > 0.30;
    // Scale threshold by zone_density: sand scrub is very rare, grass zone is dense
    let base_threshold = select(56u, 245u, in_patch);
    let threshold = u32(f32(base_threshold) * (0.08 + 0.92 * zone_density)); // 8% at h=1, 100% at h=7
    if ((hash & 0xFFu) > threshold) { return; }

    // Frustum cull (project root to clip space)
    // Use generous margin — blades extend upward from root, Y needs more slack
    let clip = u.view_proj * vec4(wx, h, wz, 1.0);
    if (clip.w < 0.1) { return; } // behind camera
    let ndc = clip.xy / clip.w;
    if (abs(ndc.x) > 1.5 || ndc.y < -2.0 || ndc.y > 1.5) { return; }

    // Distance-based blade count (LOD) — front-loaded for dense coverage
    var blade_count: u32;
    if (in_patch) {
        if (cam_dist < 15.0) { blade_count = 8u; }
        else if (cam_dist < 30.0) { blade_count = 5u; }
        else if (cam_dist < 50.0) { blade_count = 3u; }
        else if (cam_dist < 70.0) { blade_count = 2u; }
        else { blade_count = 1u; }
    } else {
        if (cam_dist < 15.0) { blade_count = 5u; }
        else if (cam_dist < 30.0) { blade_count = 3u; }
        else if (cam_dist < 50.0) { blade_count = 1u; }
        else { blade_count = 1u; }
    }
    // Scale blade count by zone density (sand scrub = 1 blade max)
    blade_count = max(u32(f32(blade_count) * zone_density + 0.5), 1u);

    // Terrain color at tuft center
    let terrain_col = terrain_color_at(h, wx, wz);
    let patch_green = select(0.0, 0.03, in_patch);

    // Spawn blade tuft
    for (var bi = 0u; bi < blade_count; bi++) {
        let blade_hash = cell_hash(bi, hash, 0xB1ADu + bi);

        // Offset from tuft center — wider spread for fluffy volume
        let angle = f32(blade_hash & 0xFFu) / 255.0 * TAU;
        var offset_dist = 0.0;
        if (bi > 0u) {
            offset_dist = 0.05 + f32((blade_hash >> 8u) & 0xFFu) / 255.0 * 0.30;
        }
        let bx = wx + cos(angle) * offset_dist;
        let bz = wz + sin(angle) * offset_dist;
        let by = get_height_world(bx, bz);

        // Height scale variety within tuft — very short on sand, full in grass zone
        let size_bits = f32((blade_hash >> 16u) & 0xFFu) / 255.0;
        let base_scale = 0.5 + size_bits * 1.0; // 0.5 - 1.5
        let scale = base_scale * (0.15 + 0.85 * zone_density); // 15% height on sand, full in grass

        // Per-blade color variation (+/-6%)
        let color_var = f32((blade_hash >> 4u) & 0xFFu) / 255.0;
        let v = color_var * 0.12 - 0.06;
        let r = max(terrain_col.r + v, 0.05);
        let g = max(terrain_col.g + v + patch_green, 0.10);
        let b = max(terrain_col.b + v, 0.03);

        // Random rotation
        let rot_bits = f32((blade_hash >> 24u) & 0xFFu) / 255.0;
        let rotation = rot_bits * TAU;

        // Atomic append to instance buffer
        let idx = atomicAdd(&draw_args[1], 1u);
        if (idx >= MAX_INSTANCES) {
            // Buffer full — undo and bail
            atomicSub(&draw_args[1], 1u);
            return;
        }

        instances[idx] = GrassInstance(
            vec4(bx, by, bz, scale),
            vec4(r, g, b, rotation),
        );
    }
}
