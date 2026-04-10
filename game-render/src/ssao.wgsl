// Screen-Space Ambient Occlusion — reads scene depth, outputs AO to R8Unorm.
// Reconstructs world position from depth via inv_view_proj, computes normal from
// screen-space derivatives, then samples hemisphere to detect nearby occluders.

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
const SAMPLES: u32 = 8;
const RADIUS: f32 = 3.0;
const STRENGTH: f32 = 1.5;

fn linearize(d: f32) -> f32 {
    return NEAR * FAR / (FAR - d * (FAR - NEAR));
}

fn hash_f(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, vec3(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33));
    return fract((p3.x + p3.y) * p3.z);
}

fn reconstruct_pos(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0);
    let clip = vec4(ndc, depth, 1.0);
    let wh = u.inv_view_proj * clip;
    return wh.xyz / wh.w;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    let depth = textureLoad(depth_tex, pixel, 0);

    // Reconstruct world position and normal (before any branching for dpdx/dpdy)
    let pos = reconstruct_pos(in.uv, depth);
    var n = normalize(cross(dpdx(pos), dpdy(pos)));
    // Ensure normal faces camera
    n *= select(1.0, -1.0, dot(n, pos - u.camera_pos) > 0.0);

    // Sky: no occlusion
    if depth >= 0.999 {
        return vec4(1.0);
    }

    let dims = vec2<f32>(textureDimensions(depth_tex));
    let pixel_f = in.position.xy;

    var occ = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        let fi = f32(i);
        let r1 = hash_f(pixel_f + vec2(fi * 7.13, fi * 11.31));
        let r2 = hash_f(pixel_f + vec2(fi * 3.77 + 37.0, fi * 5.91 + 91.0));
        let r3 = hash_f(pixel_f + vec2(fi * 13.37 + 59.0, fi * 2.71 + 23.0));

        // Random direction on unit sphere
        let theta = r1 * 6.28318;
        let cos_phi = 2.0 * r2 - 1.0;
        let sin_phi = sqrt(max(1.0 - cos_phi * cos_phi, 0.0));
        var offset = vec3(sin_phi * cos(theta), sin_phi * sin(theta), cos_phi);

        // Flip into hemisphere around surface normal
        offset = select(-offset, offset, dot(offset, n) >= 0.0);

        // Distribute samples within radius (quadratic bias toward center)
        let scale = max(r3 * r3, 0.1);
        let sample_pos = pos + offset * RADIUS * scale;

        // Project sample to screen
        let proj = u.view_proj * vec4(sample_pos, 1.0);
        let proj_w = max(proj.w, 0.001);
        let proj_ndc = proj.xyz / proj_w;
        let proj_uv = vec2(proj_ndc.x * 0.5 + 0.5, 1.0 - (proj_ndc.y * 0.5 + 0.5));
        let proj_pixel = vec2<i32>(proj_uv * dims);

        let in_bounds = proj.w > 0.0
            && proj_pixel.x >= 0 && proj_pixel.x < i32(dims.x)
            && proj_pixel.y >= 0 && proj_pixel.y < i32(dims.y);

        if in_bounds {
            let actual_depth = textureLoad(depth_tex, proj_pixel, 0);
            let actual_linear = linearize(actual_depth);
            let expected_linear = linearize(clamp(proj_ndc.z, 0.0, 1.0));

            // Occluded if actual surface is closer than our sample point
            let diff = expected_linear - actual_linear;
            let range_ok = smoothstep(RADIUS, 0.0, diff);
            occ += step(0.05, diff) * range_ok;
        }
    }

    let ao = clamp(1.0 - (occ / f32(SAMPLES)) * STRENGTH, 0.0, 1.0);
    return vec4(vec3(ao), 1.0);
}
