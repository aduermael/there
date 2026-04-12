// Player humanoid shader: instanced with skeletal skinning.
// Uniforms, lighting, fog, and shadow bindings provided by common.wgsl prefix.

// Bone matrices storage buffer (bind group 2)
@group(2) @binding(0) var<storage, read> bone_matrices: array<mat4x4<f32>>;

const NUM_BONES: u32 = 15u;

// Crash-test-dummy body-part colors indexed by bone.
// Yellow body, black joints — classic dummy look.
fn body_part_color(bone: u32) -> vec3<f32> {
    let yellow = vec3(0.95, 0.85, 0.15);
    let black  = vec3(0.15, 0.15, 0.15);
    switch bone {
        // Black joints: neck, hips, upper legs, feet
        case 3u: { return black; } // neck
        case 0u: { return black; } // hips
        case 9u, 12u: { return black; } // upper legs
        case 11u, 14u: { return black; } // feet
        default: { return yellow; }
    }
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) bone_index: u32,
    @location(3) inst_pos_yaw: vec4<f32>,
    @location(4) inst_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_id: u32) -> VertexOutput {
    // Look up bone matrix for this vertex
    let bone_offset = instance_id * NUM_BONES + in.bone_index;
    let bone_mat = bone_matrices[bone_offset];

    // Skin the vertex: bone_mat transforms from bind pose to current pose (model space)
    let skinned_pos = (bone_mat * vec4(in.position, 1.0)).xyz;
    let skinned_normal = normalize((bone_mat * vec4(in.normal, 0.0)).xyz);

    // Instance yaw rotation
    let yaw = in.inst_pos_yaw.w;
    let c = cos(yaw);
    let s = sin(yaw);
    let rotated_pos = vec3(
        skinned_pos.x * c + skinned_pos.z * s,
        skinned_pos.y,
        -skinned_pos.x * s + skinned_pos.z * c,
    );
    let rotated_normal = vec3(
        skinned_normal.x * c + skinned_normal.z * s,
        skinned_normal.y,
        -skinned_normal.x * s + skinned_normal.z * c,
    );

    let world_pos = rotated_pos + in.inst_pos_yaw.xyz;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.world_normal = rotated_normal;
    out.color = body_part_color(in.bone_index);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let shadow = sample_shadow(in.world_pos, n);
    let lit = hemisphere_lighting(n, in.color, shadow, in.world_pos);
    let color = apply_fog(in.world_pos, lit);
    return vec4(color, 1.0);
}
