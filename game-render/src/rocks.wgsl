// Rock-specific: instanced deformed icosphere.
// Uniforms, lighting, fog, and shadow bindings provided by common.wgsl prefix.
// Instance data from GPU compute shader via storage buffer.

struct RockInstanceData {
    pos_scale: vec4<f32>,
    color: vec4<f32>,
};

// Scene pass reads instances from group 2
@group(2) @binding(0) var<storage, read> instances: array<RockInstanceData>;
// Shadow pass reads instances from group 1
@group(1) @binding(0) var<storage, read> shadow_instances: array<RockInstanceData>;

// Material atlas (group 3, scene pass only)
@group(3) @binding(0) var atlas: texture_2d_array<f32>;
@group(3) @binding(1) var atlas_sampler: sampler;

const MAT_ROCK: i32 = 3;

struct VertexInput {
    @builtin(instance_index) instance_id: u32,
    @location(0) position: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let inst = instances[in.instance_id];
    let scale = inst.pos_scale.w;
    let world_pos = in.position * scale + inst.pos_scale.xyz;

    var out: VertexOutput;
    out.clip_pos = u.view_proj * vec4(world_pos, 1.0);
    out.world_pos = world_pos;
    out.color = inst.color.rgb;
    return out;
}

@vertex
fn vs_shadow(
    @builtin(instance_index) instance_id: u32,
    @location(0) position: vec3<f32>,
) -> @builtin(position) vec4<f32> {
    let inst = shadow_instances[instance_id];
    let world_pos = position * inst.pos_scale.w + inst.pos_scale.xyz;
    return u.sun_view_proj * vec4(world_pos, 1.0);
}

/// Triplanar sample: blend 3 axis-aligned projections weighted by normal.
fn triplanar_sample(world_pos: vec3<f32>, n: vec3<f32>, layer: i32) -> vec3<f32> {
    let scale = 0.6; // ~1 tile per 1.67 world units — coarser for rocks
    let blend = abs(n);
    let w = blend / (blend.x + blend.y + blend.z + 0.001);

    let tx = textureSample(atlas, atlas_sampler, fract(world_pos.yz * scale), layer).rgb;
    let ty = textureSample(atlas, atlas_sampler, fract(world_pos.xz * scale), layer).rgb;
    let tz = textureSample(atlas, atlas_sampler, fract(world_pos.xy * scale), layer).rgb;

    return tx * w.x + ty * w.y + tz * w.z;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = compute_flat_normal(in.world_pos);
    let tex = triplanar_sample(in.world_pos, n, MAT_ROCK);

    // Blend: texture detail modulated by instance color
    let color = tex * in.color * 2.0;

    let shadow = sample_shadow(in.world_pos);
    let lit = hemisphere_lighting(n, color, shadow, in.world_pos);
    let rim = rim_light(n, in.world_pos);
    let final_color = apply_fog(in.world_pos, lit + rim);
    return vec4(final_color, 1.0);
}
