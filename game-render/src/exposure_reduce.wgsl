// Reduce luminance histogram to a single exposure value.
// Trims top/bottom 5%, computes weighted average, applies EMA smoothing.

@group(0) @binding(0) var<storage, read> histogram: array<u32, 256>;
@group(0) @binding(1) var<storage, read_write> exposure: array<f32, 1>;

const LOG_MIN: f32 = -8.0;
const LOG_RANGE: f32 = 12.0;
const ADAPT_SPEED: f32 = 0.05;

@compute @workgroup_size(1, 1, 1)
fn cs_main() {
    // Total pixel count
    var total = 0u;
    for (var i = 0u; i < 256u; i++) {
        total += histogram[i];
    }

    if total == 0u {
        return;
    }

    // Find 5th percentile bin (low trim)
    let trim_count = u32(f32(total) * 0.05);
    var cumulative = 0u;
    var low_bin = 0u;
    for (var i = 0u; i < 256u; i++) {
        cumulative += histogram[i];
        if cumulative >= trim_count {
            low_bin = i;
            break;
        }
    }

    // Find 95th percentile bin (high trim)
    cumulative = 0u;
    var high_bin = 255u;
    for (var i = 0u; i < 256u; i++) {
        let ri = 255u - i;
        cumulative += histogram[ri];
        if cumulative >= trim_count {
            high_bin = ri;
            break;
        }
    }

    // Weighted average luminance in trimmed range
    var sum = 0.0;
    var weight = 0u;
    for (var i = low_bin; i <= high_bin; i++) {
        let bin_lum = exp2(LOG_MIN + (f32(i) + 0.5) / 256.0 * LOG_RANGE);
        sum += bin_lum * f32(histogram[i]);
        weight += histogram[i];
    }

    if weight == 0u {
        return;
    }

    let avg_lum = sum / f32(weight);
    let target_exposure = 0.18 / max(avg_lum, 0.001);
    let clamped = clamp(target_exposure, 0.15, 2.0);

    // EMA smoothing toward target
    let current = exposure[0];
    exposure[0] = mix(current, clamped, ADAPT_SPEED);
}
