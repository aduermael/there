// Screen-Space Ambient Occlusion — reads scene depth, outputs AO to R8Unorm.
// v2: Fixed half-res coordinate mapping, IGN noise, TBN hemisphere sampling.
// 1.0 = unoccluded, 0.0 = fully occluded.
// Uniforms from uniforms.wgsl, noise from noise.wgsl, fullscreen VS from fullscreen.wgsl.

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var depth_tex: texture_depth_2d;

const SAMPLES: u32 = 8;
const RADIUS: f32 = 1.0;
const STRENGTH: f32 = 2.5;

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
    let rotation = ign(in.position.xy) * TAU;

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
        let proj_uv = ndc_to_uv(proj_ndc.xy);
        let proj_pixel = vec2<i32>(proj_uv * depth_dims);

        let in_bounds = proj.w > 0.0
            && proj_pixel.x >= 0 && proj_pixel.x < i32(depth_dims.x)
            && proj_pixel.y >= 0 && proj_pixel.y < i32(depth_dims.y);

        if in_bounds {
            let actual_depth = textureLoad(depth_tex, proj_pixel, 0);
            let actual_linear = linearize_depth(actual_depth);
            let expected_linear = linearize_depth(clamp(proj_ndc.z, 0.0, 1.0));

            // Occluded when scene surface is closer than sample, with thickness heuristic:
            // accept small positive diffs (real occlusion), reject large ones
            // (sampling through thin geometry to far background)
            let diff = expected_linear - actual_linear;
            let thickness = smoothstep(0.0, 0.04, diff) * (1.0 - smoothstep(RADIUS * 0.5, RADIUS, diff));
            occ += thickness;
        }
    }

    let ao = clamp(1.0 - (occ / f32(SAMPLES)) * STRENGTH, 0.0, 1.0);
    return vec4(vec3(ao), 1.0);
}
