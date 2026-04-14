// Cloud shadow bake: compute pass that writes cloud shadow values to a 2D texture.
// Each texel maps to a world-space XZ position covering the full world (0..world_size).
// Output: rgba8unorm where r = shadow factor (1.0 = fully lit, lower = shadowed).
// Prepended: uniforms.wgsl (Uniforms struct), noise.wgsl (fbm3).

// Cloud layer constants — keep in sync with common.wgsl.
const CLOUD_HIGH_ALTITUDE: f32 = 220.0;
const CLOUD_HIGH_SCALE: f32 = 700.0;
const CLOUD_HIGH_COVERAGE: f32 = 0.38;
const CLOUD_HIGH_OPACITY: f32 = 0.5;
const CLOUD_HIGH_DRIFT: f32 = 1.3;

const CLOUD_MID_ALTITUDE: f32 = 120.0;
const CLOUD_MID_SCALE: f32 = 500.0;
const CLOUD_MID_COVERAGE: f32 = 0.35;
const CLOUD_MID_OPACITY: f32 = 1.0;
const CLOUD_MID_DRIFT: f32 = 1.0;

const CLOUD_LOW_ALTITUDE: f32 = 80.0;
const CLOUD_LOW_SCALE: f32 = 350.0;
const CLOUD_LOW_COVERAGE: f32 = 0.42;
const CLOUD_LOW_OPACITY: f32 = 0.85;
const CLOUD_LOW_DRIFT: f32 = 0.7;

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var output_tex: texture_storage_2d<rgba8unorm, write>;

fn cloud_drift_cs(drift_mult: f32) -> vec2<f32> {
    return vec2(u.time * 6.0, u.time * 2.0) * drift_mult;
}

fn cloud_shadow_layer(world_xz: vec2<f32>, altitude: f32, scale: f32, coverage: f32, drift_mult: f32) -> f32 {
    // Project ground position to cloud plane along sun direction.
    // Uses Y=0 reference; error at terrain height is negligible at cloud scale (80-220 units).
    let t = altitude / max(u.sun_dir.y, 0.001);
    let cloud_xz = world_xz + u.sun_dir.xz * t;
    let drift = cloud_drift_cs(drift_mult);
    let sample_pos = (cloud_xz + drift) / scale;
    let density = fbm3(sample_pos);
    return smoothstep(coverage, coverage + 0.25, density);
}

@compute @workgroup_size(16, 16, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let tex_size = textureDimensions(output_tex);
    if (gid.x >= tex_size.x || gid.y >= tex_size.y) { return; }

    // Map texel center to world XZ
    let world_xz = vec2<f32>(
        (f32(gid.x) + 0.5) / f32(tex_size.x) * u.world_size,
        (f32(gid.y) + 0.5) / f32(tex_size.y) * u.world_size,
    );

    let d_high = cloud_shadow_layer(world_xz, CLOUD_HIGH_ALTITUDE, CLOUD_HIGH_SCALE, CLOUD_HIGH_COVERAGE, CLOUD_HIGH_DRIFT) * CLOUD_HIGH_OPACITY;
    let d_mid  = cloud_shadow_layer(world_xz, CLOUD_MID_ALTITUDE, CLOUD_MID_SCALE, CLOUD_MID_COVERAGE, CLOUD_MID_DRIFT);
    let d_low  = cloud_shadow_layer(world_xz, CLOUD_LOW_ALTITUDE, CLOUD_LOW_SCALE, CLOUD_LOW_COVERAGE, CLOUD_LOW_DRIFT) * CLOUD_LOW_OPACITY;

    let total = min(d_high + d_mid + d_low, 1.0);
    let shadow = 1.0 - total * 0.45;

    textureStore(output_tex, vec2<i32>(gid.xy), vec4(shadow, 0.0, 0.0, 1.0));
}
