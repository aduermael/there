// Compute bloom: threshold extraction, 13-tap downscale, 9-tap tent upscale.
// Three entry points: cs_downsample_first (threshold + downsample),
// cs_downsample (downsample only), cs_upsample (upsample + blend).

// Shared bindings for all entry points
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var dst_tex: texture_storage_2d<rgba16float, write>;

// Extra binding for upsample: current level's downscale data to blend with
@group(1) @binding(0) var blend_tex: texture_2d<f32>;

// Soft threshold for bloom extraction
fn soft_threshold(color: vec3<f32>) -> vec3<f32> {
    let brightness = max(color.r, max(color.g, color.b));
    let threshold = 0.6;
    let knee = 0.3;
    let soft = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
    let contribution = soft * soft / (4.0 * knee + 0.0001);
    let factor = max(contribution, brightness - threshold) / max(brightness, 0.0001);
    return color * max(factor, 0.0);
}

// 13-tap downsample (Jimenez, SIGGRAPH 2014)
// Samples 13 positions in a 5x5 pattern with overlapping quads for alias-free downscale.
fn downsample_13tap(uv: vec2<f32>, t: vec2<f32>) -> vec3<f32> {
    //  a . b . c
    //  . j . k .
    //  d . e . f
    //  . l . m .
    //  g . h . i
    let a = textureSampleLevel(src_tex, src_sampler, uv + vec2(-2.0 * t.x,  2.0 * t.y), 0.0).rgb;
    let b = textureSampleLevel(src_tex, src_sampler, uv + vec2(       0.0,  2.0 * t.y), 0.0).rgb;
    let c = textureSampleLevel(src_tex, src_sampler, uv + vec2( 2.0 * t.x,  2.0 * t.y), 0.0).rgb;

    let d = textureSampleLevel(src_tex, src_sampler, uv + vec2(-2.0 * t.x,        0.0), 0.0).rgb;
    let e = textureSampleLevel(src_tex, src_sampler, uv,                                 0.0).rgb;
    let f = textureSampleLevel(src_tex, src_sampler, uv + vec2( 2.0 * t.x,        0.0), 0.0).rgb;

    let g = textureSampleLevel(src_tex, src_sampler, uv + vec2(-2.0 * t.x, -2.0 * t.y), 0.0).rgb;
    let h = textureSampleLevel(src_tex, src_sampler, uv + vec2(       0.0, -2.0 * t.y), 0.0).rgb;
    let i = textureSampleLevel(src_tex, src_sampler, uv + vec2( 2.0 * t.x, -2.0 * t.y), 0.0).rgb;

    let j = textureSampleLevel(src_tex, src_sampler, uv + vec2(-t.x,  t.y), 0.0).rgb;
    let k = textureSampleLevel(src_tex, src_sampler, uv + vec2( t.x,  t.y), 0.0).rgb;
    let l = textureSampleLevel(src_tex, src_sampler, uv + vec2(-t.x, -t.y), 0.0).rgb;
    let m = textureSampleLevel(src_tex, src_sampler, uv + vec2( t.x, -t.y), 0.0).rgb;

    // Weighted combination: center 1/8, corners 1/32 each, edges 1/16 each, inner 1/8 each
    var result = e * 0.125;
    result += (a + c + g + i) * 0.03125;
    result += (b + d + f + h) * 0.0625;
    result += (j + k + l + m) * 0.125;
    return result;
}

// 9-tap tent filter [1,2,1]x[1,2,1]/16 for smooth upsampling
fn upsample_tent(uv: vec2<f32>, t: vec2<f32>) -> vec3<f32> {
    var s = textureSampleLevel(src_tex, src_sampler, uv + vec2(-t.x, -t.y), 0.0).rgb;
    s += textureSampleLevel(src_tex, src_sampler, uv + vec2( 0.0, -t.y), 0.0).rgb * 2.0;
    s += textureSampleLevel(src_tex, src_sampler, uv + vec2( t.x, -t.y), 0.0).rgb;
    s += textureSampleLevel(src_tex, src_sampler, uv + vec2(-t.x,  0.0), 0.0).rgb * 2.0;
    s += textureSampleLevel(src_tex, src_sampler, uv,                     0.0).rgb * 4.0;
    s += textureSampleLevel(src_tex, src_sampler, uv + vec2( t.x,  0.0), 0.0).rgb * 2.0;
    s += textureSampleLevel(src_tex, src_sampler, uv + vec2(-t.x,  t.y), 0.0).rgb;
    s += textureSampleLevel(src_tex, src_sampler, uv + vec2( 0.0,  t.y), 0.0).rgb * 2.0;
    s += textureSampleLevel(src_tex, src_sampler, uv + vec2( t.x,  t.y), 0.0).rgb;
    return s / 16.0;
}

// First downsample pass: threshold extraction + 13-tap downsample
@compute @workgroup_size(8, 8, 1)
fn cs_downsample_first(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(dst_tex);
    if id.x >= dims.x || id.y >= dims.y { return; }

    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(dims);
    let t = 1.0 / vec2<f32>(textureDimensions(src_tex));
    let color = soft_threshold(downsample_13tap(uv, t));
    textureStore(dst_tex, id.xy, vec4(color, 1.0));
}

// Subsequent downsample passes: 13-tap downsample only
@compute @workgroup_size(8, 8, 1)
fn cs_downsample(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(dst_tex);
    if id.x >= dims.x || id.y >= dims.y { return; }

    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(dims);
    let t = 1.0 / vec2<f32>(textureDimensions(src_tex));
    textureStore(dst_tex, id.xy, vec4(downsample_13tap(uv, t), 1.0));
}

// Upsample pass: 9-tap tent upsample from smaller mip + blend with current level's downscale data
@compute @workgroup_size(8, 8, 1)
fn cs_upsample(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(dst_tex);
    if id.x >= dims.x || id.y >= dims.y { return; }

    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(dims);
    let t = 1.0 / vec2<f32>(textureDimensions(src_tex));
    let upsampled = upsample_tent(uv, t);
    let current = textureSampleLevel(blend_tex, src_sampler, uv, 0.0).rgb;
    textureStore(dst_tex, id.xy, vec4(upsampled + current, 1.0));
}
