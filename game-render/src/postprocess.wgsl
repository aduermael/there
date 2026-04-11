@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var ao_texture: texture_2d<f32>;
@group(0) @binding(3) var depth_texture: texture_depth_2d;

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

@group(1) @binding(0) var<uniform> u: Uniforms;

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

// ACES fitted tone mapping (Narkowicz 2015)
fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return saturate((x * (a * x + b)) / (x * (c * x + d) + e));
}

// Interleaved Gradient Noise for per-pixel jitter (breaks banding)
fn ign(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(0.06711056 * pixel.x + 0.00583715 * pixel.y));
}

// Screen-space radial god rays using depth-based occlusion
fn god_rays(uv: vec2<f32>, pixel: vec2<f32>) -> vec3<f32> {
    // Compute sun screen position from sun_dir (place sun far away along direction)
    let sun_world = u.camera_pos + u.sun_dir * 200.0;
    let sun_clip = u.view_proj * vec4(sun_world, 1.0);

    // Sun behind camera — no rays
    if sun_clip.w <= 0.0 {
        return vec3(0.0);
    }

    let sun_ndc = sun_clip.xy / sun_clip.w;
    let sun_uv = vec2(sun_ndc.x * 0.5 + 0.5, 1.0 - (sun_ndc.y * 0.5 + 0.5));

    // Ray intensity: strongest at low sun angles (dawn/dusk), fades toward noon, off at night
    let elevation = u.sun_dir.y;
    if elevation < 0.005 {
        return vec3(0.0);
    }
    // Full intensity at horizon, fading as sun rises toward noon
    let angle_intensity = 1.0 - smoothstep(0.20, 0.70, elevation);

    // Radial march toward sun position in screen space
    let delta = sun_uv - uv;
    let ray_length = length(delta);

    // Fade by distance from sun in screen space
    let distance_fade = 1.0 - smoothstep(0.0, 1.4, ray_length);

    let depth_dims = vec2<f32>(textureDimensions(depth_texture));

    const NUM_STEPS: i32 = 20;
    let step_size = 1.0 / f32(NUM_STEPS);
    // March toward sun — cover enough to resolve tree silhouettes
    let step_delta = delta * step_size * 0.6;

    // Per-pixel jitter to break banding
    let jitter = ign(pixel);

    var accumulation = 0.0;
    var sample_uv = uv + step_delta * jitter;
    var weight_sum = 0.0;

    for (var i = 0; i < NUM_STEPS; i++) {
        sample_uv += step_delta;

        // Skip out-of-bounds samples
        if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
            continue;
        }

        // Depth-based occlusion: sky pixels (depth ~1.0) pass light, geometry blocks
        let d_pixel = vec2<i32>(sample_uv * depth_dims);
        let d = textureLoad(depth_texture, d_pixel, 0);
        let is_sky = smoothstep(0.998, 0.9999, d);

        // Also weight by scene brightness (sun glow is brighter than plain sky)
        let sample_color = textureSample(hdr_texture, hdr_sampler, sample_uv).rgb;
        let brightness = min(dot(sample_color, vec3(0.2126, 0.7152, 0.0722)), 2.0);

        // Sky contributes light, geometry occludes. Weight decays along march.
        let w = 1.0 - f32(i) * step_size * 0.7;
        accumulation += is_sky * brightness * w;
        weight_sum += w;
    }

    if weight_sum > 0.0 {
        accumulation /= weight_sum;
    }

    // Intensity cap to prevent blowout near sun
    accumulation = min(accumulation, 0.8);

    // Final ray color: tinted by sun color, scaled by elevation and distance
    let ray_strength = 0.45;
    let rays = accumulation * angle_intensity * distance_fade * ray_strength;
    return u.sun_color * rays;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(hdr_texture, hdr_sampler, in.uv).rgb;

    // --- Depth-aware bilateral SSAO blur ---
    // 9-tap blur that respects depth edges: smooth within surfaces, sharp across silhouettes
    let ao_texel = 1.0 / vec2<f32>(textureDimensions(ao_texture));
    let depth_dims = vec2<f32>(textureDimensions(depth_texture));
    let t = ao_texel * 1.5;

    // Center depth (full-res, loaded at the pixel corresponding to this UV)
    let depth_pixel = vec2<i32>(in.uv * depth_dims);
    let center_depth = textureLoad(depth_texture, depth_pixel, 0);

    // Bilateral weights: Gaussian spatial * depth similarity
    let depth_threshold = 0.002;
    var ao_sum = textureSample(ao_texture, hdr_sampler, in.uv).r * 4.0;
    var weight_sum = 4.0;

    let offsets = array<vec2<f32>, 8>(
        vec2(-t.x,  0.0), vec2( t.x,  0.0), vec2( 0.0, -t.y), vec2( 0.0,  t.y),
        vec2(-t.x, -t.y), vec2( t.x, -t.y), vec2(-t.x,  t.y), vec2( t.x,  t.y)
    );
    let spatial_weights = array<f32, 8>(2.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0);

    for (var i = 0u; i < 8u; i++) {
        let sample_uv = in.uv + offsets[i];
        let sample_depth_pixel = vec2<i32>(sample_uv * depth_dims);
        let sample_depth = textureLoad(depth_texture, sample_depth_pixel, 0);
        let depth_diff = abs(sample_depth - center_depth);
        let depth_weight = select(0.05, 1.0, depth_diff < depth_threshold);
        let w = spatial_weights[i] * depth_weight;
        ao_sum += textureSample(ao_texture, hdr_sampler, sample_uv).r * w;
        weight_sum += w;
    }
    let ao = ao_sum / weight_sum;

    // Colored AO: occluded areas shift warm (impressionistic shadow tone)
    let shadow_warmth = vec3(0.5, 0.4, 0.35);
    color *= mix(shadow_warmth, vec3(1.0), ao);

    // --- God rays (additive, in HDR before tone mapping) ---
    color += god_rays(in.uv, in.position.xy);

    // ACES tone mapping (HDR -> LDR with filmic curve)
    color = aces_tonemap(color);

    // Color grading: warm amber shadows (impressionist warmth in darks)
    let luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
    let shadow_weight = saturate(1.0 - luminance * 2.5);
    color += shadow_weight * vec3(0.025, 0.012, -0.005);

    // Saturation boost (ACES desaturates — compensate for rich, painterly color)
    let grey = vec3(luminance);
    color = mix(grey, color, 1.28);

    // Subtle cool fill for very dark areas (night readability)
    let dark_fill = saturate(1.0 - luminance * 4.0);
    color += dark_fill * dark_fill * vec3(0.018, 0.024, 0.055);

    // S-curve contrast for visual punch
    color = mix(color, smoothstep(vec3(0.0), vec3(1.0), color), 0.32);

    // Vignette
    let center = in.uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 0.35;
    color *= vignette;

    return vec4(color, 1.0);
}
