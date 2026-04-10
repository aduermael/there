/// All atmosphere parameters derived from a single `sun_angle`.
/// sun_angle: 0.0 = dawn, 0.25 = noon, 0.5 = dusk, 0.75 = night, 1.0 = dawn again.
pub struct AtmosphereParams {
    pub sun_dir: glam::Vec3,
    pub sun_color: [f32; 3],
    pub sky_zenith: [f32; 3],
    pub sky_horizon: [f32; 3],
    pub fog_color: [f32; 3],
    pub ambient_intensity: f32,
    pub sky_ambient: [f32; 3],
    pub ground_ambient: [f32; 3],
}

pub fn compute_atmosphere(sun_angle: f32) -> AtmosphereParams {
    let theta = sun_angle * std::f32::consts::TAU;

    // Sun direction: orbits east-west, elevation = sin(theta)
    let elevation = theta.sin();
    let east_west = theta.cos();
    let sun_dir = glam::Vec3::new(east_west, elevation.max(0.01), 0.3).normalize();

    // Time-of-day factor: 1.0 at noon (angle=0.25), 0.0 at midnight (angle=0.75)
    // Using a smooth cosine curve centered on noon
    let day_factor = ((sun_angle - 0.25) * std::f32::consts::TAU).cos() * 0.5 + 0.5;
    let day_factor = day_factor.clamp(0.0, 1.0);

    // Dawn/dusk detection: peaks at angle ~0.0 and ~0.5
    let dawn_dusk = 1.0 - (2.0 * (sun_angle * 2.0 - (sun_angle * 2.0).round())).abs();
    let dawn_dusk = dawn_dusk.clamp(0.0, 1.0);
    let horizon_glow = dawn_dusk * (1.0 - (day_factor - 0.5).abs() * 2.0).max(0.0);

    // Sun color: white at noon, warm orange at dawn/dusk, dim blue at night
    let noon_sun = [1.0_f32, 0.98, 0.92];
    let dawn_sun = [1.0_f32, 0.6, 0.3];
    let night_sun = [0.2_f32, 0.25, 0.4];
    let sun_color = lerp3(
        &lerp3(&night_sun, &noon_sun, day_factor),
        &dawn_sun,
        horizon_glow * 0.7,
    );

    // Sky zenith: bright blue at noon, dark blue at night, slight pink at dawn/dusk
    let noon_zenith = [0.40_f32, 0.60, 0.90];
    let night_zenith = [0.05_f32, 0.05, 0.15];
    let dawn_zenith = [0.45_f32, 0.45, 0.75];
    let sky_zenith = lerp3(
        &lerp3(&night_zenith, &noon_zenith, day_factor),
        &dawn_zenith,
        horizon_glow * 0.5,
    );

    // Sky horizon: light blue at noon, orange/pink at dawn/dusk, dark at night
    let noon_horizon = [0.65_f32, 0.80, 0.92];
    let night_horizon = [0.08_f32, 0.08, 0.18];
    let dawn_horizon = [0.95_f32, 0.55, 0.30];
    let sky_horizon = lerp3(
        &lerp3(&night_horizon, &noon_horizon, day_factor),
        &dawn_horizon,
        horizon_glow * 0.8,
    );

    // Fog color matches horizon (what you see in the distance)
    let fog_color = sky_horizon;

    // Ambient intensity: 0.15 at night, 0.3 at day
    let ambient_intensity = 0.15 + 0.15 * day_factor;

    // Hemisphere lighting: sky-tinted ambient from above, warm ground bounce from below
    let sky_ambient = [
        sky_zenith[0] * ambient_intensity,
        sky_zenith[1] * ambient_intensity,
        sky_zenith[2] * ambient_intensity,
    ];
    let ground_base = lerp3(&[0.20, 0.15, 0.08], &[0.35, 0.30, 0.15], day_factor);
    let ground_ambient = [
        ground_base[0] * ambient_intensity,
        ground_base[1] * ambient_intensity,
        ground_base[2] * ambient_intensity,
    ];

    AtmosphereParams {
        sun_dir,
        sun_color,
        sky_zenith,
        sky_horizon,
        fog_color,
        ambient_intensity,
        sky_ambient,
        ground_ambient,
    }
}

fn lerp3(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
