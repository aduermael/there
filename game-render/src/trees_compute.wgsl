// GPU-driven tree instance generation.
// Dispatched per-frame to populate a storage buffer with visible tree instances.
// Camera-centered grid with 6-texel cell step: matches CPU scatter density.
// Includes tree clustering (companion trees near anchor trees).
// Uniforms from uniforms.wgsl, noise/hash from noise.wgsl.

struct TreeInstance {
    pos_scale: vec4<f32>,
    foliage_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var heightmap: texture_2d<f32>;
@group(2) @binding(0) var<storage, read_write> instances: array<TreeInstance>;
@group(2) @binding(1) var<storage, read_write> draw_args: array<atomic<u32>, 5>;

const MAX_INSTANCES: u32 = 8192u;
const GRID_EXTENT: u32 = 128u;
const CELL_STEP: u32 = 6u;
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

// --- Append helper ---

fn append_tree(inst: TreeInstance) -> bool {
    let idx = atomicAdd(&draw_args[1], 1u);
    if (idx >= MAX_INSTANCES) {
        atomicSub(&draw_args[1], 1u);
        return false;
    }
    instances[idx] = inst;
    return true;
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

    // Height filter: trees grow in grass zones (h 10-17)
    let tc = vec2<i32>(i32(hm_x), i32(hm_z));
    let h = get_height(tc);
    if (h < 10.0 || h > 17.0) { return; }

    // Slope filter: trees don't grow on steep slopes
    let slope = compute_slope(tc);
    if (slope > 0.4) { return; }

    // Hash-based acceptance (~35%)
    let hash = cell_hash(hm_x, hm_z, 0xBEEFu);
    if ((hash & 0xFFu) > 90u) { return; }

    // Extended jitter: push beyond cell boundary to break grid pattern
    let jx = f32((hash >> 8u) & 0xFFu) / 255.0 * 2.0 - 0.5;
    let jz = f32((hash >> 16u) & 0xFFu) / 255.0 * 2.0 - 0.5;
    let wx = (f32(hm_x) + jx * f32(CELL_STEP)) * texel_size;
    let wz = (f32(hm_z) + jz * f32(CELL_STEP)) * texel_size;

    // Height at jittered position
    let wy = get_height_world(wx, wz);

    // Distance cull
    let cam_dist = length(vec2(wx, wz) - u.camera_pos.xz);
    if (cam_dist > 200.0) { return; }

    // Distance-based LOD thinning: skip small trees far away
    let size_hash = f32((hash >> 24u) & 0xFFu) / 255.0;
    let scale = 0.5 + size_hash * size_hash * 3.5; // 0.5-4.0, biased small
    if (cam_dist > 150.0 && scale < 1.5) { return; }

    // Frustum cull (test at tree mid-height, generous margins for crown)
    let test_y = wy + scale * 1.5;
    let clip = u.view_proj * vec4(wx, test_y, wz, 1.0);
    if (clip.w < 0.1) { return; }
    let ndc = clip.xy / clip.w;
    if (abs(ndc.x) > 2.0 || ndc.y < -3.0 || ndc.y > 2.0) { return; }

    // Color variation: blue-green to yellow-green
    let green_var = f32((hash >> 4u) & 0xFFu) / 255.0;
    let blue_var = f32((hash >> 12u) & 0xFFu) / 255.0;
    let r = 0.20 + green_var * 0.15;
    let g = 0.45 + green_var * 0.30;
    let b = 0.10 + blue_var * 0.18;

    // Crown shape factor: 0=narrow, 1=wide/bushy
    let shape = f32((hash >> 20u) & 0xFFu) / 255.0;

    // Append primary tree
    if (!append_tree(TreeInstance(vec4(wx, wy, wz, scale), vec4(r, g, b, shape)))) { return; }

    // --- Clustering: ~25% become cluster seeds ---
    let cluster_hash = cell_hash(u32(wx * 100.0), u32(wz * 100.0), 0xCEEDu);
    if ((cluster_hash & 0xFFu) > 64u) { return; }

    let companions = 2u + ((cluster_hash >> 8u) % 3u); // 2-4 companions
    for (var j = 0u; j < companions; j++) {
        let ch = cell_hash(j, cluster_hash, 0xACE0u);
        let angle = f32(ch & 0xFFu) / 255.0 * TAU;
        let dist = 3.0 + f32((ch >> 8u) & 0xFFu) / 255.0 * 4.0; // 3-7 units away
        let cx = wx + cos(angle) * dist;
        let cz = wz + sin(angle) * dist;
        let cy = get_height_world(cx, cz);

        // Companions must stay in valid height range and near parent
        if (cy < 10.0 || cy > 17.0 || abs(cy - wy) > 3.0) { continue; }

        // Companion size: smaller than primary, biased small
        let sh = f32((ch >> 16u) & 0xFFu) / 255.0;
        let cscale = 0.4 + sh * sh * 1.2; // 0.4-1.6

        // Skip small companions at distance
        if (cam_dist > 150.0 && cscale < 1.0) { continue; }

        // Companion color
        let gv = f32((ch >> 4u) & 0xFFu) / 255.0;
        let bv = f32((ch >> 12u) & 0xFFu) / 255.0;
        let cr = 0.20 + gv * 0.15;
        let cg = 0.45 + gv * 0.30;
        let cb = 0.10 + bv * 0.18;
        let cshape = f32((ch >> 24u) & 0xFFu) / 255.0;

        if (!append_tree(TreeInstance(vec4(cx, cy, cz, cscale), vec4(cr, cg, cb, cshape)))) { return; }
    }
}
