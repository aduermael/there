use glam::Quat;
use crate::skeleton::NUM_BONES;

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
