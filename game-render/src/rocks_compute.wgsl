// GPU-driven rock instance generation.
// Dispatched per-frame to populate a storage buffer with visible rock instances.
// Camera-centered grid with 8-texel cell step: matches CPU scatter density.
// Uniforms from uniforms.wgsl, noise/hash from noise.wgsl.

struct RockInstance {
    pos_scale: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var heightmap: texture_2d<f32>;
@group(2) @binding(0) var<storage, read_write> instances: array<RockInstance>;
@group(2) @binding(1) var<storage, read_write> draw_args: array<atomic<u32>, 5>;

const MAX_INSTANCES: u32 = 4096u;
const GRID_EXTENT: u32 = 96u;
const CELL_STEP: u32 = 8u;

// --- Heightmap access ---

fn get_height(tc: vec2<i32>) -> f32 {
    let res = i32(u.hm_res);
    return textureLoad(heightmap, clamp(tc, vec2(0), vec2(res - 1)), 0).r;
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

// --- Main compute kernel ---

@compute @workgroup_size(16, 16, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= GRID_EXTENT || gid.y >= GRID_EXTENT) { return; }

    let texel_size = u.world_size / u.hm_res;
    let cell_size = texel_size * f32(CELL_STEP);

    // Camera-centered grid origin (in cell coordinates)
    let half_extent = f32(GRID_EXTENT) * cell_size * 0.5;
    let max_cells = u32(u.hm_res) / CELL_STEP;
    let origin_x = u32(clamp((u.camera_pos.x - half_extent) / cell_size, 0.0, f32(max_cells) - 1.0));
    let origin_z = u32(clamp((u.camera_pos.z - half_extent) / cell_size, 0.0, f32(max_cells) - 1.0));

    let cell_x = origin_x + gid.x;
    let cell_z = origin_z + gid.y;

    // Map to heightmap texel coordinates
    let hm_x = cell_x * CELL_STEP;
    let hm_z = cell_z * CELL_STEP;

    // Bounds check
    if (hm_x >= u32(u.hm_res) || hm_z >= u32(u.hm_res)) { return; }

    // Height filter: rocks in mountain zones (h > 18)
    let tc = vec2<i32>(i32(hm_x), i32(hm_z));
    let h = get_height(tc);
    if (h <= 18.0) { return; }

    // Slope filter
    let slope = compute_slope(tc);
    if (slope > 0.7) { return; }

    // Hash-based acceptance (~40%)
    let hash = cell_hash(hm_x, hm_z, 0xDEADu);
    if ((hash & 0xFFu) > 100u) { return; }

    // Jitter position within cell
    let jx = f32((hash >> 8u) & 0xFFu) / 255.0;
    let jz = f32((hash >> 16u) & 0xFFu) / 255.0;
    let wx = (f32(hm_x) + jx * f32(CELL_STEP)) * texel_size;
    let wz = (f32(hm_z) + jz * f32(CELL_STEP)) * texel_size;

    // Height at jittered position
    let uv = vec2(wx, wz) / u.world_size;
    let jtc = vec2<i32>(vec2<f32>(uv * u.hm_res));
    let wy = get_height(jtc);

    // Distance cull
    let cam_dist = length(vec2(wx, wz) - u.camera_pos.xz);
    if (cam_dist > 200.0) { return; }

    // Size variant from hash
    let size_hash = f32((hash >> 24u) & 0xFFu) / 255.0;
    let scale = 0.5 + size_hash * 1.5; // 0.5-2.0

    // Distance LOD thinning: skip small rocks far away
    if (cam_dist > 150.0 && scale < 1.0) { return; }

    // Frustum cull (test at rock center, generous margins)
    let clip = u.view_proj * vec4(wx, wy + scale * 0.5, wz, 1.0);
    if (clip.w < 0.1) { return; }
    let ndc = clip.xy / clip.w;
    if (abs(ndc.x) > 1.8 || ndc.y < -2.5 || ndc.y > 1.8) { return; }

    // Grey-brown color with slight variation
    let color_var = f32((hash >> 4u) & 0xFFu) / 255.0 * 0.1;
    let r = 0.50 + color_var;
    let g = 0.45 + color_var * 0.8;
    let b = 0.40 + color_var * 0.5;

    // Atomic append
    let idx = atomicAdd(&draw_args[1], 1u);
    if (idx >= MAX_INSTANCES) {
        atomicSub(&draw_args[1], 1u);
        return;
    }

    instances[idx] = RockInstance(vec4(wx, wy, wz, scale), vec4(r, g, b, 0.0));
}
