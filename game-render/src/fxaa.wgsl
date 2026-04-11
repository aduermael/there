// FXAA 3.11 (Timothy Lottes) — single fullscreen fragment pass.
// Operates on LDR (post-tonemapped) image. Fullscreen VS from fullscreen.wgsl.

@group(0) @binding(0) var fxaa_tex: texture_2d<f32>;
@group(0) @binding(1) var fxaa_sampler: sampler;

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.299, 0.587, 0.114));
}

const EDGE_THRESHOLD: f32 = 0.125;
const EDGE_THRESHOLD_MIN: f32 = 0.0625;
const SUBPIX_QUALITY: f32 = 0.75;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(fxaa_tex));
    let uv = in.uv;

    // Center and 4 cardinal neighbors
    let rgbM = textureSample(fxaa_tex, fxaa_sampler, uv).rgb;
    let lumaM = luma(rgbM);
    let lumaN = luma(textureSample(fxaa_tex, fxaa_sampler, uv + vec2(0.0, -t.y)).rgb);
    let lumaS = luma(textureSample(fxaa_tex, fxaa_sampler, uv + vec2(0.0, t.y)).rgb);
    let lumaW = luma(textureSample(fxaa_tex, fxaa_sampler, uv + vec2(-t.x, 0.0)).rgb);
    let lumaE = luma(textureSample(fxaa_tex, fxaa_sampler, uv + vec2(t.x, 0.0)).rgb);

    // Range check — skip low-contrast pixels
    let lumaMin = min(lumaM, min(min(lumaN, lumaS), min(lumaW, lumaE)));
    let lumaMax = max(lumaM, max(max(lumaN, lumaS), max(lumaW, lumaE)));
    let lumaRange = lumaMax - lumaMin;
    let is_edge = lumaRange >= max(EDGE_THRESHOLD_MIN, lumaMax * EDGE_THRESHOLD);

    // 4 diagonal neighbors for edge direction + sub-pixel detection
    let lumaNW = luma(textureSample(fxaa_tex, fxaa_sampler, uv + vec2(-t.x, -t.y)).rgb);
    let lumaNE = luma(textureSample(fxaa_tex, fxaa_sampler, uv + vec2(t.x, -t.y)).rgb);
    let lumaSW = luma(textureSample(fxaa_tex, fxaa_sampler, uv + vec2(-t.x, t.y)).rgb);
    let lumaSE = luma(textureSample(fxaa_tex, fxaa_sampler, uv + vec2(t.x, t.y)).rgb);

    // Sub-pixel aliasing factor
    let lumaAvg = (lumaN + lumaS + lumaW + lumaE) * 0.25;
    let subpixA = saturate(abs(lumaAvg - lumaM) / max(lumaRange, 0.001));
    let subpixC = (-2.0 * subpixA + 3.0) * subpixA * subpixA * SUBPIX_QUALITY;

    // Edge direction: horizontal if vertical gradient dominates
    let edgeH = abs(lumaNW - 2.0 * lumaN + lumaNE)
              + 2.0 * abs(lumaW - 2.0 * lumaM + lumaE)
              + abs(lumaSW - 2.0 * lumaS + lumaSE);
    let edgeV = abs(lumaNW - 2.0 * lumaW + lumaSW)
              + 2.0 * abs(lumaN - 2.0 * lumaM + lumaS)
              + abs(lumaNE - 2.0 * lumaE + lumaSE);
    let isH = edgeH >= edgeV;

    // Perpendicular neighbor pair
    let luma1 = select(lumaW, lumaN, isH);
    let luma2 = select(lumaE, lumaS, isH);
    let grad1 = abs(luma1 - lumaM);
    let grad2 = abs(luma2 - lumaM);
    let stepLen = select(t.x, t.y, isH);

    // Step toward the steeper gradient side
    let steep1 = grad1 >= grad2;
    let lumaEdge = 0.5 * (select(luma2, luma1, steep1) + lumaM);
    let gradScaled = max(grad1, grad2) * 0.25;
    let perpSign = select(1.0, -1.0, steep1);

    // Starting UV: offset half a pixel perpendicular to the edge
    var uvE = uv;
    if isH { uvE.y += perpSign * stepLen * 0.5; } else { uvE.x += perpSign * stepLen * 0.5; }

    // Search direction: along the edge
    let ss = select(vec2(0.0, t.y), vec2(t.x, 0.0), isH);

    // Search both directions for the edge end (12 steps)
    // Always sample unconditionally to maintain uniform control flow.
    var uvN = uvE - ss;
    var uvP = uvE + ss;
    var endN = luma(textureSample(fxaa_tex, fxaa_sampler, uvN).rgb) - lumaEdge;
    var endP = luma(textureSample(fxaa_tex, fxaa_sampler, uvP).rgb) - lumaEdge;
    var reachedN = abs(endN) >= gradScaled;
    var reachedP = abs(endP) >= gradScaled;

    for (var i = 0; i < 12; i++) {
        // No break — loop must run uniformly for textureSample compliance
        if !reachedN { uvN -= ss; }
        if !reachedP { uvP += ss; }
        // Always sample both unconditionally
        let sN = luma(textureSample(fxaa_tex, fxaa_sampler, uvN).rgb) - lumaEdge;
        let sP = luma(textureSample(fxaa_tex, fxaa_sampler, uvP).rgb) - lumaEdge;
        if !reachedN {
            endN = sN;
            reachedN = abs(endN) >= gradScaled;
        }
        if !reachedP {
            endP = sP;
            reachedP = abs(endP) >= gradScaled;
        }
    }

    // Edge blend: offset based on distance to closest edge end
    let distN = select(uv.y - uvN.y, uv.x - uvN.x, isH);
    let distP = select(uvP.y - uv.y, uvP.x - uv.x, isH);
    let totalDist = distN + distP;
    var edgeBlend = 0.5 - min(distN, distP) / totalDist;

    // Validate: only blend if luminance change is consistent
    let centerSmaller = lumaM < lumaEdge;
    let goodN = (endN < 0.0) != centerSmaller;
    let goodP = (endP < 0.0) != centerSmaller;
    if (distN < distP && !goodN) || (distN >= distP && !goodP) {
        edgeBlend = 0.0;
    }

    // Final UV offset: max of edge-based and sub-pixel offsets
    let finalOffset = max(edgeBlend, subpixC);
    var finalUV = uv;
    if isH { finalUV.y += perpSign * finalOffset * stepLen; }
    else   { finalUV.x += perpSign * finalOffset * stepLen; }

    let fxaaResult = textureSample(fxaa_tex, fxaa_sampler, finalUV);

    // Select: passthrough for low-contrast pixels, FXAA result for edges
    return select(vec4(rgbM, 1.0), fxaaResult, is_edge);
}
