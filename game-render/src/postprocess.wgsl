@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Fullscreen triangle (3 vertices, no buffer needed)
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

    // ACES tone mapping (HDR → LDR with filmic curve)
    color = aces_tonemap(color);

    // Color grading: warm shadows
    let luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
    let shadow_weight = saturate(1.0 - luminance * 2.0);
    color.r += shadow_weight * 0.02;

    // Saturation boost (ACES desaturates, compensate)
    let grey = vec3(luminance);
    color = mix(grey, color, 1.15);

    // Gentle S-curve contrast (half-strength to avoid crushing)
    color = mix(color, smoothstep(vec3(0.0), vec3(1.0), color), 0.5);

    // Vignette: smooth radial darkening at edges
    let center = in.uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 0.4;
    color *= vignette;

    return vec4(color, 1.0);
}
