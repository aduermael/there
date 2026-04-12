// Compute luminance histogram from HDR buffer.
// 256 bins in log2 space, shared-memory accumulation per workgroup.

@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> histogram: array<atomic<u32>, 256>;

const LOG_MIN: f32 = -8.0;
const LOG_RANGE: f32 = 12.0;

var<workgroup> local_hist: array<atomic<u32>, 256>;

@compute @workgroup_size(16, 16, 1)
fn cs_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) lid: u32,
) {
    // Clear shared histogram
    if lid < 256u {
        atomicStore(&local_hist[lid], 0u);
    }
    workgroupBarrier();

    let tex_size = textureDimensions(hdr_texture);
    if gid.x < tex_size.x && gid.y < tex_size.y {
        let color = textureLoad(hdr_texture, vec2<i32>(gid.xy), 0).rgb;
        let luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));

        // Skip very dark pixels (noise floor)
        if luminance > 0.001 {
            let log_lum = log2(luminance);
            let normalized = clamp((log_lum - LOG_MIN) / LOG_RANGE, 0.0, 1.0);
            let bin = min(u32(normalized * 256.0), 255u);
            atomicAdd(&local_hist[bin], 1u);
        }
    }

    workgroupBarrier();

    // Flush shared histogram to global
    if lid < 256u {
        let count = atomicLoad(&local_hist[lid]);
        if count > 0u {
            atomicAdd(&histogram[lid], count);
        }
    }
}
