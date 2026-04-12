use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, parser::ValueSource};
use serde::Deserialize;

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

    /// Render 8 orbit views in a grid (implies --orbit --show-player)
    #[arg(long, default_value_t = false)]
    turntable: bool,

    /// Number of columns in turntable grid (default 4)
    #[arg(long, default_value_t = 4)]
    turntable_cols: u32,

    /// Load scenario from JSON file. CLI flags override JSON values.
    #[arg(long)]
    scenario: Option<String>,
}

/// JSON scenario config. All fields optional — defaults come from CLI.
#[derive(Deserialize, Default)]
struct ScenarioConfig {
    width: Option<u32>,
    height: Option<u32>,
    sun_angle: Option<f32>,
    output: Option<String>,
    show_player: Option<bool>,
    player_pos: Option<[f32; 3]>,
    player_yaw: Option<f32>,
    orbit: Option<bool>,
    orbit_yaw: Option<f32>,
    orbit_pitch: Option<f32>,
    orbit_distance: Option<f32>,
    turntable: Option<bool>,
    turntable_cols: Option<u32>,
    #[serde(default)]
    steps: Vec<Step>,
}

/// A single step in a movement simulation scenario.
#[derive(Deserialize)]
#[serde(untagged)]
enum Step {
    Input {
        input: InputState,
        duration_secs: f32,
    },
    Snapshot {
        snapshot: String,
    },
}

/// Input state for a simulation step. All fields default to 0.0.
#[derive(Deserialize, Default, Clone, Copy)]
struct InputState {
    #[serde(default)]
    forward: f32,
    #[serde(default)]
    strafe: f32,
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

/// Returns true if the user explicitly passed this flag on the command line.
fn was_explicit(matches: &ArgMatches, name: &str) -> bool {
    matches.value_source(name) == Some(ValueSource::CommandLine)
}

/// Pick: explicit CLI value > JSON value > CLI default.
macro_rules! merge {
    ($matches:expr, $args:expr, $scenario:expr, $field:ident) => {
        if was_explicit($matches, stringify!($field)) {
            $args.$field
        } else {
            $scenario.$field.unwrap_or($args.$field)
        }
    };
}

/// Resolved configuration after merging CLI args and scenario JSON.
struct Config {
    width: u32,
    height: u32,
    camera_pos: glam::Vec3,
    camera_target: glam::Vec3,
    sun_angle: f32,
    output: String,
    show_player: bool,
    player_pos: Option<glam::Vec3>,
    player_yaw: Option<f32>,
    orbit: bool,
    orbit_yaw: f32,
    orbit_pitch: f32,
    orbit_distance: f32,
    turntable: bool,
    turntable_cols: u32,
    steps: Vec<Step>,
}

fn build_config(matches: &ArgMatches, args: Args, scenario: ScenarioConfig) -> Config {
    let player_pos_from_scenario = scenario.player_pos.map(|p| glam::Vec3::new(p[0], p[1], p[2]));
    let player_pos = if was_explicit(matches, "player_pos") {
        args.player_pos
    } else {
        player_pos_from_scenario.or(args.player_pos)
    };

    let player_yaw = if was_explicit(matches, "player_yaw") {
        args.player_yaw
    } else {
        scenario.player_yaw.or(args.player_yaw)
    };

    let output = if was_explicit(matches, "output") {
        args.output
    } else {
        scenario.output.unwrap_or(args.output)
    };

    Config {
        width: merge!(matches, args, scenario, width),
        height: merge!(matches, args, scenario, height),
        camera_pos: args.camera_pos,
        camera_target: args.camera_target,
        sun_angle: merge!(matches, args, scenario, sun_angle),
        output,
        show_player: merge!(matches, args, scenario, show_player),
        player_pos,
        player_yaw,
        orbit: merge!(matches, args, scenario, orbit),
        orbit_yaw: merge!(matches, args, scenario, orbit_yaw),
        orbit_pitch: merge!(matches, args, scenario, orbit_pitch),
        orbit_distance: merge!(matches, args, scenario, orbit_distance),
        turntable: merge!(matches, args, scenario, turntable),
        turntable_cols: merge!(matches, args, scenario, turntable_cols),
        steps: scenario.steps,
    }
}

fn main() {
    env_logger::init();

    let matches = Args::command().get_matches();
    let args = Args::from_arg_matches(&matches).unwrap();

    // Load scenario JSON if provided
    let scenario = if let Some(path) = &args.scenario {
        let json = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read scenario file '{}': {}", path, e));
        serde_json::from_str::<ScenarioConfig>(&json)
            .unwrap_or_else(|e| panic!("Failed to parse scenario file '{}': {}", path, e))
    } else {
        ScenarioConfig::default()
    };

    let cfg = build_config(&matches, args, scenario);

    log::info!(
        "Rendering {}x{} frame, sun_angle={}, output={}",
        cfg.width,
        cfg.height,
        cfg.sun_angle,
        cfg.output,
    );

    // Simulation mode: step-based movement + snapshot sequence
    if !cfg.steps.is_empty() {
        let player_ground = cfg.player_pos.unwrap_or(cfg.camera_target);
        let sim = pollster::block_on(render::SimRenderer::new(
            cfg.width,
            cfg.height,
            cfg.sun_angle,
            player_ground,
            cfg.orbit_yaw,
            cfg.orbit_pitch,
            cfg.orbit_distance,
        ));

        let heightmap = sim.heightmap();
        let ground_y = game_core::terrain::sample_height(heightmap, player_ground.x, player_ground.z);
        let mut player_pos = glam::Vec3::new(player_ground.x, ground_y, player_ground.z);
        let mut player_yaw = cfg.player_yaw.unwrap_or(0.0);
        let mut orbit_yaw = cfg.orbit_yaw;

        for step in &cfg.steps {
            match step {
                Step::Input { input, duration_secs } => {
                    let ticks = (*duration_secs / game_core::TICK_INTERVAL_SECS) as u32;
                    let is_moving = input.forward != 0.0 || input.strafe != 0.0;
                    for _ in 0..ticks {
                        // Match client frame order: movement → move_yaw → camera follow
                        player_pos = game_core::movement::apply_movement(
                            player_pos,
                            input.forward,
                            input.strafe,
                            orbit_yaw,
                            game_core::TICK_INTERVAL_SECS,
                            heightmap,
                        );
                        if is_moving {
                            player_yaw = game_core::movement::move_yaw(
                                input.forward, input.strafe, orbit_yaw,
                            );
                            orbit_yaw = game_core::movement::camera_follow_yaw(
                                orbit_yaw, player_yaw, game_core::TICK_INTERVAL_SECS,
                            );
                        }
                    }
                    sim.update_player(player_pos, player_yaw);
                }
                Step::Snapshot { snapshot } => {
                    sim.update_player(player_pos, player_yaw);
                    sim.snapshot(player_pos, orbit_yaw, snapshot);
                    println!("{}", snapshot);
                }
            }
        }
        return;
    }

    let pixels = if cfg.turntable {
        let player_ground = cfg.player_pos.unwrap_or(cfg.camera_target);
        let heightmap_data = game_core::terrain::generate_heightmap();
        let ground_y =
            game_core::terrain::sample_height(&heightmap_data, player_ground.x, player_ground.z);
        let player_pos = glam::Vec3::new(player_ground.x, ground_y, player_ground.z);

        pollster::block_on(render::render_turntable(
            cfg.width,
            cfg.height,
            player_pos,
            cfg.orbit_pitch,
            cfg.orbit_distance,
            cfg.sun_angle,
            cfg.turntable_cols,
        ))
    } else {
        let show_player = cfg.show_player || cfg.orbit;

        let player_opts = if show_player {
            Some(render::PlayerOpts {
                pos: cfg.player_pos,
                yaw: cfg.player_yaw,
            })
        } else {
            None
        };

        let (camera_pos, camera_target, player_opts) = if cfg.orbit {
            let player_ground = cfg.player_pos.unwrap_or(cfg.camera_target);
            let heightmap_data = game_core::terrain::generate_heightmap();
            let ground_y = game_core::terrain::sample_height(
                &heightmap_data,
                player_ground.x,
                player_ground.z,
            );
            let player_pos = glam::Vec3::new(player_ground.x, ground_y, player_ground.z);
            let (eye, target) = game_core::camera::orbit_eye(
                player_pos,
                cfg.orbit_yaw,
                cfg.orbit_pitch,
                cfg.orbit_distance,
            );
            let opts = Some(render::PlayerOpts {
                pos: Some(player_pos),
                yaw: cfg.player_yaw,
            });
            (eye, target, opts)
        } else {
            (cfg.camera_pos, cfg.camera_target, player_opts)
        };

        pollster::block_on(render::render_frame(
            cfg.width,
            cfg.height,
            camera_pos,
            camera_target,
            cfg.sun_angle,
            player_opts,
        ))
    };

    image::save_buffer(
        &cfg.output,
        &pixels,
        cfg.width,
        cfg.height,
        image::ColorType::Rgba8,
    )
    .expect("Failed to save PNG");

    log::info!("Saved {}", cfg.output);
    println!("{}", cfg.output);
}
