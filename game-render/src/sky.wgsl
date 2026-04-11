// Sky-specific: gradient, sun disc/glow, procedural clouds.
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

    // --- Procedural clouds ---
    let cloud_altitude = 120.0;
    var cloud_density = 0.0;

    if (ray_dir.y > 0.005) {
        // Intersect ray with cloud plane
        let dist_to_cloud = (cloud_altitude - u.camera_pos.y) / ray_dir.y;
        let cloud_xz = u.camera_pos.xz + ray_dir.xz * dist_to_cloud;

        // Sample noise at cloud position with slow drift
        let cloud_scale = 500.0;
        let drift = vec2(u.time * 6.0, u.time * 2.0);
        let sample_pos = (cloud_xz + drift) / cloud_scale;

        cloud_density = fbm3(sample_pos);

        // Shape clouds: threshold + smooth falloff
        let coverage = 0.35;
        cloud_density = smoothstep(coverage, coverage + 0.25, cloud_density);

        // Fade near horizon to avoid hard cutoff
        let horizon_fade = smoothstep(0.005, 0.2, ray_dir.y);
        cloud_density *= horizon_fade;

        // Distance fade
        let cloud_dist_fade = 1.0 - smoothstep(800.0, 2000.0, dist_to_cloud);
        cloud_density *= cloud_dist_fade;

        // Cloud lighting: sun illumination on cloud tops
        let sun_up = max(u.sun_dir.y, 0.0);
        let illumination = sun_up * 0.7 + 0.3;

        let cloud_bright = u.sun_color * illumination * 1.1;
        let cloud_shadow = mix(u.sky_zenith * 0.4, u.sky_horizon * 0.5, 0.5);

        let shade_factor = smoothstep(0.0, 0.6, cloud_density);
        let cloud_color = mix(cloud_bright, cloud_shadow, shade_factor * 0.4);

        let cloud_silver = pow(max(sun_dot, 0.0), 8.0) * (1.0 - cloud_density) * 0.3;
        let lit_cloud = cloud_color + u.sun_color * cloud_silver;

        color = mix(color, lit_cloud, cloud_density);
    }

    // Sun disc attenuated by cloud density
    color = mix(color, sun_intensity, disc * (1.0 - cloud_density * 0.85));

    return vec4(color, 1.0);
}
