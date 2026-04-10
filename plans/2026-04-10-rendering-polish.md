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

- [ ] 8b: Verify server + client scene consistency

  **Approach**: Run the server (`make server`), open two browser tabs connecting to the same room. Verify both clients render the identical scene — same terrain, same rock/tree/grass placement, same atmosphere. Confirm players see each other moving.

  **Contracts**:
  - Heightmap is deterministic (already shared via `game-core::generate_heightmap()`)
  - Object scatter is deterministic (same `scatter_objects()` from heightmap data)
  - Both clients render all visual polish (clouds, grass, fog, etc.)
  - Players appear at correct positions and move smoothly
  - No visual desync between clients (same rocks, trees, grass in same locations)

  **Failure modes**: If any randomness crept into scatter or rendering (e.g. using `rand` instead of hash-based placement), clients would see different object layouts. If uniform struct sizes differ between snapshot and client builds, rendering will break.

---

## Snapshot Verification Process

**Every phase must be visually verified before marking complete.** After implementing a phase:

1. Run `make snapshot` to generate dawn/noon/dusk/night images
2. View all four snapshots and compare against the previous phase
3. Save to `snapshots/phaseN/` (never overwrite previous phases — accumulate history)
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

### The Impressionist Test

Hold the final noon snapshot at arm's length. If it could pass for an impressionist landscape painting — patches of color suggesting detail rather than rendering it explicitly, light and atmosphere dominating over geometry — we've succeeded.
