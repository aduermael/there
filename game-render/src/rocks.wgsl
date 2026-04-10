struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_dir: vec3<f32>,
    fog_color: vec3<f32>,
    fog_far: f32,
    world_size: f32,
    hm_res: f32,
    ambient_intensity: f32,
    time: f32,
    sun_color: vec3<f32>,
    sky_zenith: vec3<f32>,
    sky_horizon: vec3<f32>,
    inv_view_proj: mat4x4<f32>,
    sky_ambient: vec3<f32>,
    ground_ambient: vec3<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) inst_pos_scale: vec4<f32>,
    @location(2) inst_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let scale = in.inst_pos_scale.w;
    let world_pos = in.position * scale + in.inst_pos_scale.xyz;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = in.inst_color.rgb;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Flat shading via screen-space derivatives
    let dx = dpdx(in.world_pos);
    let dy = dpdy(in.world_pos);
    let n = normalize(cross(dx, dy));

    // Hemisphere ambient
    let hemi_t = dot(n, vec3(0.0, 1.0, 0.0)) * 0.5 + 0.5;
    let ambient = mix(u.ground_ambient, u.sky_ambient, hemi_t);

    let ndl = max(dot(n, u.sun_dir), 0.0);
    let lit = in.color * (ambient + ndl * u.sun_color);

    // Rim/fresnel lighting for silhouette definition
    let view_dir = normalize(u.camera_pos - in.world_pos);
    let fresnel = pow(1.0 - max(dot(n, view_dir), 0.0), 3.0);
    let rim = fresnel * u.sky_ambient * 0.8;

    // Distance fog
    let dist = length(in.world_pos - u.camera_pos);
    let fog = clamp(dist / u.fog_far, 0.0, 1.0);
    let color = mix(lit + rim, u.fog_color, fog);

    return vec4(color, 1.0);
}
