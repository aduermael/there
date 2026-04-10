use crate::grass::GrassInstance;
use crate::rocks::RockInstance;
use crate::trees::TreeInstance;

/// Deterministic scatter placement for rocks, trees, and grass based on heightmap.
pub fn scatter_objects(
    heightmap: &[f32],
) -> (Vec<RockInstance>, Vec<TreeInstance>, Vec<GrassInstance>) {
    let hm_res = game_core::HEIGHTMAP_RES as usize;
    let world_size = game_core::WORLD_SIZE;
    let texel_size = world_size / hm_res as f32;

    let mut rocks = Vec::new();
    let mut trees = Vec::new();
    let mut grass = Vec::new();

    // Grid-based placement: check every Nth cell
    let rock_step = 8; // check every 8 texels for rock candidates
    let tree_step = 6; // check every 6 texels for tree candidates

    // Rocks: height > 18 (rocky/mountain zones)
    for gz in (0..hm_res).step_by(rock_step) {
        for gx in (0..hm_res).step_by(rock_step) {
            let h = heightmap[gz * hm_res + gx];
            if h <= 18.0 {
                continue;
            }

            let hash = cell_hash(gx as u32, gz as u32, 0xDEAD);
            // ~40% acceptance rate
            if (hash & 0xFF) > 100 {
                continue;
            }

            let slope = sample_slope(heightmap, hm_res, gx, gz, texel_size);
            if slope > 0.7 {
                continue; // too steep
            }

            // Jitter position within the cell
            let jx = ((hash >> 8) & 0xFF) as f32 / 255.0;
            let jz = ((hash >> 16) & 0xFF) as f32 / 255.0;
            let wx = (gx as f32 + jx * rock_step as f32) * texel_size;
            let wz = (gz as f32 + jz * rock_step as f32) * texel_size;
            let wy = game_core::terrain::sample_height(heightmap, wx, wz);

            // Size variant from hash
            let size_hash = ((hash >> 24) & 0xFF) as f32 / 255.0;
            let scale = 0.5 + size_hash * 1.5; // 0.5 to 2.0

            // Grey-brown color with slight variation
            let color_var = ((hash >> 4) & 0xFF) as f32 / 255.0 * 0.1;
            let r = 0.50 + color_var;
            let g = 0.45 + color_var * 0.8;
            let b = 0.40 + color_var * 0.5;

            rocks.push(RockInstance {
                pos_scale: [wx, wy, wz, scale],
                color: [r, g, b, 0.0],
            });
        }
    }

    // Trees: height 10–17 (grass zones), avoid steep slopes
    for gz in (0..hm_res).step_by(tree_step) {
        for gx in (0..hm_res).step_by(tree_step) {
            let h = heightmap[gz * hm_res + gx];
            if h < 10.0 || h > 17.0 {
                continue;
            }

            let hash = cell_hash(gx as u32, gz as u32, 0xBEEF);
            // ~35% acceptance rate
            if (hash & 0xFF) > 90 {
                continue;
            }

            let slope = sample_slope(heightmap, hm_res, gx, gz, texel_size);
            if slope > 0.4 {
                continue; // trees don't grow on steep slopes
            }

            let jx = ((hash >> 8) & 0xFF) as f32 / 255.0;
            let jz = ((hash >> 16) & 0xFF) as f32 / 255.0;
            let wx = (gx as f32 + jx * tree_step as f32) * texel_size;
            let wz = (gz as f32 + jz * tree_step as f32) * texel_size;
            let wy = game_core::terrain::sample_height(heightmap, wx, wz);

            let size_hash = ((hash >> 24) & 0xFF) as f32 / 255.0;
            let scale = 1.0 + size_hash * 1.0; // 1.0 to 2.0

            // Green foliage with slight variation
            let green_var = ((hash >> 4) & 0xFF) as f32 / 255.0;
            let r = 0.25 + green_var * 0.1;
            let g = 0.50 + green_var * 0.25;
            let b = 0.15 + green_var * 0.1;

            trees.push(TreeInstance {
                pos_scale: [wx, wy, wz, scale],
                foliage_color: [r, g, b, 0.0],
            });
        }
    }

    // --- Grass: patch-based distribution with rock-aware placement ---

    // Pass 1: Identify patch centers — fewer but larger for clearer meadow clusters
    let patch_step = 12;
    let mut patches: Vec<(f32, f32, f32)> = Vec::new(); // (wx, wz, radius)
    for gz in (0..hm_res).step_by(patch_step) {
        for gx in (0..hm_res).step_by(patch_step) {
            let h = heightmap[gz * hm_res + gx];
            if h < 8.0 || h > 17.0 {
                continue;
            }
            let hash = cell_hash(gx as u32, gz as u32, 0xFACE);
            // ~35% acceptance — fewer patches with more bare ground between
            if (hash & 0xFF) > 90 {
                continue;
            }
            let wx = gx as f32 * texel_size;
            let wz = gz as f32 * texel_size;
            let radius = 4.0 + ((hash >> 8) & 0xFF) as f32 / 255.0 * 4.0; // 4-8 texels
            patches.push((wx, wz, radius * texel_size));
        }
    }

    // Pass 2: Place grass on fine grid, density depends on proximity to patch centers
    let grass_step = 2;
    for gz in (0..hm_res).step_by(grass_step) {
        for gx in (0..hm_res).step_by(grass_step) {
            if grass.len() >= crate::grass::MAX_GRASS {
                break;
            }
            let h = heightmap[gz * hm_res + gx];
            if h < 8.0 || h > 17.0 {
                continue;
            }

            let slope = sample_slope(heightmap, hm_res, gx, gz, texel_size);
            if slope > 0.3 {
                continue;
            }

            let hash = cell_hash(gx as u32, gz as u32, 0xCAFE);
            let wx_base = gx as f32 * texel_size;
            let wz_base = gz as f32 * texel_size;

            // Check if inside any patch
            let in_patch = patches.iter().any(|&(px, pz, pr)| {
                let dx = wx_base - px;
                let dz = wz_base - pz;
                dx * dx + dz * dz < pr * pr
            });

            // Inside patch: ~90% dense fill, outside: ~8% sparse strays
            let threshold = if in_patch { 230 } else { 20 };
            if (hash & 0xFF) > threshold {
                continue;
            }

            let jx = ((hash >> 8) & 0xFF) as f32 / 255.0;
            let jz = ((hash >> 16) & 0xFF) as f32 / 255.0;
            let wx = (gx as f32 + jx * grass_step as f32) * texel_size;
            let wz = (gz as f32 + jz * grass_step as f32) * texel_size;
            let wy = game_core::terrain::sample_height(heightmap, wx, wz);

            let size_hash = ((hash >> 24) & 0xFF) as f32 / 255.0;
            let scale = 0.7 + size_hash * 0.6; // 0.7-1.3 — all blades visible

            // Match blade color to terrain — same noise-based color as terrain.wgsl
            let terrain_col = terrain_color_at(wy, wx, wz);
            let color_hash = ((hash >> 4) & 0xFF) as f32 / 255.0;
            let var = color_hash * 0.08 - 0.04; // ±4% random per-blade variation
            let r = (terrain_col[0] + var).max(0.05);
            let g = (terrain_col[1] + var + 0.03).max(0.10); // slight green push
            let b = (terrain_col[2] + var - 0.01).max(0.03);

            let rot_hash = ((hash >> 12) & 0xFF) as f32 / 255.0;
            let rotation = rot_hash * std::f32::consts::TAU;

            grass.push(GrassInstance {
                pos_scale: [wx, wy, wz, scale],
                color_rotation: [r, g, b, rotation],
            });
        }
    }

    // Pass 3: Scatter grass rings around rock bases
    for rock in &rocks {
        let rx = rock.pos_scale[0];
        let rz = rock.pos_scale[2];
        let rock_scale = rock.pos_scale[3];
        let ring_radius = 2.0 + rock_scale * 1.0;
        let ring_samples = 12;

        for i in 0..ring_samples {
            if grass.len() >= crate::grass::MAX_GRASS {
                break;
            }
            let hash = cell_hash(i as u32, (rx * 100.0) as u32, 0xF00D);
            let angle = (i as f32 / ring_samples as f32) * std::f32::consts::TAU
                + ((hash & 0xFF) as f32 / 255.0) * 0.5;
            let dist = ring_radius * 0.5
                + ((hash >> 8) & 0xFF) as f32 / 255.0 * ring_radius * 0.5;
            let gx = rx + angle.cos() * dist;
            let gz = rz + angle.sin() * dist;
            let gy = game_core::terrain::sample_height(heightmap, gx, gz);

            if gy < 8.0 || gy > 20.0 {
                continue;
            }

            let size_hash = ((hash >> 16) & 0xFF) as f32 / 255.0;
            let scale = 0.4 + size_hash * 0.6;

            // Match blade color to terrain at rock base — same noise matching
            let terrain_col = terrain_color_at(gy, gx, gz);
            let color_hash = ((hash >> 4) & 0xFF) as f32 / 255.0;
            let var = color_hash * 0.08 - 0.04;
            let r = (terrain_col[0] + var).max(0.05);
            let g = (terrain_col[1] + var + 0.03).max(0.10);
            let b = (terrain_col[2] + var - 0.01).max(0.03);

            let rot_hash = ((hash >> 24) & 0xFF) as f32 / 255.0;
            let rotation = rot_hash * std::f32::consts::TAU;

            grass.push(GrassInstance {
                pos_scale: [gx, gy, gz, scale],
                color_rotation: [r, g, b, rotation],
            });
        }
    }

    log::info!(
        "Scatter: {} rocks, {} trees, {} grass placed",
        rocks.len(),
        trees.len(),
        grass.len(),
    );

    (rocks, trees, grass)
}

/// Terrain color at a given position — matches terrain.wgsl noise-based coloring.
/// Evaluates the same hash2/value_noise and per-biome hue/brightness shifts.
fn terrain_color_at(h: f32, wx: f32, wz: f32) -> [f32; 3] {
    let sand = [0.76_f32, 0.70, 0.50];
    let grass = [0.32_f32, 0.54, 0.22];
    let rock = [0.50_f32, 0.45, 0.40];
    let sg = smoothstep_f32(8.0, 14.0, h);
    let gr = smoothstep_f32(18.0, 24.0, h);

    let mut base = [
        sand[0] + (grass[0] - sand[0]) * sg + (rock[0] - (sand[0] + (grass[0] - sand[0]) * sg)) * gr,
        sand[1] + (grass[1] - sand[1]) * sg + (rock[1] - (sand[1] + (grass[1] - sand[1]) * sg)) * gr,
        sand[2] + (grass[2] - sand[2]) * sg + (rock[2] - (sand[2] + (grass[2] - sand[2]) * sg)) * gr,
    ];

    // Same 3-scale noise as terrain.wgsl
    let n_large = value_noise(wx * 0.12, wz * 0.12) * 2.0 - 1.0;
    let n_med = value_noise(wx * 0.25 + 37.0, wz * 0.25 + 91.0) * 2.0 - 1.0;
    let n_fine = value_noise(wx * 0.45, wz * 0.45) * 2.0 - 1.0;
    let noise = n_large * 0.6 + n_med * 0.25 + n_fine * 0.15;

    // Per-biome hue shifts (grass zone dominates for h 8-17)
    let grass_hue = [
        0.04 * n_large + (-0.02) * n_med,
        -0.02 * n_large + 0.03 * n_med,
        -0.03 * n_large + (-0.01) * n_med,
    ];
    let sand_hue = [
        0.03 * n_large + (-0.02) * n_med,
        0.01 * n_large + (-0.01) * n_med,
        -0.04 * n_large + 0.03 * n_med,
    ];
    let grass_bright = noise * 0.12;
    let sand_bright = noise * 0.10;

    // Blend hue/brightness by biome
    let hue = [
        sand_hue[0] + (grass_hue[0] - sand_hue[0]) * sg,
        sand_hue[1] + (grass_hue[1] - sand_hue[1]) * sg,
        sand_hue[2] + (grass_hue[2] - sand_hue[2]) * sg,
    ];
    let brightness = sand_bright + (grass_bright - sand_bright) * sg;

    base[0] = (base[0] + hue[0] + base[0] * brightness).max(0.02);
    base[1] = (base[1] + hue[1] + base[1] * brightness).max(0.02);
    base[2] = (base[2] + hue[2] + base[2] * brightness).max(0.02);

    // Flat-terrain boost (grass is placed on flat terrain, so apply full boost)
    let flat_boost = 0.08 * sg * (1.0 - gr);
    base[0] = base[0] * (1.0 + flat_boost) + (-0.01) * flat_boost;
    base[1] = base[1] * (1.0 + flat_boost) + 0.02 * flat_boost;
    base[2] = base[2] * (1.0 + flat_boost) + (-0.01) * flat_boost;

    base
}

/// WGSL-matching hash2 for noise
fn hash2_f32(px: f32, py: f32) -> f32 {
    let mut p3x = fract_f32(px * 0.1031);
    let mut p3y = fract_f32(py * 0.1031);
    let mut p3z = fract_f32(px * 0.1031);
    let d = p3x * (p3y + 33.33) + p3y * (p3z + 33.33) + p3z * (p3x + 33.33);
    p3x += d;
    p3y += d;
    p3z += d;
    fract_f32((p3x + p3y) * p3z)
}

/// WGSL-matching value noise
fn value_noise(px: f32, py: f32) -> f32 {
    let ix = px.floor();
    let iy = py.floor();
    let fx = px - ix;
    let fy = py - iy;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);

    let a = hash2_f32(ix, iy);
    let b = hash2_f32(ix + 1.0, iy);
    let c = hash2_f32(ix, iy + 1.0);
    let d = hash2_f32(ix + 1.0, iy + 1.0);

    let ab = a + (b - a) * sx;
    let cd = c + (d - c) * sx;
    ab + (cd - ab) * sy
}

fn fract_f32(x: f32) -> f32 {
    x - x.floor()
}

fn smoothstep_f32(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Compute terrain slope magnitude at a heightmap texel.
fn sample_slope(heightmap: &[f32], hm_res: usize, x: usize, z: usize, texel_size: f32) -> f32 {
    let get = |ix: usize, iz: usize| -> f32 {
        let ix = ix.min(hm_res - 1);
        let iz = iz.min(hm_res - 1);
        heightmap[iz * hm_res + ix]
    };

    let hl = if x > 0 { get(x - 1, z) } else { get(x, z) };
    let hr = get(x + 1, z);
    let hd = if z > 0 { get(x, z - 1) } else { get(x, z) };
    let hu = get(x, z + 1);

    let dx = (hr - hl) / (2.0 * texel_size);
    let dz = (hu - hd) / (2.0 * texel_size);
    (dx * dx + dz * dz).sqrt()
}

/// Deterministic hash for grid cell placement decisions.
fn cell_hash(x: u32, z: u32, seed: u32) -> u32 {
    let mut h = seed;
    h = h.wrapping_add(x.wrapping_mul(0x9e3779b9));
    h ^= h >> 16;
    h = h.wrapping_add(z.wrapping_mul(0x85ebca6b));
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2ae35);
    h ^= h >> 16;
    h
}
