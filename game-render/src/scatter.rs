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

    // Pass 1: Identify patch centers on a coarse grid
    let patch_step = 10;
    let mut patches: Vec<(f32, f32, f32)> = Vec::new(); // (wx, wz, radius)
    for gz in (0..hm_res).step_by(patch_step) {
        for gx in (0..hm_res).step_by(patch_step) {
            let h = heightmap[gz * hm_res + gx];
            if h < 8.0 || h > 17.0 {
                continue;
            }
            let hash = cell_hash(gx as u32, gz as u32, 0xFACE);
            // ~45% of coarse cells become patch centers
            if (hash & 0xFF) > 115 {
                continue;
            }
            let wx = gx as f32 * texel_size;
            let wz = gz as f32 * texel_size;
            let radius = 3.0 + ((hash >> 8) & 0xFF) as f32 / 255.0 * 3.0; // 3-6 texels
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

            // Inside patch: ~85% acceptance, outside: ~12%
            let threshold = if in_patch { 217 } else { 30 };
            if (hash & 0xFF) > threshold {
                continue;
            }

            let jx = ((hash >> 8) & 0xFF) as f32 / 255.0;
            let jz = ((hash >> 16) & 0xFF) as f32 / 255.0;
            let wx = (gx as f32 + jx * grass_step as f32) * texel_size;
            let wz = (gz as f32 + jz * grass_step as f32) * texel_size;
            let wy = game_core::terrain::sample_height(heightmap, wx, wz);

            let size_hash = ((hash >> 24) & 0xFF) as f32 / 255.0;
            let scale = 0.5 + size_hash * 0.8;

            let color_hash = ((hash >> 4) & 0xFF) as f32 / 255.0;
            let r = 0.25 + color_hash * 0.15;
            let g = 0.45 + color_hash * 0.3;
            let b = 0.12 + color_hash * 0.08;

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
        let ring_radius = 2.0 + rock_scale * 1.0; // 2-4 units around rock
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

            // Relaxed height filter (rocks sit on higher terrain)
            if gy < 8.0 || gy > 20.0 {
                continue;
            }

            let size_hash = ((hash >> 16) & 0xFF) as f32 / 255.0;
            let scale = 0.4 + size_hash * 0.6;

            let color_hash = ((hash >> 4) & 0xFF) as f32 / 255.0;
            let r = 0.22 + color_hash * 0.12;
            let g = 0.40 + color_hash * 0.25;
            let b = 0.10 + color_hash * 0.08;

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
