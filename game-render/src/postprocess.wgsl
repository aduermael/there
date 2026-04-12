// Post-processing: tone mapping, AO blur, god rays, color grading, vignette.
// Uniforms from uniforms.wgsl, noise from noise.wgsl, fullscreen VS from fullscreen.wgsl.

@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var ao_texture: texture_2d<f32>;
@group(0) @binding(3) var depth_texture: texture_depth_2d;
@group(0) @binding(4) var bloom_texture: texture_2d<f32>;
@group(0) @binding(5) var<storage, read> exposure_buf: array<f32, 1>;

@group(1) @binding(0) var<uniform> u: Uniforms;

// ACES fitted tone mapping (Narkowicz 2015)
fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return saturate((x * (a * x + b)) / (x * (c * x + d) + e));
}

// Screen-space contact shadows: short-range ray march along sun direction
fn contact_shadow(uv: vec2<f32>, pixel: vec2<f32>) -> f32 {
    // Skip when sun is below horizon
    if u.sun_dir.y < 0.01 {
        return 1.0;
    }

    let depth_dims = vec2<f32>(textureDimensions(depth_texture));
    let d_pixel = vec2<i32>(uv * depth_dims);
    let raw_depth = textureLoad(depth_texture, d_pixel, 0);

    // Skip sky pixels
    if raw_depth > 0.999 {
        return 1.0;
    }

    // Reconstruct world position from depth
    let ndc = vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, raw_depth, 1.0);
    let world_h = u.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;

    // March a short distance along sun direction in world space, project each step to screen
    let march_distance = 1.5; // world units — short range for fine contact detail
    let step_world = u.sun_dir * march_distance / 12.0;

    // Per-pixel jitter to break banding
    let jitter = ign(pixel) * 0.5 + 0.5;

    var occluded = 0.0;
    var pos = world_pos + step_world * jitter;

    for (var i = 0; i < 12; i++) {
        pos += step_world;

        // Project to screen
        let clip = u.view_proj * vec4(pos, 1.0);
        if clip.w <= 0.0 { continue; }
        let proj_ndc = clip.xyz / clip.w;
        let proj_uv = vec2(proj_ndc.x * 0.5 + 0.5, 1.0 - (proj_ndc.y * 0.5 + 0.5));

        if proj_uv.x < 0.0 || proj_uv.x > 1.0 || proj_uv.y < 0.0 || proj_uv.y > 1.0 {
            continue;
        }

        // Compare projected depth with scene depth
        let sample_pixel = vec2<i32>(proj_uv * depth_dims);
        let scene_depth = textureLoad(depth_texture, sample_pixel, 0);
        let march_depth = proj_ndc.z;

        // Occluded if scene is closer than our marched point (with small bias)
        let depth_diff = march_depth - scene_depth;
        if depth_diff > 0.0002 && depth_diff < 0.01 {
            occluded += 1.0;
        }
    }

    return 1.0 - saturate(occluded / 4.0) * 0.6;
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

        // Clamp to valid range; mask out-of-bounds contributions
        let in_bounds = f32(sample_uv.x >= 0.0 && sample_uv.x <= 1.0 && sample_uv.y >= 0.0 && sample_uv.y <= 1.0);
        let safe_uv = clamp(sample_uv, vec2(0.0), vec2(1.0));

        // Depth-based occlusion: sky pixels (depth ~1.0) pass light, geometry blocks
        let d_pixel = vec2<i32>(safe_uv * depth_dims);
        let d = textureLoad(depth_texture, d_pixel, 0);
        let is_sky = smoothstep(0.998, 0.9999, d);

        // Also weight by scene brightness (sun glow is brighter than plain sky)
        let sample_color = textureSample(hdr_texture, hdr_sampler, safe_uv).rgb;
        let brightness = min(dot(sample_color, vec3(0.2126, 0.7152, 0.0722)), 2.0);

        // Sky contributes light, geometry occludes. Weight decays along march.
        let w = 1.0 - f32(i) * step_size * 0.7;
        accumulation += is_sky * brightness * w * in_bounds;
        weight_sum += w * in_bounds;
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

    // --- Contact shadows (fine detail near geometry edges) ---
    let cs = contact_shadow(in.uv, in.position.xy);
    color *= cs;

    // Night detection from sun color intensity (scotopic vision = less color)
    let sun_intensity = dot(u.sun_color, vec3(0.333));
    let night_factor = 1.0 - smoothstep(0.3, 0.8, sun_intensity);

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

    // Colored AO: warm by day (impressionistic), cold blue-gray at night
    let warm_ao = vec3(0.5, 0.4, 0.35);
    let cold_ao = vec3(0.35, 0.38, 0.45);
    let shadow_warmth = mix(warm_ao, cold_ao, night_factor);
    color *= mix(shadow_warmth, vec3(1.0), ao);

    // --- God rays (additive, in HDR before tone mapping) ---
    color += god_rays(in.uv, in.position.xy);

    // --- Bloom (additive, half-res upsampled via bilinear) ---
    let bloom = textureSample(bloom_texture, hdr_sampler, in.uv).rgb;
    color += bloom * 0.6;

    // --- Auto-exposure (compute histogram → trimmed average → EMA adaptation) ---
    let exposure = exposure_buf[0];
    color *= exposure;

    // ACES tone mapping (HDR -> LDR with filmic curve)
    color = aces_tonemap(color);

    // Color grading: warm amber shadows by day, cold at night
    let luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
    let shadow_weight = saturate(1.0 - luminance * 2.5);
    let day_warmth = vec3(0.025, 0.012, -0.005);
    let night_coolth = vec3(-0.005, -0.002, 0.010);
    color += shadow_weight * mix(day_warmth, night_coolth, night_factor);

    // Saturation boost: strong by day (1.28), heavily reduced at night (scotopic desaturation)
    let grey = vec3(luminance);
    let sat_boost = mix(1.28, 0.20, night_factor);
    color = mix(grey, color, sat_boost);

    // Subtle cool fill for very dark areas (cold blue, not purple)
    let dark_fill = saturate(1.0 - luminance * 4.0);
    color += dark_fill * dark_fill * vec3(0.010, 0.014, 0.030);

    // S-curve contrast for visual punch
    color = mix(color, smoothstep(vec3(0.0), vec3(1.0), color), 0.32);

    // Vignette
    let center = in.uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 0.35;
    color *= vignette;

    return vec4(color, 1.0);
}
