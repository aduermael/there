// Shared heightmap access utilities for compute shaders.
// Requires: uniforms.wgsl (Uniforms struct with hm_res, world_size),
//           heightmap texture bound at @group(1) @binding(0).

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
