// Screen-Space Ambient Occlusion — reads scene depth, outputs AO to R8Unorm.
// v2: Fixed half-res coordinate mapping, IGN noise, TBN hemisphere sampling.
// 1.0 = unoccluded, 0.0 = fully occluded.

struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_dir: vec3<f32>,
    fog_color: vec3<f32>,
    fog_density: f32,
    world_size: f32,
    hm_res: f32,
    fog_height_falloff: f32,
    time: f32,
    sun_color: vec3<f32>,
    sky_zenith: vec3<f32>,
    sky_horizon: vec3<f32>,
    inv_view_proj: mat4x4<f32>,
    sky_ambient: vec3<f32>,
    ground_ambient: vec3<f32>,
    sun_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var depth_tex: texture_depth_2d;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let uv_x = f32((vi << 1u) & 2u);
    let uv_y = f32(vi & 2u);
    var out: VertexOutput;
    out.position = vec4(uv_x * 2.0 - 1.0, uv_y * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2(uv_x, 1.0 - uv_y);
    return out;
}

const NEAR: f32 = 0.1;
const FAR: f32 = 500.0;
const SAMPLES: u32 = 12;
const RADIUS: f32 = 3.0;
const STRENGTH: f32 = 5.5;

fn linearize(d: f32) -> f32 {
    return NEAR * FAR / (FAR - d * (FAR - NEAR));
}

// Interleaved Gradient Noise (Jimenez 2014) — spatially stable, blue-noise-like
fn ign(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(0.06711056 * pixel.x + 0.00583715 * pixel.y));
}

fn reconstruct_pos(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0);
    let clip = vec4(ndc, depth, 1.0);
    let wh = u.inv_view_proj * clip;
    return wh.xyz / wh.w;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let depth_dims = vec2<f32>(textureDimensions(depth_tex));

    // FIX: Map screen UV to full-res depth pixel (AO target is half-res, depth is full-res)
    let depth_pixel = min(vec2<i32>(in.uv * depth_dims), vec2<i32>(depth_dims) - 1);
    let depth = textureLoad(depth_tex, depth_pixel, 0);

    // Reconstruct world position and flat normal
    let pos = reconstruct_pos(in.uv, depth);
    var n = normalize(cross(dpdx(pos), dpdy(pos)));
    n *= select(1.0, -1.0, dot(n, pos - u.camera_pos) > 0.0);

    // Sky: no occlusion
    if depth >= 0.999 {
        return vec4(1.0);
    }

    // Per-pixel rotation from IGN — removes banding without visible noise
    let rotation = ign(in.position.xy) * 6.28318;

    // Build orthonormal tangent frame around surface normal
    let ref_up = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(n.y) > 0.99);
    let tangent = normalize(cross(ref_up, n));
    let bitangent = cross(n, tangent);

    var occ = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        let fi = f32(i);

        // Stratified layer + golden angle spiral with per-pixel rotation
        let layer = (fi + 0.5) / f32(SAMPLES);
        let angle = fi * 2.399963 + rotation;

        // Cosine-weighted hemisphere point
        let cos_theta = sqrt(layer);
        let sin_theta = sqrt(1.0 - layer);
        let local = vec3(sin_theta * cos(angle), sin_theta * sin(angle), cos_theta);

        // Transform to world-space hemisphere aligned with surface
        let offset = tangent * local.x + bitangent * local.y + n * local.z;

        // Progressive radius: near samples for contact, far for broad AO
        let scale = max(layer, 0.1);
        let sample_pos = pos + offset * RADIUS * scale;

        // Project sample point to screen space
        let proj = u.view_proj * vec4(sample_pos, 1.0);
        let proj_w = max(proj.w, 0.001);
        let proj_ndc = proj.xyz / proj_w;
        let proj_uv = vec2(proj_ndc.x * 0.5 + 0.5, 1.0 - (proj_ndc.y * 0.5 + 0.5));
        let proj_pixel = vec2<i32>(proj_uv * depth_dims);

        let in_bounds = proj.w > 0.0
            && proj_pixel.x >= 0 && proj_pixel.x < i32(depth_dims.x)
            && proj_pixel.y >= 0 && proj_pixel.y < i32(depth_dims.y);

        if in_bounds {
            let actual_depth = textureLoad(depth_tex, proj_pixel, 0);
            let actual_linear = linearize(actual_depth);
            let expected_linear = linearize(clamp(proj_ndc.z, 0.0, 1.0));

            // Occluded when scene surface is closer than sample, with range falloff
            let diff = expected_linear - actual_linear;
            let range_ok = smoothstep(RADIUS, 0.0, diff);
            occ += step(0.04, diff) * range_ok;
        }
    }

    let ao = clamp(1.0 - (occ / f32(SAMPLES)) * STRENGTH, 0.0, 1.0);
    return vec4(vec3(ao), 1.0);
}
