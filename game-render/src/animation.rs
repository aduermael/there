use glam::Quat;
use crate::skeleton::NUM_BONES;
use crate::clips;

/// A single keyframe for one bone: a rotation at a specific time.
#[derive(Clone)]
pub struct BoneKeyframe {
    pub time: f32,
    pub rotation: Quat,
}

/// Per-bone track of keyframes (sorted by time).
#[derive(Clone)]
pub struct BoneTrack {
    pub keyframes: Vec<BoneKeyframe>,
}

impl BoneTrack {
    /// Sample this track at `t`, interpolating between keyframes.
    /// Returns `Quat::IDENTITY` if empty.
    pub fn sample(&self, t: f32) -> Quat {
        let kf = &self.keyframes;
        if kf.is_empty() {
            return Quat::IDENTITY;
        }
        if kf.len() == 1 || t <= kf[0].time {
            return kf[0].rotation;
        }
        if t >= kf.last().unwrap().time {
            return kf.last().unwrap().rotation;
        }
        // Find the two keyframes surrounding `t`
        for i in 0..kf.len() - 1 {
            if t >= kf[i].time && t < kf[i + 1].time {
                let span = kf[i + 1].time - kf[i].time;
                let frac = (t - kf[i].time) / span;
                return kf[i].rotation.slerp(kf[i + 1].rotation, frac);
            }
        }
        kf.last().unwrap().rotation
    }
}

/// A complete animation clip with per-bone tracks.
#[derive(Clone)]
pub struct AnimationClip {
    pub name: &'static str,
    pub duration: f32,
    pub looping: bool,
    /// One track per bone. Bones without animation have an empty track.
    pub tracks: [BoneTrack; NUM_BONES],
}

/// Sample all bone rotations from a clip at time `t`.
/// For looping clips, `t` is wrapped to `[0, duration)`.
pub fn sample_clip(clip: &AnimationClip, t: f32) -> [Quat; NUM_BONES] {
    let t = if clip.looping && clip.duration > 0.0 {
        t.rem_euclid(clip.duration)
    } else {
        t.clamp(0.0, clip.duration)
    };
    let mut rotations = [Quat::IDENTITY; NUM_BONES];
    for (i, track) in clip.tracks.iter().enumerate() {
        rotations[i] = track.sample(t);
    }
    rotations
}

/// Blend two sets of bone rotations by factor `t` (0.0 = a, 1.0 = b).
pub fn blend_poses(a: &[Quat; NUM_BONES], b: &[Quat; NUM_BONES], t: f32) -> [Quat; NUM_BONES] {
    let mut result = [Quat::IDENTITY; NUM_BONES];
    for i in 0..NUM_BONES {
        result[i] = a[i].slerp(b[i], t);
    }
    result
}

// ── Animation state machine ─────────────────────────────────────────────

/// Movement states that drive animation selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimState {
    Idle,
    Walk,
    Run,
    Jump,
    Fall,
    Swim,
}

impl AnimState {
    /// Derive animation state from movement parameters.
    ///   - `speed`: horizontal speed (length of XZ velocity)
    ///   - `vertical_velocity`: Y velocity (positive = going up)
    ///   - `y`: player world Y position
    ///   - `water_level`: world water height
    pub fn from_movement(speed: f32, vertical_velocity: f32, y: f32, water_level: f32) -> Self {
        if y < water_level - 0.3 {
            return AnimState::Swim;
        }
        if vertical_velocity > 1.0 {
            return AnimState::Jump;
        }
        if vertical_velocity < -1.0 {
            return AnimState::Fall;
        }
        if speed > 4.0 {
            return AnimState::Run;
        }
        if speed > 0.3 {
            return AnimState::Walk;
        }
        AnimState::Idle
    }

    /// Convert to u8 for network serialization.
    pub fn to_u8(self) -> u8 {
        match self {
            AnimState::Idle => 0,
            AnimState::Walk => 1,
            AnimState::Run => 2,
            AnimState::Jump => 3,
            AnimState::Fall => 4,
            AnimState::Swim => 5,
        }
    }

    /// Convert from u8 (network deserialization).
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => AnimState::Walk,
            2 => AnimState::Run,
            3 => AnimState::Jump,
            4 => AnimState::Fall,
            5 => AnimState::Swim,
            _ => AnimState::Idle,
        }
    }
}

/// Crossfade duration in seconds.
const BLEND_DURATION: f32 = 0.2;

/// Per-player animation playback controller.
pub struct AnimationPlayer {
    /// All clips, indexed by AnimState.
    clips: Vec<AnimationClip>,
    /// Current active state.
    current_state: AnimState,
    /// Elapsed time in current clip.
    current_time: f32,
    /// Previous state (for crossfade blending).
    prev_state: Option<AnimState>,
    /// Elapsed time in previous clip when transition started.
    prev_time: f32,
    /// Blend progress (0.0 → 1.0 over BLEND_DURATION).
    blend_elapsed: f32,
}

impl AnimationPlayer {
    pub fn new() -> Self {
        // Pre-build all clips. Order must match AnimState discriminants.
        let clips = vec![
            clips::idle_clip(), // 0 = Idle
            clips::walk_clip(), // 1 = Walk
            clips::run_clip(),  // 2 = Run
            clips::jump_clip(), // 3 = Jump
            clips::fall_clip(), // 4 = Fall
            clips::swim_clip(), // 5 = Swim
        ];
        Self {
            clips,
            current_state: AnimState::Idle,
            current_time: 0.0,
            prev_state: None,
            prev_time: 0.0,
            blend_elapsed: 0.0,
        }
    }

    /// Set the desired animation state. Triggers crossfade if state changes.
    pub fn set_state(&mut self, state: AnimState) {
        if state != self.current_state {
            self.prev_state = Some(self.current_state);
            self.prev_time = self.current_time;
            self.blend_elapsed = 0.0;
            self.current_state = state;
            self.current_time = 0.0;
        }
    }

    /// Advance playback by `dt` seconds and return the blended bone rotations.
    pub fn update(&mut self, dt: f32) -> [Quat; NUM_BONES] {
        self.current_time += dt;

        let idx = self.current_state.to_u8() as usize;
        let current_pose = sample_clip(&self.clips[idx], self.current_time);

        // Crossfade blending
        if let Some(prev) = self.prev_state {
            self.blend_elapsed += dt;
            let blend_t = (self.blend_elapsed / BLEND_DURATION).min(1.0);

            if blend_t >= 1.0 {
                // Blend complete
                self.prev_state = None;
                current_pose
            } else {
                self.prev_time += dt;
                let prev_idx = prev.to_u8() as usize;
                let prev_pose = sample_clip(&self.clips[prev_idx], self.prev_time);
                blend_poses(&prev_pose, &current_pose, blend_t)
            }
        } else {
            current_pose
        }
    }

    pub fn current_state(&self) -> AnimState {
        self.current_state
    }
}
