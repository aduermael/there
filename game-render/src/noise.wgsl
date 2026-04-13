// Shared noise and hash functions.
// Prepended to shaders that need procedural generation or dithering.

const TAU: f32 = 6.2831853;

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

fn fbm2(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    // 2 octaves
    val += amp * value_noise(pos); pos *= 2.03; amp *= 0.5;
    val += amp * value_noise(pos);
    return val;
}

fn fbm3(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    // 3 octaves
    val += amp * value_noise(pos); pos *= 2.03; amp *= 0.5;
    val += amp * value_noise(pos); pos *= 2.03; amp *= 0.5;
    val += amp * value_noise(pos);
    return val;
}

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

// Interleaved Gradient Noise (Jimenez 2014) — spatially stable, blue-noise-like
fn ign(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(0.06711056 * pixel.x + 0.00583715 * pixel.y));
}
