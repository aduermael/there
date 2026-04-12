// Blob shadow: soft dark ellipse under each player instance.
// Vertex shader expands a unit quad into a flat world-space disc at player feet.
// Fragment shader outputs radial falloff alpha.

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Quad corners: unit square centered at origin
const CORNERS = array<vec2<f32>, 4>(
    vec2(-1.0, -1.0),
    vec2( 1.0, -1.0),
    vec2( 1.0,  1.0),
    vec2(-1.0,  1.0),
);

const SHADOW_RADIUS: f32 = 1.2;
const SHADOW_Y_OFFSET: f32 = 0.15; // lift above terrain to clear faceted geometry

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @location(0) inst_pos_yaw: vec4<f32>,
    @location(1) inst_color: vec4<f32>,
) -> VertexOutput {
    let corner = CORNERS[vid];

    // Expand quad in world XZ plane at player foot Y
    let world_pos = vec3(
        inst_pos_yaw.x + corner.x * SHADOW_RADIUS + u.sun_dir.x * 0.1,
        inst_pos_yaw.y + SHADOW_Y_OFFSET,
        inst_pos_yaw.z + corner.y * SHADOW_RADIUS + u.sun_dir.z * 0.1,
    );

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = length(in.uv);
    // Shadow intensity: how much to darken (blend mode multiplies dst by 1-alpha)
    // Reduced from 0.7: directional shadow handles most grounding now
    let intensity = smoothstep(1.0, 0.0, dist) * 0.35;
    return vec4(0.0, 0.0, 0.0, intensity);
}
