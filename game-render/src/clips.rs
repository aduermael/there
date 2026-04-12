/// Procedural animation clips for the humanoid skeleton.
///
/// Each function returns an `AnimationClip` with hand-tuned keyframe rotations.
/// Conventions:
///   - Y-up, facing -Z in bind pose
///   - Rotations are local to the bone (relative to parent)
///   - X-axis rotation = pitch (forward/back swing for limbs)
///   - Z-axis rotation = lateral tilt
///   - Bind-pose rotations are `Quat::IDENTITY` for all bones

use glam::Quat;
use crate::animation::{AnimationClip, BoneTrack, BoneKeyframe};
use crate::skeleton::*;

/// Helper: create a track from a list of (time, rotation) pairs.
fn track(keyframes: &[(f32, Quat)]) -> BoneTrack {
    BoneTrack {
        keyframes: keyframes
            .iter()
            .map(|&(time, rotation)| BoneKeyframe { time, rotation })
            .collect(),
    }
}

/// Helper: identity track (bone stays in bind pose).
fn identity() -> BoneTrack {
    BoneTrack {
        keyframes: vec![BoneKeyframe { time: 0.0, rotation: Quat::IDENTITY }],
    }
}

/// Helper: constant rotation track.
fn constant(rotation: Quat) -> BoneTrack {
    BoneTrack {
        keyframes: vec![BoneKeyframe { time: 0.0, rotation }],
    }
}

/// Rotation around local X axis (pitch: positive = forward tilt / leg swings forward).
fn rx(angle: f32) -> Quat {
    Quat::from_rotation_x(angle)
}

/// Rotation around local Z axis (lateral tilt).
fn rz(angle: f32) -> Quat {
    Quat::from_rotation_z(angle)
}

/// Rotation around local Y axis (twist).
fn ry(angle: f32) -> Quat {
    Quat::from_rotation_y(angle)
}

/// Build empty tracks array (all identity).
fn empty_tracks() -> [BoneTrack; NUM_BONES] {
    std::array::from_fn(|_| identity())
}

// ── Idle ─────────────────────────────────────────────────────────────────

pub fn idle_clip() -> AnimationClip {
    let dur = 3.0;
    let mut tracks = empty_tracks();

    // Subtle spine breathing sway
    tracks[SPINE] = track(&[
        (0.0, rx(0.0)),
        (1.5, rx(0.02)),
        (3.0, rx(0.0)),
    ]);

    // Slight head bob
    tracks[HEAD] = track(&[
        (0.0, rx(0.0)),
        (1.5, rx(0.015)),
        (3.0, rx(0.0)),
    ]);

    // Arms hang naturally — tiny sway
    tracks[UPPER_ARM_L] = track(&[
        (0.0, rx(0.0)),
        (1.5, rx(0.01)),
        (3.0, rx(0.0)),
    ]);
    tracks[UPPER_ARM_R] = track(&[
        (0.0, rx(0.0)),
        (1.5, rx(-0.01)),
        (3.0, rx(0.0)),
    ]);

    // Lower arms at slight bend — matches walk midpoint so blends are smooth
    tracks[LOWER_ARM_L] = constant(rx(-0.15));
    tracks[LOWER_ARM_R] = constant(rx(-0.15));

    AnimationClip {
        name: "idle",
        duration: dur,
        looping: true,
        tracks,
    }
}

// ── Walk ─────────────────────────────────────────────────────────────────

pub fn walk_clip() -> AnimationClip {
    let dur = 1.0; // 1 second per full walk cycle
    let swing = 0.4; // leg swing amplitude (radians)
    let arm_swing = 0.3;
    let knee_bend = 0.5;

    let mut tracks = empty_tracks();

    // Hips — slight lateral sway
    tracks[HIPS] = track(&[
        (0.0, rz(0.02)),
        (0.25, rz(0.0)),
        (0.5, rz(-0.02)),
        (0.75, rz(0.0)),
        (1.0, rz(0.02)),
    ]);

    // Spine counter-rotation
    tracks[SPINE] = track(&[
        (0.0, ry(0.04)),
        (0.5, ry(-0.04)),
        (1.0, ry(0.04)),
    ]);

    // Left leg: forward at 0.0, back at 0.5
    tracks[UPPER_LEG_L] = track(&[
        (0.0, rx(swing)),
        (0.25, rx(0.0)),
        (0.5, rx(-swing)),
        (0.75, rx(0.0)),
        (1.0, rx(swing)),
    ]);
    // Left knee bends when leg passes under body
    tracks[LOWER_LEG_L] = track(&[
        (0.0, rx(0.0)),
        (0.15, rx(knee_bend)),
        (0.35, rx(0.05)),
        (0.5, rx(0.0)),
        (0.65, rx(0.1)),
        (0.85, rx(0.05)),
        (1.0, rx(0.0)),
    ]);
    // Left foot stays relatively flat
    tracks[FOOT_L] = track(&[
        (0.0, rx(-0.1)),
        (0.25, rx(0.1)),
        (0.5, rx(0.0)),
        (0.75, rx(0.0)),
        (1.0, rx(-0.1)),
    ]);

    // Right leg: opposite phase (back at 0.0, forward at 0.5)
    tracks[UPPER_LEG_R] = track(&[
        (0.0, rx(-swing)),
        (0.25, rx(0.0)),
        (0.5, rx(swing)),
        (0.75, rx(0.0)),
        (1.0, rx(-swing)),
    ]);
    tracks[LOWER_LEG_R] = track(&[
        (0.0, rx(0.0)),
        (0.15, rx(0.1)),
        (0.35, rx(0.05)),
        (0.5, rx(0.0)),
        (0.65, rx(knee_bend)),
        (0.85, rx(0.05)),
        (1.0, rx(0.0)),
    ]);
    tracks[FOOT_R] = track(&[
        (0.0, rx(0.0)),
        (0.25, rx(0.0)),
        (0.5, rx(-0.1)),
        (0.75, rx(0.1)),
        (1.0, rx(0.0)),
    ]);

    // Arms swing opposite to legs
    tracks[UPPER_ARM_L] = track(&[
        (0.0, rx(-arm_swing)),
        (0.25, rx(0.0)),
        (0.5, rx(arm_swing)),
        (0.75, rx(0.0)),
        (1.0, rx(-arm_swing)),
    ]);
    tracks[LOWER_ARM_L] = track(&[
        (0.0, rx(-0.2)),
        (0.25, rx(-0.4)),
        (0.5, rx(-0.2)),
        (0.75, rx(-0.1)),
        (1.0, rx(-0.2)),
    ]);

    tracks[UPPER_ARM_R] = track(&[
        (0.0, rx(arm_swing)),
        (0.25, rx(0.0)),
        (0.5, rx(-arm_swing)),
        (0.75, rx(0.0)),
        (1.0, rx(arm_swing)),
    ]);
    tracks[LOWER_ARM_R] = track(&[
        (0.0, rx(-0.2)),
        (0.25, rx(-0.1)),
        (0.5, rx(-0.2)),
        (0.75, rx(-0.4)),
        (1.0, rx(-0.2)),
    ]);

    AnimationClip {
        name: "walk",
        duration: dur,
        looping: true,
        tracks,
    }
}

// ── Run ──────────────────────────────────────────────────────────────────

pub fn run_clip() -> AnimationClip {
    let dur = 0.6; // faster cycle
    let swing = 0.6; // wider stride
    let arm_swing = 0.5;
    let knee_bend = 0.8;

    let mut tracks = empty_tracks();

    // Torso leans forward when running
    tracks[SPINE] = track(&[
        (0.0, rx(0.12) * ry(0.05)),
        (0.3, rx(0.12) * ry(-0.05)),
        (0.6, rx(0.12) * ry(0.05)),
    ]);

    tracks[CHEST] = constant(rx(0.05));

    // Left leg
    tracks[UPPER_LEG_L] = track(&[
        (0.0, rx(swing)),
        (0.15, rx(0.0)),
        (0.3, rx(-swing * 0.7)),
        (0.45, rx(0.0)),
        (0.6, rx(swing)),
    ]);
    tracks[LOWER_LEG_L] = track(&[
        (0.0, rx(0.1)),
        (0.1, rx(knee_bend)),
        (0.25, rx(0.1)),
        (0.3, rx(0.0)),
        (0.45, rx(0.2)),
        (0.6, rx(0.1)),
    ]);
    tracks[FOOT_L] = track(&[
        (0.0, rx(-0.15)),
        (0.15, rx(0.15)),
        (0.3, rx(0.0)),
        (0.6, rx(-0.15)),
    ]);

    // Right leg (opposite phase)
    tracks[UPPER_LEG_R] = track(&[
        (0.0, rx(-swing * 0.7)),
        (0.15, rx(0.0)),
        (0.3, rx(swing)),
        (0.45, rx(0.0)),
        (0.6, rx(-swing * 0.7)),
    ]);
    tracks[LOWER_LEG_R] = track(&[
        (0.0, rx(0.0)),
        (0.15, rx(0.2)),
        (0.3, rx(0.1)),
        (0.4, rx(knee_bend)),
        (0.55, rx(0.1)),
        (0.6, rx(0.0)),
    ]);
    tracks[FOOT_R] = track(&[
        (0.0, rx(0.0)),
        (0.3, rx(-0.15)),
        (0.45, rx(0.15)),
        (0.6, rx(0.0)),
    ]);

    // Arms pump harder
    tracks[UPPER_ARM_L] = track(&[
        (0.0, rx(-arm_swing)),
        (0.3, rx(arm_swing)),
        (0.6, rx(-arm_swing)),
    ]);
    tracks[LOWER_ARM_L] = track(&[
        (0.0, rx(-0.4)),
        (0.15, rx(-0.7)),
        (0.3, rx(-0.3)),
        (0.45, rx(-0.2)),
        (0.6, rx(-0.4)),
    ]);

    tracks[UPPER_ARM_R] = track(&[
        (0.0, rx(arm_swing)),
        (0.3, rx(-arm_swing)),
        (0.6, rx(arm_swing)),
    ]);
    tracks[LOWER_ARM_R] = track(&[
        (0.0, rx(-0.3)),
        (0.15, rx(-0.2)),
        (0.3, rx(-0.4)),
        (0.45, rx(-0.7)),
        (0.6, rx(-0.3)),
    ]);

    AnimationClip {
        name: "run",
        duration: dur,
        looping: true,
        tracks,
    }
}

// ── Jump ─────────────────────────────────────────────────────────────────

pub fn jump_clip() -> AnimationClip {
    let dur = 0.4; // short, plays once on launch
    let mut tracks = empty_tracks();

    // Crouch → spring up
    tracks[HIPS] = track(&[
        (0.0, Quat::IDENTITY),
        (0.4, Quat::IDENTITY),
    ]);

    // Arms reach up
    tracks[UPPER_ARM_L] = track(&[
        (0.0, rx(0.0)),
        (0.2, rx(0.5)),
        (0.4, rx(0.8)),
    ]);
    tracks[UPPER_ARM_R] = track(&[
        (0.0, rx(0.0)),
        (0.2, rx(0.5)),
        (0.4, rx(0.8)),
    ]);
    tracks[LOWER_ARM_L] = track(&[
        (0.0, rx(0.0)),
        (0.4, rx(-0.3)),
    ]);
    tracks[LOWER_ARM_R] = track(&[
        (0.0, rx(0.0)),
        (0.4, rx(-0.3)),
    ]);

    // Legs tuck slightly
    tracks[UPPER_LEG_L] = track(&[
        (0.0, rx(0.0)),
        (0.2, rx(0.3)),
        (0.4, rx(0.15)),
    ]);
    tracks[UPPER_LEG_R] = track(&[
        (0.0, rx(0.0)),
        (0.2, rx(0.3)),
        (0.4, rx(0.15)),
    ]);
    tracks[LOWER_LEG_L] = track(&[
        (0.0, rx(0.0)),
        (0.2, rx(0.4)),
        (0.4, rx(0.3)),
    ]);
    tracks[LOWER_LEG_R] = track(&[
        (0.0, rx(0.0)),
        (0.2, rx(0.4)),
        (0.4, rx(0.3)),
    ]);

    AnimationClip {
        name: "jump",
        duration: dur,
        looping: false,
        tracks,
    }
}

// ── Fall ─────────────────────────────────────────────────────────────────

pub fn fall_clip() -> AnimationClip {
    let dur = 0.5;
    let mut tracks = empty_tracks();

    // Arms out for balance
    tracks[UPPER_ARM_L] = track(&[
        (0.0, rx(0.4) * rz(0.3)),
        (0.5, rx(0.5) * rz(0.4)),
    ]);
    tracks[UPPER_ARM_R] = track(&[
        (0.0, rx(0.4) * rz(-0.3)),
        (0.5, rx(0.5) * rz(-0.4)),
    ]);
    tracks[LOWER_ARM_L] = constant(rx(-0.2));
    tracks[LOWER_ARM_R] = constant(rx(-0.2));

    // Legs slightly extended
    tracks[UPPER_LEG_L] = track(&[
        (0.0, rx(0.1)),
        (0.5, rx(0.15)),
    ]);
    tracks[UPPER_LEG_R] = track(&[
        (0.0, rx(0.1)),
        (0.5, rx(0.15)),
    ]);
    tracks[LOWER_LEG_L] = track(&[
        (0.0, rx(0.1)),
        (0.5, rx(0.2)),
    ]);
    tracks[LOWER_LEG_R] = track(&[
        (0.0, rx(0.1)),
        (0.5, rx(0.2)),
    ]);

    // Slight backward lean
    tracks[SPINE] = constant(rx(-0.08));

    AnimationClip {
        name: "fall",
        duration: dur,
        looping: true,
        tracks,
    }
}

// ── Swim ─────────────────────────────────────────────────────────────────

pub fn swim_clip() -> AnimationClip {
    let dur = 1.6; // breaststroke cycle
    let mut tracks = empty_tracks();

    // Body tilts forward in water
    tracks[SPINE] = constant(rx(0.3));
    tracks[CHEST] = constant(rx(0.15));

    // Arms: breaststroke-like sweep
    // Out → pull → recover
    tracks[UPPER_ARM_L] = track(&[
        (0.0, rx(0.8) * rz(0.3)),
        (0.4, rx(0.2) * rz(0.6)),
        (0.8, rx(-0.3) * rz(0.2)),
        (1.2, rx(0.1) * rz(0.1)),
        (1.6, rx(0.8) * rz(0.3)),
    ]);
    tracks[UPPER_ARM_R] = track(&[
        (0.0, rx(0.8) * rz(-0.3)),
        (0.4, rx(0.2) * rz(-0.6)),
        (0.8, rx(-0.3) * rz(-0.2)),
        (1.2, rx(0.1) * rz(-0.1)),
        (1.6, rx(0.8) * rz(-0.3)),
    ]);
    tracks[LOWER_ARM_L] = track(&[
        (0.0, rx(-0.4)),
        (0.4, rx(-0.8)),
        (0.8, rx(-0.3)),
        (1.2, rx(-0.2)),
        (1.6, rx(-0.4)),
    ]);
    tracks[LOWER_ARM_R] = track(&[
        (0.0, rx(-0.4)),
        (0.4, rx(-0.8)),
        (0.8, rx(-0.3)),
        (1.2, rx(-0.2)),
        (1.6, rx(-0.4)),
    ]);

    // Legs: flutter kick
    tracks[UPPER_LEG_L] = track(&[
        (0.0, rx(0.2)),
        (0.4, rx(-0.2)),
        (0.8, rx(0.2)),
        (1.2, rx(-0.2)),
        (1.6, rx(0.2)),
    ]);
    tracks[UPPER_LEG_R] = track(&[
        (0.0, rx(-0.2)),
        (0.4, rx(0.2)),
        (0.8, rx(-0.2)),
        (1.2, rx(0.2)),
        (1.6, rx(-0.2)),
    ]);
    tracks[LOWER_LEG_L] = track(&[
        (0.0, rx(0.3)),
        (0.4, rx(0.1)),
        (0.8, rx(0.3)),
        (1.2, rx(0.1)),
        (1.6, rx(0.3)),
    ]);
    tracks[LOWER_LEG_R] = track(&[
        (0.0, rx(0.1)),
        (0.4, rx(0.3)),
        (0.8, rx(0.1)),
        (1.2, rx(0.3)),
        (1.6, rx(0.1)),
    ]);

    AnimationClip {
        name: "swim",
        duration: dur,
        looping: true,
        tracks,
    }
}
