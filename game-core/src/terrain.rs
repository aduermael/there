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
