@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var ao_texture: texture_2d<f32>;
@group(0) @binding(3) var depth_texture: texture_depth_2d;

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

    // Slight exposure boost before tone mapping (combats ACES desaturation)
    color *= 1.08;

    // ACES tone mapping (HDR -> LDR with filmic curve)
    color = aces_tonemap(color);

    // Color grading: warm amber shadows (impressionist warmth in darks)
    let luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
    let shadow_weight = saturate(1.0 - luminance * 2.5);
    color += shadow_weight * vec3(0.025, 0.012, -0.005);

    // Saturation boost (ACES desaturates, compensate with richer midtones)
    let grey = vec3(luminance);
    color = mix(grey, color, 1.18);

    // Cool blue fill for dark areas (ambient moonlight / night readability)
    let dark_fill = saturate(1.0 - luminance * 4.0);
    color += dark_fill * dark_fill * vec3(0.018, 0.025, 0.050);

    // Gentle S-curve contrast (lower blend to preserve dark greens)
    color = mix(color, smoothstep(vec3(0.0), vec3(1.0), color), 0.25);

    // Vignette
    let center = in.uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 0.35;
    color *= vignette;

    return vec4(color, 1.0);
}
