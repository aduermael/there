use clap::Parser;

mod render;

#[derive(Parser)]
#[command(about = "Render a single frame of the game scene to a PNG")]
struct Args {
    /// Output image width
    #[arg(long, default_value_t = 1920)]
    width: u32,

    /// Output image height
    #[arg(long, default_value_t = 1080)]
    height: u32,

    /// Camera eye position (x,y,z)
    #[arg(long, default_value = "160,25,160", value_parser = parse_vec3)]
    camera_pos: glam::Vec3,

    /// Camera look-at target (x,y,z)
    #[arg(long, default_value = "128,15,128", value_parser = parse_vec3)]
    camera_target: glam::Vec3,

    /// Time of day: 0.0=dawn, 0.25=noon, 0.5=dusk, 0.75=night
    #[arg(long, default_value_t = 0.5)]
    sun_angle: f32,

    /// Output PNG file path
    #[arg(long, default_value = "frame.png")]
    output: String,

    /// Show a player avatar in the scene
    #[arg(long, default_value_t = false)]
    show_player: bool,

    /// Player position (x,y,z). Y=-1 means auto from heightmap. Defaults to camera_target.
    #[arg(long, value_parser = parse_vec3)]
    player_pos: Option<glam::Vec3>,

    /// Player facing yaw in radians (0 = -Z). Default: face toward camera.
    #[arg(long)]
    player_yaw: Option<f32>,

    /// Use third-person orbit camera (implies --show-player). Derives camera from player position.
    #[arg(long, default_value_t = false)]
    orbit: bool,

    /// Orbit camera yaw in radians (default 0.0)
    #[arg(long, default_value_t = 0.0)]
    orbit_yaw: f32,

    /// Orbit camera pitch in radians (default 0.4)
    #[arg(long, default_value_t = 0.4)]
    orbit_pitch: f32,

    /// Orbit camera distance (default 8.0)
    #[arg(long, default_value_t = 8.0)]
    orbit_distance: f32,
}

fn parse_vec3(s: &str) -> Result<glam::Vec3, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err("expected x,y,z".into());
    }
    let x: f32 = parts[0].trim().parse().map_err(|e| format!("{e}"))?;
    let y: f32 = parts[1].trim().parse().map_err(|e| format!("{e}"))?;
    let z: f32 = parts[2].trim().parse().map_err(|e| format!("{e}"))?;
    Ok(glam::Vec3::new(x, y, z))
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    log::info!(
        "Rendering {}x{} frame, sun_angle={}, output={}",
        args.width,
        args.height,
        args.sun_angle,
        args.output,
    );

    // --orbit implies --show-player
    let show_player = args.show_player || args.orbit;

    let player_opts = if show_player {
        Some(render::PlayerOpts {
            pos: args.player_pos,
            yaw: args.player_yaw,
        })
    } else {
        None
    };

    // In orbit mode, compute camera from orbit function using player position
    let (camera_pos, camera_target, player_opts) = if args.orbit {
        // Player XZ: use --player-pos if given, else default camera_target
        let player_ground = args.player_pos.unwrap_or(args.camera_target);
        let heightmap_data = game_core::terrain::generate_heightmap();
        let ground_y = game_core::terrain::sample_height(&heightmap_data, player_ground.x, player_ground.z);
        let player_pos = glam::Vec3::new(player_ground.x, ground_y, player_ground.z);
        let (eye, target) = game_core::camera::orbit_eye(player_pos, args.orbit_yaw, args.orbit_pitch, args.orbit_distance);
        // Pass resolved position to player renderer so avatar sits on terrain
        let opts = Some(render::PlayerOpts {
            pos: Some(player_pos),
            yaw: args.player_yaw,
        });
        (eye, target, opts)
    } else {
        (args.camera_pos, args.camera_target, player_opts)
    };

    let pixels = pollster::block_on(render::render_frame(
        args.width,
        args.height,
        camera_pos,
        camera_target,
        args.sun_angle,
        player_opts,
    ));

    image::save_buffer(
        &args.output,
        &pixels,
        args.width,
        args.height,
        image::ColorType::Rgba8,
    )
    .expect("Failed to save PNG");

    log::info!("Saved {}", args.output);
    println!("{}", args.output);
}
