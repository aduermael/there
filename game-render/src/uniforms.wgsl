// Single source of truth for the Uniforms struct.
// Prepended to ALL shaders (geometry, sky, ssao, postprocess, grass_compute).

struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_dir: vec3<f32>,
    fog_color: vec3<f32>,
    fog_density: f32,
    world_size: f32,
    hm_res: f32,
    fog_height_falloff: f32,
    time: f32,
    sun_color: vec3<f32>,
    sky_zenith: vec3<f32>,
    sky_horizon: vec3<f32>,
    inv_view_proj: mat4x4<f32>,
    sky_ambient: vec3<f32>,
    ground_ambient: vec3<f32>,
    // Active shadow VP — overwritten per cascade during shadow passes
    sun_view_proj: mat4x4<f32>,
    // Per-cascade view-projection matrices (for scene shader sampling)
    cascade_vp0: mat4x4<f32>,
    cascade_vp1: mat4x4<f32>,
    cascade_vp2: mat4x4<f32>,
    // Cascade split distances (x=near, y=mid, z=far, w=unused)
    cascade_splits: vec4<f32>,
};
