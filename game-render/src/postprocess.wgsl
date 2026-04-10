@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var ao_texture: texture_2d<f32>;

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

    // --- Colored SSAO with wide soft blur ---
    // 9-tap Gaussian-approximate blur on half-res AO texture
    // Weights: center 4, cross 2, corners 1 (sum = 16)
    // At 2.5 texel spread with bilinear, covers ~12 full-res pixels
    let ao_texel = 1.0 / vec2<f32>(textureDimensions(ao_texture));
    let t = ao_texel * 1.5;
    let ao = (
        textureSample(ao_texture, hdr_sampler, in.uv).r * 4.0
        + textureSample(ao_texture, hdr_sampler, in.uv + vec2(-t.x,  0.0)).r * 2.0
        + textureSample(ao_texture, hdr_sampler, in.uv + vec2( t.x,  0.0)).r * 2.0
        + textureSample(ao_texture, hdr_sampler, in.uv + vec2( 0.0, -t.y)).r * 2.0
        + textureSample(ao_texture, hdr_sampler, in.uv + vec2( 0.0,  t.y)).r * 2.0
        + textureSample(ao_texture, hdr_sampler, in.uv + vec2(-t.x, -t.y)).r
        + textureSample(ao_texture, hdr_sampler, in.uv + vec2( t.x, -t.y)).r
        + textureSample(ao_texture, hdr_sampler, in.uv + vec2(-t.x,  t.y)).r
        + textureSample(ao_texture, hdr_sampler, in.uv + vec2( t.x,  t.y)).r
    ) / 16.0;

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
