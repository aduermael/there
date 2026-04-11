/// All atmosphere parameters derived from a single `sun_angle`.
/// sun_angle: 0.0 = dawn, 0.25 = noon, 0.5 = dusk, 0.75 = night, 1.0 = dawn again.
pub struct AtmosphereParams {
    pub sun_dir: glam::Vec3,
    pub sun_color: [f32; 3],
    pub sky_zenith: [f32; 3],
    pub sky_horizon: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    pub fog_height_falloff: f32,
    pub sky_ambient: [f32; 3],
    pub ground_ambient: [f32; 3],
}

pub fn compute_atmosphere(sun_angle: f32) -> AtmosphereParams {
    let theta = sun_angle * std::f32::consts::TAU;

    // Time-of-day factor: 1.0 at noon (angle=0.25), 0.0 at midnight (angle=0.75)
    let day_factor = ((sun_angle - 0.25) * std::f32::consts::TAU).cos() * 0.5 + 0.5;
    let day_factor = day_factor.clamp(0.0, 1.0);

    // Sun direction: orbits east-west, elevation = sin(theta)
    // At night, "moon" is elevated higher in the sky for better illumination
    let elevation = theta.sin();
    let east_west = theta.cos();
    let moon_lift = (1.0 - day_factor) * 0.25; // 0 at noon, 0.25 at midnight
    let min_y = 0.01 + moon_lift;
    let sun_dir = glam::Vec3::new(east_west, elevation.max(min_y), 0.3).normalize();

    // Dawn/dusk detection: peaks at angle ~0.0 and ~0.5
    let dawn_dusk = 1.0 - (2.0 * (sun_angle * 2.0 - (sun_angle * 2.0).round())).abs();
    let dawn_dusk = dawn_dusk.clamp(0.0, 1.0);
    let horizon_glow = dawn_dusk * (1.0 - (day_factor - 0.5).abs() * 2.0).max(0.0);

    // Separate dawn vs dusk: 0.0 at dawn (angle ~0.0), 1.0 at dusk (angle ~0.5)
    let dusk_blend = smoothstep(0.15, 0.38, sun_angle) * (1.0 - smoothstep(0.62, 0.85, sun_angle));

    // Sun color: bright warm-white at noon, peach-gold at dawn, deep amber at dusk
    let noon_sun = [1.20_f32, 1.10, 0.92];
    let dawn_sun = [1.25_f32, 0.62, 0.28];
    let dusk_sun = [1.15_f32, 0.42, 0.18];
    let night_sun = [0.18_f32, 0.22, 0.38];
    let glow_sun = lerp3(&dawn_sun, &dusk_sun, dusk_blend);
    let sun_color = lerp3(
        &lerp3(&night_sun, &noon_sun, day_factor),
        &glow_sun,
        horizon_glow * 0.75,
    );

    // Sky zenith: vivid blue at noon, cool lavender at dawn, warm purple at dusk
    let noon_zenith = [0.22_f32, 0.45, 0.95];
    let night_zenith = [0.04_f32, 0.06, 0.18];
    let dawn_zenith = [0.35_f32, 0.38, 0.82];
    let dusk_zenith = [0.28_f32, 0.16, 0.55];
    let glow_zenith = lerp3(&dawn_zenith, &dusk_zenith, dusk_blend);
    let sky_zenith = lerp3(
        &lerp3(&night_zenith, &noon_zenith, day_factor),
        &glow_zenith,
        horizon_glow * 0.55,
    );

    // Sky horizon: clear blue at noon, peach at dawn, deep amber-rose at dusk
    let noon_horizon = [0.42_f32, 0.60, 0.88];
    let night_horizon = [0.03_f32, 0.04, 0.12];
    let dawn_horizon = [1.0_f32, 0.58, 0.32];
    let dusk_horizon = [1.0_f32, 0.35, 0.12];
    let glow_horizon = lerp3(&dawn_horizon, &dusk_horizon, dusk_blend);
    let sky_horizon = lerp3(
        &lerp3(&night_horizon, &noon_horizon, day_factor),
        &glow_horizon,
        horizon_glow * 0.85,
    );

    // Fog color: horizon-tinted but desaturated at dawn/dusk to avoid pink wash
    let avg_h = (sky_horizon[0] + sky_horizon[1] + sky_horizon[2]) / 3.0;
    let fog_neutral = [avg_h, avg_h, avg_h * 1.05]; // slightly cool neutral
    let fog_color = lerp3(&sky_horizon, &fog_neutral, horizon_glow * 0.45);

    // Ambient intensity: lower = more sun/shadow contrast, higher = flatter
    let ambient_intensity = 0.13 + 0.09 * day_factor;

    // Hemisphere lighting: sky-tinted ambient from above, warm earth bounce from below
    let sky_ambient = [
        sky_zenith[0] * ambient_intensity,
        sky_zenith[1] * ambient_intensity,
        sky_zenith[2] * ambient_intensity,
    ];
    let night_ground = [0.06_f32, 0.07, 0.12]; // cold blue-gray moonlit earth
    let noon_ground = [0.48_f32, 0.38, 0.18]; // strong warm earth bounce for shadow warmth
    let ground_base = lerp3(&night_ground, &noon_ground, day_factor);
    let ground_ambient = [
        ground_base[0] * ambient_intensity,
        ground_base[1] * ambient_intensity,
        ground_base[2] * ambient_intensity,
    ];

    // Exponential height fog: clear scenes, minimal wash, atmospheric at distance
    let fog_density = 0.0012 + 0.004 * horizon_glow + 0.0005 * (1.0 - day_factor);
    let fog_height_falloff = 0.05 - 0.015 * horizon_glow;

    AtmosphereParams {
        sun_dir,
        sun_color,
        sky_zenith,
        sky_horizon,
        fog_color,
        fog_density,
        fog_height_falloff,
        sky_ambient,
        ground_ambient,
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp3(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
