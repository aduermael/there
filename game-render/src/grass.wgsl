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
    @location(1) bend: f32,
    @location(2) inst_pos_scale: vec4<f32>,
    @location(3) inst_color_rotation: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let scale = in.inst_pos_scale.w;
    let angle = in.inst_color_rotation.w;
    let base_pos = in.inst_pos_scale.xyz;

    // Rotate blade around Y axis
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let rotated = vec3(
        in.position.x * cos_a - in.position.z * sin_a,
        in.position.y,
        in.position.x * sin_a + in.position.z * cos_a,
    );

    var local = rotated * scale;

    // Wind animation: displace tip vertex based on time + position hash
    // Wind blows from the west (positive X direction)
    let wind_phase = base_pos.x * 0.15 + base_pos.z * 0.1;
    let wind_base = sin(u.time * 1.8 + wind_phase) * 0.15;
    let wind_detail = sin(u.time * 3.7 + wind_phase * 2.3) * 0.05;
    let wind = (wind_base + wind_detail) * in.bend;
    local.x += wind;
    local.z += wind * 0.3;

    let world_pos = local + base_pos;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = in.inst_color_rotation.rgb;
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

    // Distance fog
    let dist = length(in.world_pos - u.camera_pos);
    let fog = clamp(dist / u.fog_far, 0.0, 1.0);
    let color = mix(lit, u.fog_color, fog);

    return vec4(color, 1.0);
}
