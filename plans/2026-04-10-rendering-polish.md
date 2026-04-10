# Rendering Polish — Impressionist Ambiance

**Date**: 2026-04-10
**Goal**: Transform the flat, prototype look into something that conveys beauty and atmosphere — inspired by impressionism and RDR II's approach of prioritizing mood over raw fidelity. Keep it low-poly, keep it lightweight, make it *feel* alive.

---

## Current State

The renderer has the bones: terrain with height-based coloring, cone trees, deformed-icosphere rocks, distance fog, time-of-day atmosphere. But everything is flat — the sky is an empty gradient, lighting is basic N·L diffuse + constant ambient, there's no color variation on terrain, no grass, no clouds, no post-processing. The scene reads as "programmer art" rather than "stylized."

### What's Missing (Biggest Impact First)

| Gap | Why it matters |
|-----|---------------|
| Empty sky | No clouds = feels like a tech demo, not a world |
| Flat terrain color | Monotone biomes with hard transitions, no life |
| No grass | Ground plane is dead, no movement or texture |
| Basic lighting | Constant ambient washes out form; no sky contribution |
| Linear fog only | No depth layering, no atmospheric perspective |
| No post-processing | Raw linear colors, no tone mapping or mood |
| Static vegetation | Nothing moves, world feels frozen |
| Simple tree shapes | Single cone reads as placeholder |

### Key Files

| File | Role |
|------|------|
| `game-render/src/sky.wgsl` | Fullscreen sky gradient (2 colors, `pow(y, 1.5)`) |
| `game-render/src/terrain.wgsl` | Height-based sand/grass/rock + N·L diffuse + linear fog |
| `game-render/src/trees.wgsl` | Instanced cone+cylinder, flat shading |
| `game-render/src/rocks.wgsl` | Instanced deformed icosphere, flat shading |
| `game-render/src/player.wgsl` | Instanced capsule, flat shading |
| `game-render/src/atmosphere.rs` | Time-of-day parameter computation (sun color, sky colors, fog) |
| `game-render/src/scatter.rs` | Deterministic placement of rocks + trees on heightmap |
| `game-render/src/terrain.rs` | Terrain mesh generation, chunk system, uniforms struct |
| `game-render/src/trees.rs` | Tree mesh generation (trunk cylinder + foliage cone) |
| `game-render/src/lib.rs` | Module exports |
| `game-client/src/renderer.rs` | Main render loop, draw call ordering |
| `game-snapshot/src/render.rs` | Headless snapshot renderer (same pipeline) |

### Uniforms Struct (shared by all shaders)

```rust
pub struct Uniforms {
    view_proj, camera_pos, sun_dir, fog_color, fog_far,
    world_size, hm_res, ambient_intensity, sun_color,
    sky_zenith, sky_horizon
}
```
128 bytes, aligned to 256. Adding fields requires updating this struct, all shaders that declare it, and both `renderer.rs` (client) and `render.rs` (snapshot).

---

## Phase 1: Procedural Clouds

The single biggest visual upgrade. An empty sky screams "prototype." Clouds create depth, scale, drama, and mood — especially with time-of-day lighting.

- [x] 1a: Add procedural clouds to sky shader using layered 2D noise

  **Approach**: In `sky.wgsl`, compute cloud coverage using a hash-based noise function (no texture needed). Layer 2-3 octaves of value noise sampled at `(world_ray_xz / cloud_scale)`. Clouds sit at a virtual altitude — reconstruct a ray from the camera through the pixel, intersect with a horizontal plane at cloud height, sample noise at that XZ.

  **Contracts**:
  - Input: screen UV + camera uniforms (need `inverse_view_proj` or camera_pos + reconstruct ray from clip coords)
  - Output: cloud color blended over sky gradient
  - Cloud coverage should thin near horizon (avoid hard edge)
  - Lit by sun: bright side facing sun_dir, darker underside
  - Time-of-day: clouds pick up sun_color tint (golden at dawn/dusk, blue at night)

  **New uniforms needed**: `time` (f32, for slow cloud drift), `inverse_view_proj` (mat4x4, for ray reconstruction)

  **Failure modes**: Noise banding if resolution too low — use smooth interpolation. Performance on mobile — keep octave count ≤ 3. Cloud plane intersection can miss at extreme pitch — clamp ray direction.

- [x] 1b: Add sun disc and glow halo to sky shader

  **Approach**: In `sky.wgsl`, compute the dot product between the view ray and `sun_dir`. Sun disc = hard threshold. Glow = soft falloff around disc using `pow(dot, exponent)`.

  **Contracts**:
  - Sun disc: small, intense, white/yellow circle
  - Glow: larger soft halo, picks up sun_color
  - Both scale with sun elevation (bigger glow at horizon for dawn/dusk drama)
  - Sun should be visible through thin clouds (attenuated), hidden by thick clouds

---

## Phase 2: Lighting Overhaul

The current constant ambient makes everything look flat. Real outdoor scenes have rich ambient from sky light above and ground bounce below. This phase transforms the shading across all objects.

- [x] 2a: Replace constant ambient with hemisphere lighting in all geometry shaders

  **Approach**: Instead of `ambient_intensity` as a flat scalar, blend between `sky_zenith` (for upward-facing normals) and a ground color (brownish-green, for downward-facing). Formula: `hemisphere = mix(ground_color, sky_color, dot(normal, up) * 0.5 + 0.5)`. This gives terrain subtle blue fill from the sky and warm undertones in crevices.

  **Applies to**: `terrain.wgsl`, `trees.wgsl`, `rocks.wgsl`, `player.wgsl`

  **New uniforms**: `sky_ambient` (vec3 — sky contribution color), `ground_ambient` (vec3 — ground bounce color). Computed in `atmosphere.rs` from existing sky_zenith/sky_horizon.

  **Contracts**:
  - Upward normals get cool sky-tinted ambient
  - Downward/sideways normals get warm ground-tinted ambient
  - Must replace `ambient_intensity` usage everywhere
  - Result should be more colorful shadows, not just dimmer

- [x] 2b: Add rim/fresnel lighting for silhouette definition

  **Approach**: Add a subtle rim light based on `1.0 - dot(normal, view_dir)`. This brightens object edges, creating separation between overlapping silhouettes (critical for trees against terrain, rocks against mountains).

  **Contracts**:
  - Rim intensity should be subtle (multiplier ~0.15-0.25)
  - Tinted by sky color (feels like atmospheric scattering at edges)
  - Applied to terrain, trees, rocks (not players — they have their own colors)

---

## Phase 3: Terrain Color & Texture

The terrain is the largest surface on screen. Breaking up its flat monotone biomes is essential.

- [x] 3a: Add procedural color noise to terrain fragment shader

  **Approach**: In `terrain.wgsl`, modulate the base height-color with a procedural noise pattern. Use 2-3 octaves of value noise at different scales: one large-scale (patches of slightly different green), one fine-scale (per-meter variation). This creates the impression of flowers, soil patches, different grass species — like an impressionist painting.

  **Contracts**:
  - Noise modulates hue and brightness, not just brightness
  - Grass zone: vary between warm green, cool green, hints of yellow/brown
  - Sand zone: vary between warm sand, cooler grey-sand
  - Rock zone: vary between grey, brown, slight blue-grey
  - Variation amplitude: ~15-25% of base color
  - Must use world-space coordinates (not screen-space) so it's stable as camera moves

- [x] 3b: Add slope-based darkening and detail to terrain

  **Approach**: Use the existing normal to detect steep slopes. Darken steep areas (exposed dirt/rock), brighten flat areas (lush grass). Also add slight color shift: steep = more brown, flat = more saturated green.

  **Contracts**:
  - Slope factor from `normal.y` (1.0 = flat, 0.0 = vertical)
  - Steep grass zones → darker, browner (exposed soil)
  - Flat grass zones → brighter, more saturated
  - Should complement height-based coloring, not fight it

---

## Phase 4: Grass Blades

Grass transforms the ground plane from dead to alive. The user specifically requested this. Individual blades catching the light create that impressionist field-of-flowers feel.

- [x] 4a: Create grass blade geometry system with instanced rendering

  **Approach**: New `grass.rs` + `grass.wgsl` in `game-render`. Each grass blade is a simple quad (2 triangles) or a 3-vertex triangle. Scatter instances across grass-zone terrain (height 8-17, slope < 0.3) using the same hash-based placement as `scatter.rs` but at much higher density. Each instance stores position, height, color, and a random rotation.

  **Contracts**:
  - Grass rendered as instanced quads/triangles (1 draw call)
  - Density: ~4-8 blades per square meter in near range, thinning with distance
  - Max instances: configurable cap (start with ~16K, tune for mobile perf)
  - LOD: only render grass within ~60-80 units of camera (beyond that, terrain color carries the impression)
  - Grass color: varies per-blade (multiple greens, some yellow, some dark)
  - Blade height: 0.3-0.8 units, variation per instance
  - Blade width: narrow (~0.05-0.1 units)
  - Face camera (billboard) or random Y-rotation (both work for low-poly)
  - Alpha testing or solid geometry (prefer solid for simplicity — no transparency sorting)

  **Failure modes**: Too many instances kills mobile perf. Need distance-based density falloff. Instance buffer upload cost — compute placement once at init, update visible set per frame based on camera position.

  **Open question**: Billboard quads vs. geometry (3-vertex triangles with slight curve). Billboards are cheaper but less natural. Suggest starting with simple triangles.

- [x] 4b: Add wind animation to grass in vertex shader

  **Approach**: In `grass.wgsl` vertex shader, displace the top vertex of each blade using a sine wave based on `(world_pos.xz + time * wind_speed)`. The base vertex stays anchored to terrain.

  **Contracts**:
  - Wind direction: can be hardcoded (e.g., from the west)
  - Displacement: sinusoidal, ~0.1-0.2 units at blade tip
  - Multiple frequencies for organic feel (base wave + detail noise)
  - Phase varies per blade (use instance hash) to prevent synchronized swaying
  - `time` uniform needed (already added in Phase 1)

- [x] 4c: Add distance fade and density falloff for grass

  **Approach**: Grass alpha/scale fades to zero between 50-80 units from camera. Beyond that, terrain color noise (from Phase 3) carries the visual weight.

  **Contracts**:
  - Smooth fade, no popping
  - Grass blades shrink *and* fade (both contribute)
  - Works with the scatter system's existing hash-based placement

---

## Phase 5: Atmospheric Fog Upgrade

Linear fog is boring. Real atmosphere has exponential falloff and height-dependent density (thicker near ground, thinner up high). This creates the layered depth that makes distant mountains feel *far away*.

- [x] 5a: Replace linear fog with exponential height fog in all geometry shaders

  **Approach**: Replace `clamp(dist / fog_far, 0, 1)` with exponential formula: `1.0 - exp(-dist * fog_density)`. Add height component: fog is denser at lower altitudes. Formula: `density = base_density * exp(-height_falloff * height)`. Integrate along the view ray for physically plausible result (or approximate with endpoint evaluation).

  **Contracts**:
  - Exponential base fog replaces linear
  - Height factor: valleys are hazier, mountaintops are clearer
  - Fog color: blend from fog_color to sky_zenith with altitude (atmospheric perspective — distant things aren't just foggy, they're *bluer*)
  - Must update `terrain.wgsl`, `trees.wgsl`, `rocks.wgsl`, `player.wgsl`, `grass.wgsl`
  - New uniforms: `fog_density` (f32), `fog_height_falloff` (f32)
  - Maintain artistic control: atmosphere.rs computes density/falloff per time-of-day (hazier mornings, clearer noon)

- [x] 5b: Add atmospheric color shift with distance

  **Approach**: Objects far away don't just fade to fog color — they shift toward the sky color (Rayleigh scattering approximation). Blend the fog color from warm/neutral (near) to cool/blue (far, high). This creates the "blue mountains in the distance" effect.

  **Contracts**:
  - Near fog: `fog_color` (matches horizon)
  - Far fog: blend toward `sky_zenith` (the overhead blue)
  - Transition distance: ~100-300 units
  - Subtle — should enhance the feeling of distance, not make everything blue

---

## Phase 6: Vegetation Enhancement

Better tree shapes and subtle animation make the forest feel like a living place.

- [x] 6a: Improve tree mesh to multi-layered foliage

  **Approach**: In `trees.rs`, replace the single cone with 2-3 stacked cones of decreasing radius and slight offset. This creates a spruce/pine silhouette that reads much better. Each cone can have slight random tilt.

  **Contracts**:
  - 3 foliage cones: bottom (widest, lowest), middle, top (narrow, highest)
  - Each cone slightly smaller and offset upward
  - Total triangle count increase: ~2x (from ~160 to ~320 per tree, still trivial)
  - Same instance data format (no changes to shader bindings)
  - Each foliage layer can have slightly different green for depth

- [x] 6b: Add wind sway to trees in vertex shader

  **Approach**: In `trees.wgsl`, apply sinusoidal displacement to foliage vertices based on height above trunk base. Trunk stays mostly still, crown sways more. Use `time` uniform + instance position for phase variation.

  **Contracts**:
  - Displacement increases with vertex height (trunk base = 0, crown tip = max)
  - Subtle: ~0.1-0.3 units of sway at crown
  - Per-tree phase offset from position hash
  - Same wind direction as grass (visual coherence)
  - `time` uniform (shared from Phase 1)

---

## Phase 7: Post-Processing Pass

The final polish layer. Currently the renderer writes directly to the swapchain. Adding an intermediate render target enables tone mapping and color grading — the difference between "technically correct" and "beautiful."

- [x] 7a: Add intermediate render target and post-processing pipeline

  **Approach**: Render the scene to an intermediate RGBA16Float texture instead of directly to the surface. Then draw a fullscreen quad with a post-processing shader that reads from this texture.

  **Changes**:
  - `renderer.rs` (client): create intermediate texture + view, two render passes
  - `render.rs` (snapshot): same two-pass setup
  - New `postprocess.rs` + `postprocess.wgsl` in game-render
  - First pass: all scene geometry → intermediate texture
  - Second pass: fullscreen quad reads intermediate → writes to surface/output

  **Contracts**:
  - Intermediate format: RGBA16Float (HDR headroom for tone mapping)
  - Post-process shader reads from texture via sampler
  - Depth buffer stays as-is (only used in first pass)

  **Failure modes**: RGBA16Float may not be supported on all WebGPU targets. Fallback to RGBA8Unorm if needed (less headroom but still works). Safari mobile: verify format support.

- [x] 7b: Implement tone mapping and color grading in post-process shader

  **Approach**: In `postprocess.wgsl`:
  1. **ACES tone mapping**: Maps HDR → LDR with a filmic curve (preserves highlights, rich shadows)
  2. **Color grading**: Subtle warm shift (raise shadow warmth, slightly desaturate highlights)
  3. **Contrast curve**: Gentle S-curve for punchier image
  4. **Vignette**: Subtle darkening at screen edges (focuses eye on center)

  **Contracts**:
  - Tone mapping: ACES fitted curve (standard constants)
  - Color temperature: slightly warm (+0.02 on red channel in shadows)
  - Saturation boost in midtones (~1.1x)
  - Vignette: smooth radial, ~10-15% darkening at corners
  - All parameters can be hardcoded (no runtime tunables needed for POC)
  - Must look good at all four time-of-day presets (dawn/noon/dusk/night)

---

## Phase 8: Multiplayer Scene Verification

All rendering polish must work in the live multiplayer context — multiple browser clients connected to the server, all seeing the same world with the same visual quality as the snapshots.

- [x] 8a: Fix client WASM build and verify it compiles with all rendering changes

  **Approach**: The client (`game-client`) targets `wasm32-unknown-unknown` via `wasm-pack`. All shader and uniform changes from Phases 1-7 must compile cleanly for the browser. Fix any web-target-specific issues (e.g. `SurfaceTarget` API differences, missing features).

  **Contracts**:
  - `cd game-client && wasm-pack build --target web` succeeds
  - All rendering polish (clouds, hemisphere lighting, terrain noise, grass, fog, trees, post-processing) is present in the browser build

- [x] 8b: Verify server + client scene consistency

  **Approach**: Run the server (`make server`), open two browser tabs connecting to the same room. Verify both clients render the identical scene — same terrain, same rock/tree/grass placement, same atmosphere. Confirm players see each other moving.

  **Contracts**:
  - Heightmap is deterministic (already shared via `game-core::generate_heightmap()`)
  - Object scatter is deterministic (same `scatter_objects()` from heightmap data)
  - Both clients render all visual polish (clouds, grass, fog, etc.)
  - Players appear at correct positions and move smoothly
  - No visual desync between clients (same rocks, trees, grass in same locations)

  **Failure modes**: If any randomness crept into scatter or rendering (e.g. using `rand` instead of hash-based placement), clients would see different object layouts. If uniform struct sizes differ between snapshot and client builds, rendering will break.

---

## Phase 9: Shader DRY Consolidation

All geometry shaders duplicate the same code: Uniforms struct (6×17 lines), fog computation (5×7 lines), hemisphere lighting (4×4 lines), rim/fresnel (3×4 lines), flat normal via derivatives (4×3 lines), hash/noise functions (2×18 lines). That's ~800 lines of duplication across shaders. Adding shadows and AO would multiply this. Consolidate first.

- [x] 9a: Create `common.wgsl` with all shared shader code and Rust-side concatenation

  **Approach**: Create `game-render/src/common.wgsl` containing the shared Uniforms struct, binding declaration, and all shared utility functions. In Rust, concatenate `common.wgsl` as a prefix when loading each geometry shader via `format!("{}\n{}", include_str!("common.wgsl"), include_str!("terrain.wgsl"))`.

  **Shared functions to extract**:
  - `struct Uniforms` + `@group(0) @binding(0) var<uniform> u: Uniforms;` (currently copy-pasted in 6 shaders)
  - `fn compute_flat_normal(world_pos: vec3<f32>) -> vec3<f32>` — `dpdx`/`dpdy` cross product (used in grass, rocks, trees, player)
  - `fn hash2(p: vec2<f32>) -> f32` — hash-based noise (used in terrain, sky)
  - `fn value_noise(p: vec2<f32>) -> f32` — smooth value noise (used in terrain, sky)
  - `fn fbm3(p: vec2<f32>) -> f32` — 3-octave FBM (used in sky, useful for future work)
  - `fn hemisphere_lighting(normal: vec3<f32>, base_color: vec3<f32>) -> vec3<f32>` — hemisphere ambient + N·L diffuse (used in terrain, grass, rocks, trees, player)
  - `fn rim_light(normal: vec3<f32>, world_pos: vec3<f32>) -> vec3<f32>` — fresnel rim (used in terrain, rocks, trees)
  - `fn apply_fog(world_pos: vec3<f32>, lit_color: vec3<f32>) -> vec3<f32>` — exponential height fog + atmospheric color shift (used in all 5 geometry shaders)

  **Contracts**:
  - All shared functions reference `u` (the uniform binding) directly — they are not pure functions, they depend on the global uniform
  - `common.wgsl` must NOT contain `@vertex` or `@fragment` entry points
  - Each shader removes its local Uniforms struct and duplicated functions, keeping only its entry points and shader-specific logic
  - Sky shader is special: it uses Uniforms + noise but not lighting/fog — still benefits from shared struct and noise functions
  - Postprocess shader is independent (different bind group, no Uniforms) — leave it alone

  **Failure modes**: WGSL has no native `#include`. The Rust-side `format!()` concatenation is the simplest approach. Must ensure no name collisions between common functions and shader-local variables. Must verify all 6 shaders still compile and produce identical output.

- [ ] 9b: Verify visual output is unchanged after consolidation

  **Approach**: Run `make snapshot` before and after. Compare all 4 time-of-day images pixel-for-pixel (or visually). No rendering changes should be visible — this is a pure refactor.

  **Contracts**:
  - Dawn, noon, dusk, night snapshots look identical to pre-refactor
  - `cd game-client && wasm-pack build --target web` still compiles
  - Total duplicated shader lines reduced from ~800 to ~0

---

## Phase 10: Fix Noon Exposure

The noon snapshot is washed out / burnt. Root cause: multiple compounding factors that together blow out highlights and kill contrast.

**Analysis** (from atmosphere.rs and postprocess.wgsl):
1. Noon sun color `[1.0, 0.98, 0.92]` — nearly pure white, too intense
2. Noon ambient intensity 0.30 — adds ~50% luminance before sun contribution
3. ACES saturation boost of 1.15× compensates for ACES desaturation but lifts greys at high luminance
4. S-curve contrast at 0.5 strength flattens bright regions (smoothstep plateaus above 0.7)
5. Vignette 0.4 edge darkening makes the center burn more obvious by contrast

- [ ] 10a: Tune atmosphere and post-processing for balanced noon exposure

  **Approach**: Adjust in two places:

  1. `atmosphere.rs` — reduce noon sun intensity and ambient:
     - Noon sun: `[1.0, 0.98, 0.92]` → `[0.90, 0.85, 0.78]` (warmer, less intense)
     - Ambient formula: `0.15 + 0.15 * day_factor` → `0.12 + 0.10 * day_factor` (noon drops from 0.30 to 0.22)

  2. `postprocess.wgsl` — soften post-processing:
     - Saturation boost: 1.15 → 1.08 (less aggressive compensation)
     - S-curve blend: 0.5 → 0.3 (gentler contrast, preserves highlight detail)
     - Vignette: 0.4 → 0.3 (subtler edge darkening)

  **Contracts**:
  - Noon should have warm, rich colors — not blown out or grey
  - Dawn and dusk should still be dramatic (they benefit from lower sun elevation)
  - Night should remain dark and moody
  - All 4 times of day must be visually verified after tuning
  - This is a tuning pass — use the visual-iterate skill to iterate until all 4 presets look good

  **Failure modes**: Reducing noon exposure too much makes it look overcast/dull. The goal is "warm sunny day" not "cloudy day." Iterate with snapshots.

---

## Phase 11: Grass Overhaul

Three problems: blades are pointy triangles (should be simple rectangles), distribution is uniform (should be patchy), and grass doesn't appear near rocks. The grass-ground transition is also harsh because blades are fully opaque with no terrain color matching.

- [ ] 11a: Change grass blade geometry from triangle to rectangle

  **Approach**: In `grass.rs`, replace the 3-vertex triangle blade with a 4-vertex quad (2 triangles). The quad is slightly narrower at the top for a natural look, but **not** pointy — it should read as a simple rectangular blade.

  **Current** (grass.rs ~line 178):
  ```
  3 vertices: bottom-left, bottom-right, tip → 1 triangle
  ```

  **New**:
  ```
  4 vertices: bottom-left, bottom-right, top-left, top-right → 2 triangles
  Indices: [0, 1, 2, 1, 3, 2]
  ```

  Vertex layout stays the same: `[x, y, z, bend]` where `bend = 0.0` at base, `1.0` at top. Top vertices get `bend = 1.0` for wind animation (already in grass.wgsl).

  **Contracts**:
  - Blade shape: rectangular, width ~0.08 at base, ~0.04 at top
  - Blade height: 0.6 (unchanged)
  - Triangle count per blade: 1 → 2 (16K blades × 2 = 32K tris — still lightweight)
  - Wind animation continues to work (top vertices bend, base anchored)
  - `bend` attribute interpolates across quad for smoother wind deformation

- [ ] 11b: Implement patch-based grass distribution with rock-aware placement

  **Approach**: Replace the uniform grid placement in `scatter.rs` with a two-pass system:

  **Pass 1 — Patch centers**: Iterate at a coarser grid (e.g., every 10-12 texels). Use hash to accept ~40-50% of positions as patch centers. Each patch has a random radius (3-6 texels).

  **Pass 2 — Grass within patches**: For each active patch, iterate the fine grid (step 2) within the patch radius. Apply existing height/slope filters. Higher acceptance rate within patches (~80-90%) creates dense clumps. Sparse grass (10-15% acceptance) in non-patch areas prevents bald spots.

  **Rock-aware placement**: After computing rocks, add dedicated grass rings around rock bases. For each rock, scatter grass within a 2-4 unit radius at slightly relaxed height/slope thresholds (rocks sit on high terrain, so grass can extend into the transition zone height 17-20, slope up to 0.5). This makes rocks feel grounded in vegetation rather than floating on bare terrain.

  **Contracts**:
  - Grass still deterministic (hash-based, no `rand`)
  - Patches create visible clumps — not uniform green carpet
  - Some rocks have grass at their base (foreground interest)
  - Total instance count stays within MAX_GRASS (16384) — patches redistribute density, not increase it
  - Distance-based density falloff still applies (50-80 units from camera)

  **Failure modes**: Patches too large = looks uniform again. Patches too small = looks spotted/diseased. Aim for natural meadow feel — some areas lush, some areas bare earth showing through terrain noise (Phase 3).

- [ ] 11c: Improve grass-ground color blending

  **Approach**: The disconnect comes from grass being fully opaque with colors that don't match the terrain beneath. Fix in two ways:

  1. **Match blade base color to terrain**: In `scatter.rs`, when placing grass, sample the terrain's height-based color at that position (same logic as terrain.wgsl's color bands). Set the blade's instance color to a variation of this terrain color, ensuring grass blends with its surroundings.

  2. **Blade base darkening**: In `grass.wgsl` fragment shader, darken the bottom ~20% of each blade (where `bend` interpolant is near 0). This simulates the blade emerging from shadow at ground level and hides the seam.

  **Contracts**:
  - Blade base color is close to terrain color at that position (within 15% variation)
  - Blade tips are lighter/more saturated (sunlit)
  - No alpha blending needed — keep grass fully opaque (simpler pipeline, no sort issues)
  - The visual transition from terrain to grass should feel gradual, not a hard line

---

## Phase 12: Shadow Mapping

The scene has no cast shadows — everything is lit uniformly. A single directional shadow map from the sun transforms depth perception. Keep it lightweight for WebGPU mobile.

- [ ] 12a: Add shadow depth pass and infrastructure

  **Approach**: Add a depth-only render pass from the sun's point of view. This is a third render pass (shadow → scene → postprocess).

  **New files**:
  - `game-render/src/shadow.rs` — shadow map renderer: creates depth texture, manages shadow pipeline, computes sun-view-projection matrix
  - `game-render/src/shadow.wgsl` — minimal vertex-only shader (transforms geometry to sun clip space, no fragment output, depth-only)

  **Shadow map settings**:
  - Resolution: 512×512 (single Depth32Float texture) — good quality/perf tradeoff
  - Orthographic projection from sun direction, covering the visible scene area (~200×200 units centered on camera)
  - Rendered geometry: terrain + rocks + trees (skip grass — too thin to cast meaningful shadows, skip players — dynamic but tiny)
  - Bias: small constant + slope-scale to prevent shadow acne

  **Uniforms expansion**: Add `sun_view_proj: mat4x4<f32>` (64 bytes) to the Uniforms struct. This pushes the struct past 256 bytes — pad to 512 bytes (next wgpu alignment boundary). Update the Uniforms struct in `terrain.rs`, `common.wgsl`, and both renderers.

  **Render pass order**:
  1. Shadow depth pass (sun POV → shadow depth texture)
  2. Scene pass (camera POV → HDR intermediate, samples shadow texture)
  3. Postprocess pass (HDR → surface)

  **Bind group**: The shadow depth texture needs to be bound as a texture+sampler in the scene pass. Add a second bind group (group 1) for shadow resources, or extend the existing uniform bind group.

  **Contracts**:
  - Shadow map rendered every frame (sun moves with time of day)
  - Shadow pass uses the same terrain/rock/tree geometry — reuses existing vertex/instance buffers
  - Shadow depth texture created once, reused each frame
  - At night (sun below horizon), skip shadow pass entirely

  **Failure modes**: Shadow map too small (512px) can cause blocky shadows at distance — acceptable for stylized look. Shadow acne from insufficient bias — tune per-scene. Peter-panning from too much bias — keep bias minimal.

- [ ] 12b: Sample shadow map in geometry shaders

  **Approach**: Add a `sample_shadow(world_pos: vec3<f32>) -> f32` function in `common.wgsl`. All geometry shaders call this in their lighting calculation.

  ```wgsl
  fn sample_shadow(world_pos: vec3<f32>) -> f32 {
      let light_clip = u.sun_view_proj * vec4(world_pos, 1.0);
      let light_ndc = light_clip.xyz / light_clip.w;
      let shadow_uv = light_ndc.xy * 0.5 + 0.5;
      // Flip Y for texture coordinates
      let uv = vec2(shadow_uv.x, 1.0 - shadow_uv.y);
      // Out of shadow map bounds = fully lit
      if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 { return 1.0; }
      let shadow_depth = textureSample(shadow_map, shadow_sampler, uv).r;
      let current_depth = light_ndc.z;
      let bias = 0.003;
      return select(0.3, 1.0, current_depth - bias <= shadow_depth);
  }
  ```

  Then modify `hemisphere_lighting()` in `common.wgsl` to factor in shadow:
  ```wgsl
  let shadow = sample_shadow(world_pos);
  let lit = base_color * (ambient + ndl * u.sun_color * shadow);
  ```

  Ambient is NOT affected by shadows (it's sky light, not sun light). Only the N·L diffuse term gets shadow attenuation. This means shadowed areas keep their hemisphere ambient coloring (blue-ish from sky) — which looks correct.

  **Shadow strength**: The `select(0.3, 1.0, ...)` means shadows darken to 30% of sun contribution, not pure black. This keeps the stylized, painterly feel.

  **Contracts**:
  - All geometry (terrain, grass, rocks, trees, players) receives shadows via the shared function
  - Shadows are soft-edged due to low resolution (512px) — this is a feature, not a bug for stylized look
  - Shadow darkness tunable via single constant in `common.wgsl`
  - Both client and snapshot renderers get shadows (shared code via `game-render`)

---

## Phase 13: Ambient Occlusion

Vertex-based AO computed at scatter time is the cheapest effective approach: ~0.05ms/frame vs ~2ms for SSAO. Perfect for mobile WebGPU.

- [ ] 13a: Compute AO factor at scatter time and store in instance data

  **Approach**: In `scatter.rs`, after placing each instance (rock, tree, grass), compute a local AO factor by sampling surrounding terrain heights. Higher neighbors = more occlusion.

  ```rust
  fn compute_local_ao(heightmap: &[f32], hm_res: usize, x: f32, z: f32, texel_size: f32) -> f32 {
      let cx = (x / texel_size) as usize;
      let cz = (z / texel_size) as usize;
      let center_h = heightmap[cz * hm_res + cx];
      let mut occlusion = 0.0;
      // Sample 8 neighbors at ~2 unit radius
      for &(dx, dz) in &[(-2,0),(2,0),(0,-2),(0,2),(-1,-1),(1,-1),(-1,1),(1,1)] {
          let nx = (cx as i32 + dx).clamp(0, hm_res as i32 - 1) as usize;
          let nz = (cz as i32 + dz).clamp(0, hm_res as i32 - 1) as usize;
          let nh = heightmap[nz * hm_res + nx];
          occlusion += (nh - center_h).max(0.0); // How much higher is neighbor
      }
      // Normalize: 0.0 = heavily occluded, 1.0 = fully open
      (1.0 - (occlusion / 8.0).min(1.0)).max(0.3) // Never darker than 0.3
  }
  ```

  **Storage**: All instance structs are currently 32 bytes (2×vec4). The 4th component of the second vec4 is used for rotation (grass) or is implicit padding (rocks, trees). Options:
  - **Grass**: `color_rotation` = `[r, g, b, rotation]` — pack AO into the color by pre-multiplying: `color *= ao_factor`. No struct change needed.
  - **Rocks/Trees**: Same approach — pre-multiply AO into instance color at scatter time.

  This means AO is baked into vertex colors with zero per-frame cost.

  **Contracts**:
  - Instances in valleys/depressions are darker
  - Instances on ridges/hilltops are brighter
  - AO is subtle (min 0.3 factor) — avoids black patches
  - No instance struct changes — AO baked into existing color channels
  - Deterministic (same heightmap → same AO)

- [ ] 13b: Add terrain self-occlusion in terrain shader

  **Approach**: The terrain itself should also show AO in valleys and concavities. In `terrain.wgsl`, sample the heightmap at the current fragment's world position ± a small offset. If surrounding terrain is higher, darken.

  This is similar to the scatter AO but computed per-fragment. Since terrain already samples the heightmap (it's in a texture bind group), this adds ~4 texture samples per terrain fragment.

  ```wgsl
  fn terrain_ao(world_pos: vec3<f32>) -> f32 {
      let tc = world_pos.xz / u.world_size;
      let texel = 2.0 / u.hm_res; // 2-texel radius
      let center_h = world_pos.y;
      var occ = 0.0;
      occ += max(textureLoad(heightmap, tc + vec2(texel, 0.0)).r - center_h, 0.0);
      occ += max(textureLoad(heightmap, tc - vec2(texel, 0.0)).r - center_h, 0.0);
      occ += max(textureLoad(heightmap, tc + vec2(0.0, texel)).r - center_h, 0.0);
      occ += max(textureLoad(heightmap, tc - vec2(0.0, texel)).r - center_h, 0.0);
      return clamp(1.0 - occ * 0.5, 0.3, 1.0);
  }
  ```

  **Contracts**:
  - Valleys and terrain concavities get subtle darkening
  - Ridges and peaks stay bright
  - 4 extra texture samples per terrain fragment (~0.2ms cost)
  - Complements instance AO (objects darken in same areas as terrain)

---

## Phase 14: Final Visual Pass & Browser Verification

Verify everything works together across all 4 times of day and in the browser.

- [ ] 14a: Full visual iteration and tuning

  **Approach**: Use the visual-iterate skill to render all 4 times of day. Evaluate and tune:
  - Shadow strength and bias across all times of day
  - AO intensity (not too dark, not invisible)
  - Grass patch density and distribution (natural meadow feel)
  - Grass-rock interaction (grass visible around rocks in foreground)
  - Overall color balance (noon no longer burnt, dawn/dusk still dramatic)

  Iterate until all 4 snapshots look cohesive and polished.

- [ ] 14b: Verify WASM build and browser rendering

  **Approach**: Same as Phase 8 — build the client WASM and test in browser.
  - `cd game-client && wasm-pack build --target web` must succeed
  - Shadow map, AO, and grass changes must all render correctly in WebGPU browser context
  - Run server + 2 browser tabs, verify visual consistency

  **Contracts**:
  - All new features (shadows, AO, patchy grass, rectangular blades) visible in browser
  - Performance acceptable on mid-range hardware (target: 60fps at 1080p)
  - No visual desync between clients

---

## Snapshot Verification Process

**Every phase must be visually verified before marking complete.** After implementing a phase:

1. Run `make snapshot` to generate dawn/noon/dusk/night images
2. View all four snapshots and compare against the previous phase
3. Save to `snapshots/2026-04-10-rendering-polish/phaseN/` (plan-specific folder, never overwrite previous phases — accumulate history)
4. Iterate on the code if the result doesn't meet the phase's success criteria below
5. Only commit and move on once the visuals are genuinely ready to deliver

Do not skip this step. The snapshot tool is fast (~2s per cycle) and catching issues early avoids compounding problems across phases.

---

## Success Criteria

- **Phase 1 done**: Sky has clouds + sun glow. Scene immediately reads as "outdoors" not "tech demo."
- **Phase 2 done**: Shadows have color (blue-ish in shade, warm in light). Objects have visible form even in shadow.
- **Phase 3 done**: Terrain has visual variety — patches, subtle color shifts. No more monotone green.
- **Phase 4 done**: Grass blades visible in foreground, gently swaying. Ground feels alive.
- **Phase 5 done**: Distant terrain has blue atmospheric haze. Valleys are mistier than peaks.
- **Phase 6 done**: Trees have better silhouettes, gentle sway. Forest reads as natural.
- **Phase 7 done**: Image has film-like quality — rich colors, gentle contrast, warm mood. Dawn/dusk are dramatic.
- **Phase 8 done**: Two browser clients connected to the same server see the identical polished scene. Players see each other.
- **Phase 9 done**: All duplicated shader code consolidated into `common.wgsl`. Shaders are short, focused, and easy to modify. Visual output identical to pre-refactor.
- **Phase 10 done**: Noon is warm and rich, not washed out. All 4 times of day have balanced exposure.
- **Phase 11 done**: Grass is rectangular blades in natural patches. Some grass grows around rock bases. Blades blend smoothly into terrain.
- **Phase 12 done**: Objects cast sun shadows onto terrain and each other. Shadows are soft and stylized, not harsh.
- **Phase 13 done**: Valleys and terrain concavities are subtly darker. Objects in depressions feel grounded. Ridges are bright.
- **Phase 14 done**: All features verified in browser at 60fps. Two clients see the same polished scene.

### The Impressionist Test

Hold the final noon snapshot at arm's length. If it could pass for an impressionist landscape painting — patches of color suggesting detail rather than rendering it explicitly, light and atmosphere dominating over geometry — we've succeeded.
