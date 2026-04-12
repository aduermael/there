// Shared triplanar texture sampling for atlas-based materials.
// Requires atlas (texture_2d_array) and atlas_sampler to be declared in the including shader.

fn triplanar_sample(world_pos: vec3<f32>, n: vec3<f32>, layer: i32, scale: f32) -> vec3<f32> {
    let blend = abs(n);
    let w = blend / (blend.x + blend.y + blend.z + 0.001);

    let tx = textureSample(atlas, atlas_sampler, fract(world_pos.yz * scale), layer).rgb;
    let ty = textureSample(atlas, atlas_sampler, fract(world_pos.xz * scale), layer).rgb;
    let tz = textureSample(atlas, atlas_sampler, fract(world_pos.xy * scale), layer).rgb;

    return tx * w.x + ty * w.y + tz * w.z;
}
