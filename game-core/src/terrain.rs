use crate::{HEIGHTMAP_RES, WORLD_SIZE};

/// Sample height from a heightmap at world coordinates (x, z).
/// Heightmap is a flat array of f32 values, HEIGHTMAP_RES x HEIGHTMAP_RES.
pub fn sample_height(heightmap: &[f32], x: f32, z: f32) -> f32 {
    let u = x / WORLD_SIZE;
    let v = z / WORLD_SIZE;

    if u < 0.0 || u >= 1.0 || v < 0.0 || v >= 1.0 {
        return 0.0;
    }

    let fx = u * (HEIGHTMAP_RES - 1) as f32;
    let fz = v * (HEIGHTMAP_RES - 1) as f32;

    let ix = fx as usize;
    let iz = fz as usize;

    let ix1 = (ix + 1).min(HEIGHTMAP_RES as usize - 1);
    let iz1 = (iz + 1).min(HEIGHTMAP_RES as usize - 1);

    let frac_x = fx - ix as f32;
    let frac_z = fz - iz as f32;

    let res = HEIGHTMAP_RES as usize;
    let h00 = heightmap[iz * res + ix];
    let h10 = heightmap[iz * res + ix1];
    let h01 = heightmap[iz1 * res + ix];
    let h11 = heightmap[iz1 * res + ix1];

    // Bilinear interpolation
    let h0 = h00 + (h10 - h00) * frac_x;
    let h1 = h01 + (h11 - h01) * frac_x;
    h0 + (h1 - h0) * frac_z
}

/// Check if a position is within world bounds.
pub fn in_bounds(x: f32, z: f32) -> bool {
    x >= 0.0 && x < WORLD_SIZE && z >= 0.0 && z < WORLD_SIZE
}

/// Tree-placement height range (must match trees_compute.wgsl).
const TREE_HEIGHT_MAX: f32 = 17.0;

/// Find a clear spawn position (no trees) near world center.
/// Searches outward for a position where terrain height > TREE_HEIGHT_MAX.
pub fn find_clear_spawn(heightmap: &[f32]) -> (f32, f32) {
    let cx = WORLD_SIZE / 2.0;
    let cz = WORLD_SIZE / 2.0;
    let step = 2.0;

    // Search in expanding square rings around center
    for ring in 0..40 {
        let radius = ring as f32 * step;
        let samples = ((ring * 4).max(1)) as i32;
        for i in 0..samples {
            let angle = i as f32 / samples as f32 * std::f32::consts::TAU;
            let x = cx + radius * angle.cos();
            let z = cz + radius * angle.sin();
            if !in_bounds(x, z) {
                continue;
            }
            if sample_height(heightmap, x, z) > TREE_HEIGHT_MAX {
                return (x, z);
            }
        }
    }
    (cx, cz) // fallback: center
}

/// Generate a simple procedural heightmap using layered sine waves.
/// Returns a Vec<f32> of HEIGHTMAP_RES * HEIGHTMAP_RES values.
pub fn generate_heightmap() -> Vec<f32> {
    let res = HEIGHTMAP_RES as usize;
    let mut data = vec![0.0f32; res * res];
    for iz in 0..res {
        for ix in 0..res {
            let u = ix as f32 / res as f32;
            let v = iz as f32 / res as f32;
            let h = 8.0 * (u * 3.0 * std::f32::consts::PI).sin()
                * (v * 2.0 * std::f32::consts::PI).sin()
                + 4.0 * (u * 7.0 + 0.3).sin() * (v * 5.0 + 1.2).sin()
                + 2.0 * (u * 13.0 + 2.1).sin() * (v * 11.0 + 0.7).sin()
                + 15.0;
            data[iz * res + ix] = h;
        }
    }
    data
}
