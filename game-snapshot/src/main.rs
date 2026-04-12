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

    let player_opts = if args.show_player {
        Some(render::PlayerOpts {
            pos: args.player_pos,
            yaw: args.player_yaw,
        })
    } else {
        None
    };

    let pixels = pollster::block_on(render::render_frame(
        args.width,
        args.height,
        args.camera_pos,
        args.camera_target,
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
