// Sky-specific: gradient, sun disc/glow, multi-layer procedural clouds with self-shadowing.
// Uniforms, noise, and shadow bindings provided by common.wgsl prefix.

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Fullscreen triangle: 3 vertices cover the screen without a vertex buffer
@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VertexOutput {
    let x = f32(i32(id & 1u)) * 4.0 - 1.0;
    let y = f32(i32(id >> 1u)) * 4.0 - 1.0;

    var out: VertexOutput;
    out.clip_pos = vec4(x, y, 1.0, 1.0); // z=1.0 (far plane)
    out.uv = vec2(x * 0.5 + 0.5, -y * 0.5 + 0.5);
    return out;
}

// Sample a cloud layer at the given altitude.
// Returns (cloud_color, density) where density is 0..1.
fn sample_cloud_layer(
    ray_dir: vec3<f32>,
    sun_dot: f32,
    altitude: f32,
    scale: f32,
    coverage: f32,
    opacity: f32,
    drift_mult: f32,
) -> vec4<f32> {
    if ray_dir.y <= 0.005 {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }

    let dist = (altitude - u.camera_pos.y) / ray_dir.y;
    let cloud_xz = u.camera_pos.xz + ray_dir.xz * dist;

    let drift = cloud_drift(drift_mult);
    let sample_pos = (cloud_xz + drift) / scale;

    var density = fbm3(sample_pos);
    density = smoothstep(coverage, coverage + 0.25, density);

    // Horizon and distance fade
    let horizon_fade = smoothstep(0.005, 0.2, ray_dir.y);
    let dist_fade = 1.0 - smoothstep(800.0, 2000.0, dist);
    density *= horizon_fade * dist_fade;

    if density < 0.001 {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }

    // Self-shadow: sample density at point offset along sun direction on cloud plane.
    // This simulates light being blocked by denser cloud regions closer to the sun.
    let shadow_reach = 40.0;
    let shadow_xz = cloud_xz + u.sun_dir.xz * shadow_reach / max(u.sun_dir.y, 0.05);
    let shadow_pos = (shadow_xz + drift) / scale;
    let shadow_density = smoothstep(coverage, coverage + 0.25, fbm2(shadow_pos));

    // Cloud lighting
    let sun_up = max(u.sun_dir.y, 0.0);
    let illumination = sun_up * 0.7 + 0.3;

    let cloud_bright = u.sun_color * illumination * 1.1;
    let cloud_dark = mix(u.sky_zenith * 0.4, u.sky_horizon * 0.5, 0.5);

    // Self-shadowing: darker bases where cloud above blocks sun
    let own_shade = smoothstep(0.0, 0.6, density) * 0.4;
    let self_shade = shadow_density * 0.45;
    let total_shade = min(own_shade + self_shade, 1.0);

    let cloud_color = mix(cloud_bright, cloud_dark, total_shade);

    // Silver lining at sun edges
    let silver = pow(max(sun_dot, 0.0), 8.0) * (1.0 - density) * 0.3;
    let lit = cloud_color + u.sun_color * silver;

    return vec4(lit, density * opacity);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Reconstruct view ray from screen UV using inverse view-proj
    let ndc = vec4(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, 1.0, 1.0);
    let world_far = u.inv_view_proj * ndc;
    let ray_dir = normalize(world_far.xyz / world_far.w - u.camera_pos);

    // Sky gradient based on ray direction (up = zenith, horizontal = horizon)
    let up_factor = max(ray_dir.y, 0.0);
    let t = pow(1.0 - up_factor, 2.0);
    var color = mix(u.sky_zenith, u.sky_horizon, t);

    // --- Sun disc and glow (Henyey-Greenstein Mie scattering) ---
    let sun_dot = dot(ray_dir, u.sun_dir);

    // Dual-lobe HG phase function: forward Mie peak + subtle back-scatter corona
    let g_fwd = 0.76;
    let g_back = -0.3;
    let hg_fwd = (1.0 - g_fwd * g_fwd) / pow(1.0 + g_fwd * g_fwd - 2.0 * g_fwd * sun_dot, 1.5);
    let hg_back = (1.0 - g_back * g_back) / pow(1.0 + g_back * g_back - 2.0 * g_back * sun_dot, 1.5);
    let phase = hg_fwd * 0.8 + hg_back * 0.2;

    // Horizon boost: wider, stronger glow when sun is low (dawn/dusk atmosphere path)
    let horizon_boost = 1.0 + (1.0 - max(u.sun_dir.y, 0.0)) * 1.5;
    let glow = phase * 0.12 * horizon_boost;
    color += u.sun_color * glow;

    // Sun disc: small bright circle
    let disc = smoothstep(0.9994, 0.9997, sun_dot);
    let sun_intensity = mix(vec3(1.0, 0.95, 0.85), u.sun_color, 0.3) * 5.0;

    // --- Multi-layer procedural clouds ---
    let c_high = sample_cloud_layer(ray_dir, sun_dot, CLOUD_HIGH_ALTITUDE, CLOUD_HIGH_SCALE, CLOUD_HIGH_COVERAGE, CLOUD_HIGH_OPACITY, CLOUD_HIGH_DRIFT);
    let c_mid = sample_cloud_layer(ray_dir, sun_dot, CLOUD_MID_ALTITUDE, CLOUD_MID_SCALE, CLOUD_MID_COVERAGE, CLOUD_MID_OPACITY, CLOUD_MID_DRIFT);
    let c_low = sample_cloud_layer(ray_dir, sun_dot, CLOUD_LOW_ALTITUDE, CLOUD_LOW_SCALE, CLOUD_LOW_COVERAGE, CLOUD_LOW_OPACITY, CLOUD_LOW_DRIFT);

    // Composite back-to-front (highest layer first, then overlay closer layers)
    var total_cloud_density = 0.0;
    color = mix(color, c_high.rgb, c_high.a);
    total_cloud_density = c_high.a + (1.0 - c_high.a) * total_cloud_density;

    color = mix(color, c_mid.rgb, c_mid.a);
    total_cloud_density = c_mid.a + (1.0 - c_mid.a) * total_cloud_density;

    color = mix(color, c_low.rgb, c_low.a);
    total_cloud_density = c_low.a + (1.0 - c_low.a) * total_cloud_density;

    // Sun disc attenuated by cloud density
    color = mix(color, sun_intensity, disc * (1.0 - total_cloud_density * 0.85));

    return vec4(color, 1.0);
}
