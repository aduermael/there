// Shared depth linearization, position reconstruction, and NDC-to-UV utilities.
// Prepended to shaders that need depth buffer operations.

const NEAR: f32 = 0.1;
const FAR: f32 = 500.0;

fn linearize_depth(d: f32) -> f32 {
    return NEAR * FAR / (FAR - d * (FAR - NEAR));
}

fn reconstruct_pos(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0);
    let clip = vec4(ndc, depth, 1.0);
    let wh = u.inv_view_proj * clip;
    return wh.xyz / wh.w;
}

fn ndc_to_uv(ndc_xy: vec2<f32>) -> vec2<f32> {
    return vec2(ndc_xy.x * 0.5 + 0.5, 1.0 - (ndc_xy.y * 0.5 + 0.5));
}
