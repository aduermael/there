/// Humanoid skeleton: 15 bones with bind-pose transforms and hierarchy.
///
/// Bone indices (stable — animation clips and mesh generation depend on these):
///   0 Hips (root)
///   1 Spine
///   2 Chest
///   3 Neck
///   4 Head
///   5 UpperArmL
///   6 LowerArmL
///   7 UpperArmR
///   8 LowerArmR
///   9 UpperLegL
///  10 LowerLegL
///  11 FootL
///  12 UpperLegR
///  13 LowerLegR
///  14 FootR

pub const NUM_BONES: usize = 15;

pub const HIPS: usize = 0;
pub const SPINE: usize = 1;
pub const CHEST: usize = 2;
pub const NECK: usize = 3;
pub const HEAD: usize = 4;
pub const UPPER_ARM_L: usize = 5;
pub const LOWER_ARM_L: usize = 6;
pub const UPPER_ARM_R: usize = 7;
pub const LOWER_ARM_R: usize = 8;
pub const UPPER_LEG_L: usize = 9;
pub const LOWER_LEG_L: usize = 10;
pub const FOOT_L: usize = 11;
pub const UPPER_LEG_R: usize = 12;
pub const LOWER_LEG_R: usize = 13;
pub const FOOT_R: usize = 14;

/// Static bone definition: parent and bind-pose local transform.
struct BoneDef {
    parent: Option<usize>,
    local_translation: glam::Vec3,
    local_rotation: glam::Quat,
}

/// Build the default humanoid skeleton definition.
///
/// All translations are in bind pose (T-pose), Y-up, facing -Z.
/// Heights are measured from feet at y=0:
///   feet 0.0, knees 0.45, hips 0.9, spine 1.05, chest 1.2,
///   neck 1.4, head 1.55, shoulders ±0.22, elbows down 0.28, wrists down 0.28.
fn bone_defs() -> [BoneDef; NUM_BONES] {
    use glam::{Quat, Vec3};
    [
        // 0 Hips — root, at hip height
        BoneDef { parent: None,             local_translation: Vec3::new(0.0, 0.9, 0.0),   local_rotation: Quat::IDENTITY },
        // 1 Spine
        BoneDef { parent: Some(HIPS),       local_translation: Vec3::new(0.0, 0.15, 0.0),  local_rotation: Quat::IDENTITY },
        // 2 Chest
        BoneDef { parent: Some(SPINE),      local_translation: Vec3::new(0.0, 0.15, 0.0),  local_rotation: Quat::IDENTITY },
        // 3 Neck
        BoneDef { parent: Some(CHEST),      local_translation: Vec3::new(0.0, 0.20, 0.0),  local_rotation: Quat::IDENTITY },
        // 4 Head
        BoneDef { parent: Some(NECK),       local_translation: Vec3::new(0.0, 0.15, 0.0),  local_rotation: Quat::IDENTITY },
        // 5 UpperArmL — offset left from chest
        BoneDef { parent: Some(CHEST),      local_translation: Vec3::new(-0.22, 0.15, 0.0), local_rotation: Quat::IDENTITY },
        // 6 LowerArmL
        BoneDef { parent: Some(UPPER_ARM_L), local_translation: Vec3::new(0.0, -0.28, 0.0), local_rotation: Quat::IDENTITY },
        // 7 UpperArmR — offset right from chest
        BoneDef { parent: Some(CHEST),      local_translation: Vec3::new(0.22, 0.15, 0.0),  local_rotation: Quat::IDENTITY },
        // 8 LowerArmR
        BoneDef { parent: Some(UPPER_ARM_R), local_translation: Vec3::new(0.0, -0.28, 0.0), local_rotation: Quat::IDENTITY },
        // 9 UpperLegL — offset left from hips
        BoneDef { parent: Some(HIPS),       local_translation: Vec3::new(-0.10, 0.0, 0.0),  local_rotation: Quat::IDENTITY },
        // 10 LowerLegL
        BoneDef { parent: Some(UPPER_LEG_L), local_translation: Vec3::new(0.0, -0.45, 0.0), local_rotation: Quat::IDENTITY },
        // 11 FootL
        BoneDef { parent: Some(LOWER_LEG_L), local_translation: Vec3::new(0.0, -0.45, 0.0), local_rotation: Quat::IDENTITY },
        // 12 UpperLegR — offset right from hips
        BoneDef { parent: Some(HIPS),       local_translation: Vec3::new(0.10, 0.0, 0.0),   local_rotation: Quat::IDENTITY },
        // 13 LowerLegR
        BoneDef { parent: Some(UPPER_LEG_R), local_translation: Vec3::new(0.0, -0.45, 0.0), local_rotation: Quat::IDENTITY },
        // 14 FootR
        BoneDef { parent: Some(LOWER_LEG_R), local_translation: Vec3::new(0.0, -0.45, 0.0), local_rotation: Quat::IDENTITY },
    ]
}

/// Runtime skeleton: bind-pose world matrices and inverse bind matrices.
pub struct Skeleton {
    /// Parent index per bone (None for root).
    parents: [Option<usize>; NUM_BONES],
    /// Bind-pose local translation per bone.
    local_translations: [glam::Vec3; NUM_BONES],
    /// Bind-pose local rotation per bone.
    local_rotations: [glam::Quat; NUM_BONES],
    /// Inverse bind-pose world matrices (for skinning: vertex → bone-local space).
    inv_bind: [glam::Mat4; NUM_BONES],
}

impl Skeleton {
    pub fn new() -> Self {
        let defs = bone_defs();
        let mut parents = [None; NUM_BONES];
        let mut local_translations = [glam::Vec3::ZERO; NUM_BONES];
        let mut local_rotations = [glam::Quat::IDENTITY; NUM_BONES];
        let mut bind_world = [glam::Mat4::IDENTITY; NUM_BONES];

        for (i, def) in defs.iter().enumerate() {
            parents[i] = def.parent;
            local_translations[i] = def.local_translation;
            local_rotations[i] = def.local_rotation;

            let local = glam::Mat4::from_rotation_translation(def.local_rotation, def.local_translation);
            bind_world[i] = match def.parent {
                Some(p) => bind_world[p] * local,
                None => local,
            };
        }

        let mut inv_bind = [glam::Mat4::IDENTITY; NUM_BONES];
        for i in 0..NUM_BONES {
            inv_bind[i] = bind_world[i].inverse();
        }

        Self { parents, local_translations, local_rotations, inv_bind }
    }

    /// Compute world-space bone matrices from per-bone local rotations (pose).
    ///
    /// `pose_rotations` overrides the bind-pose local rotation for each bone.
    /// Returns `[Mat4; NUM_BONES]` ready for GPU upload (world × inv_bind).
    pub fn compute_skin_matrices(&self, pose_rotations: &[glam::Quat; NUM_BONES]) -> [glam::Mat4; NUM_BONES] {
        let mut world = [glam::Mat4::IDENTITY; NUM_BONES];
        let mut skin = [glam::Mat4::IDENTITY; NUM_BONES];

        for i in 0..NUM_BONES {
            let local = glam::Mat4::from_rotation_translation(
                pose_rotations[i],
                self.local_translations[i],
            );
            world[i] = match self.parents[i] {
                Some(p) => world[p] * local,
                None => local,
            };
            skin[i] = world[i] * self.inv_bind[i];
        }

        skin
    }

    /// Bind-pose skin matrices (all identity — mesh already in bind pose).
    pub fn bind_pose_matrices(&self) -> [glam::Mat4; NUM_BONES] {
        self.compute_skin_matrices(&self.local_rotations)
    }

    /// Bind-pose world position of a bone (useful for mesh generation).
    pub fn bone_world_pos(&self, bone: usize) -> glam::Vec3 {
        let defs = bone_defs();
        let mut world = [glam::Mat4::IDENTITY; NUM_BONES];
        for i in 0..=bone {
            let local = glam::Mat4::from_rotation_translation(
                defs[i].local_rotation,
                defs[i].local_translation,
            );
            world[i] = match defs[i].parent {
                Some(p) => world[p] * local,
                None => local,
            };
        }
        world[bone].transform_point3(glam::Vec3::ZERO)
    }
}
