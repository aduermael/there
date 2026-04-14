// Tree-specific: instanced cone+cylinder with wind sway.
// Uniforms, lighting, fog, and shadow bindings provided by common.wgsl prefix.
// Instance data from GPU compute shader via storage buffer.

struct TreeInstanceData {
    pos_scale: vec4<f32>,
    foliage_color: vec4<f32>,
};

// Scene pass reads instances from group 2
@group(2) @binding(0) var<storage, read> instances: array<TreeInstanceData>;
// Shadow pass reads instances from group 1 (shadow pipeline has no shadow map bind group)
@group(1) @binding(0) var<storage, read> shadow_instances: array<TreeInstanceData>;

// Material atlas (group 3, scene pass only)
@group(3) @binding(0) var atlas: texture_2d_array<f32>;
@group(3) @binding(1) var atlas_sampler: sampler;

const MAT_BARK: i32 = 4;
const MAT_FOLIAGE: i32 = 5;

struct VertexInput {
    @builtin(instance_index) instance_id: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) vert_color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) is_foliage: f32,
    @location(3) world_normal: vec3<f32>,
};

fn apply_tree_transform(position: vec3<f32>, vert_color: vec3<f32>, inst: TreeInstanceData) -> vec3<f32> {
    let scale = inst.pos_scale.w;
    var local_pos = position * scale;

    // Wind sway: foliage vertices displaced based on height above ground.
    let is_foliage = step(2.0, vert_color.r + vert_color.g + vert_color.b);

    // Crown shape variation: narrow-to-wide per instance
    let shape = inst.foliage_color.a;
    let crown_w = mix(0.6, 1.7, shape);
    local_pos.x *= mix(1.0, crown_w, is_foliage);
    local_pos.z *= mix(1.0, crown_w, is_foliage);
    // Subtle height: wide trees slightly squatter, narrow slightly taller
    let crown_h = mix(1.08, 0.93, shape);
    let foliage_center_y = 1.3 * scale;
    let dy = local_pos.y - foliage_center_y;
    local_pos.y = mix(local_pos.y, foliage_center_y + dy * crown_h, is_foliage);

    let height_factor = saturate(position.y / 2.5);
    let sway_strength = is_foliage * height_factor * height_factor;

    let tree_pos = inst.pos_scale.xyz;
    let phase = tree_pos.x * 0.73 + tree_pos.z * 1.37;

    let wind_x = sin(u.time * 0.8 + phase) * 0.2 + sin(u.time * 1.9 + phase * 2.1) * 0.07;
    let wind_z = sin(u.time * 0.6 + phase * 1.5) * 0.08;

    local_pos.x += wind_x * sway_strength * scale;
    local_pos.z += wind_z * sway_strength * scale;

    return local_pos + inst.pos_scale.xyz;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let inst = instances[in.instance_id];
    let world_pos = apply_tree_transform(in.position, in.vert_color, inst);
    let color = in.vert_color * inst.foliage_color.rgb;
    let is_foliage = step(2.0, in.vert_color.r + in.vert_color.g + in.vert_color.b);

    // Transform normal: apply inverse transpose of the crown deformation
    let shape = inst.foliage_color.a;
    let crown_w = mix(0.6, 1.7, shape);
    let crown_h = mix(1.08, 0.93, shape);
    let inv_w = mix(1.0, 1.0 / crown_w, is_foliage);
    let inv_h = mix(1.0, 1.0 / crown_h, is_foliage);
    let world_normal = normalize(vec3(in.normal.x * inv_w, in.normal.y * inv_h, in.normal.z * inv_w));

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = color;
    out.is_foliage = is_foliage;
    out.world_normal = world_normal;
    return out;
}

@vertex
fn vs_shadow(
    @builtin(instance_index) instance_id: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) vert_color: vec3<f32>,
) -> @builtin(position) vec4<f32> {
    let inst = shadow_instances[instance_id];
    let world_pos = apply_tree_transform(position, vert_color, inst);
    return u.sun_view_proj * vec4(world_pos, 1.0);
}

// triplanar_sample from triplanar.wgsl

const TREE_TEXTURE_SCALE: f32 = 0.35; // ~1 tile per 2.9 world units

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);

    // Select material: bark for trunk, foliage for canopy
    let layer = select(MAT_BARK, MAT_FOLIAGE, in.is_foliage > 0.5);
    let tex = triplanar_sample(in.world_pos, n, layer, TREE_TEXTURE_SCALE);

    // Texture is primary color source, instance color provides hue variation
    let color = mix(in.color, tex, 0.35);

    let shadow = sample_shadow(in.world_pos, n);
    let lit = hemisphere_lighting(n, color, shadow, in.world_pos);
    let final_color = apply_fog(in.world_pos, lit);
    return vec4(final_color, 1.0);
}
