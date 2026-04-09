struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    sun_dir: vec3<f32>,
    fog_color: vec3<f32>,
    fog_far: f32,
    world_size: f32,
    hm_res: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var heightmap: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

fn get_height(ix: i32, iz: i32) -> f32 {
    let res = i32(u.hm_res);
    return textureLoad(heightmap, clamp(vec2(ix, iz), vec2(0), vec2(res - 1)), 0).r;
}

@vertex
fn vs_main(@location(0) pos_xz: vec2<f32>) -> VertexOutput {
    let uv = pos_xz / u.world_size;
    let tc = vec2<i32>(uv * u.hm_res);

    let h = get_height(tc.x, tc.y);
    let world_pos = vec3(pos_xz.x, h, pos_xz.y);

    // Normal from finite differences (4-neighbor)
    let hL = get_height(tc.x - 1, tc.y);
    let hR = get_height(tc.x + 1, tc.y);
    let hD = get_height(tc.x, tc.y - 1);
    let hU = get_height(tc.x, tc.y + 1);
    let step = u.world_size / u.hm_res;
    let normal = normalize(vec3(hL - hR, 2.0 * step, hD - hU));

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.normal = normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let h = in.world_pos.y;

    // Height-based coloring: sand → grass → rock
    let sand  = vec3(0.76, 0.70, 0.50);
    let grass = vec3(0.32, 0.54, 0.22);
    let rock  = vec3(0.50, 0.45, 0.40);

    let sg = smoothstep(8.0, 14.0, h);
    let gr = smoothstep(18.0, 24.0, h);
    let base_color = mix(mix(sand, grass, sg), rock, gr);

    // Directional light (sun) + ambient
    let n = normalize(in.normal);
    let ndl = max(dot(n, u.sun_dir), 0.0);
    let lit = base_color * (0.3 + 0.7 * ndl);

    // Distance fog
    let dist = length(in.world_pos - u.camera_pos);
    let fog = clamp(dist / u.fog_far, 0.0, 1.0);
    let color = mix(lit, u.fog_color, fog);

    return vec4(color, 1.0);
}
