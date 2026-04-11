# Rendering Upgrade: Compute Shaders, Textures & Visual Polish

**⚠️ MANDATORY Iteration protocol — run this at EVERY phase boundary, no exceptions:**
1. Run `make snapshot` to capture all 4 times of day
2. Launch **3 critic sub-agents in parallel** (art direction, technical quality, comparative improvement vs previous phase)
3. Each critic scores 1-10 and gives specific feedback
4. If any critic < 7: synthesize feedback, iterate (up to 5 iterations per phase)
5. All 3 critics ≥ 7: phase is shippable, commit + archive + move on
Do NOT skip the critic loop. Do NOT commit the phase as done before critics have passed.

**Snapshot archiving**: After each phase is complete (all tasks in the phase committed), copy the final snapshots into a phase-specific directory:
```
mkdir -p snapshots/2026-04-10-rendering-upgrade/phase-N
cp snapshots/*.png snapshots/2026-04-10-rendering-upgrade/phase-N/
```
Where N is the phase number. Include this copy in the phase's final commit.

---

## Context

The project has completed 11 phases of visual iteration. The rendering pipeline is mature with a single compute shader (grass), and all other rendering via vertex/fragment. This plan adds compute shaders across the pipeline, introduces a low-res pixel texture system, performs foundational refactoring for DRY/simplicity, and systematically upgrades visual quality while keeping everything mobile/web-performant.

### Current Architecture
- **5 GPU passes per frame**: grass compute → shadow (1024x1024) → scene (HDR Rgba16Float) → SSAO (half-res R8) → postprocess (surface)
- **Only compute shader**: `grass_compute.wgsl` (GPU instance generation + indirect draw)
- **No texture assets**: all surfaces use procedural noise coloring, no UV coordinates on any mesh
- **No anti-aliasing, no bloom**: HDR values above 1.0 exist but are invisible
- **Uniforms struct**: duplicated in 4 WGSL files + 1 Rust struct (5 sync points)
- **11 pipeline creation calls**: heavy boilerplate repetition across renderers
- **Trees/rocks**: CPU scatter at startup, all instances drawn every frame with zero culling
- **Renderer orchestration**: duplicated between game-client and game-snapshot

### Key Architecture Observations
- `common.wgsl` is prepended to 6 geometry shaders via `format!("{}\n{}")` — good pattern but 3 shaders (ssao, postprocess, grass_compute) bypass it and duplicate the Uniforms struct
- Trees, rocks, and player renderers have nearly identical struct fields, buffer creation, draw, and draw_shadow methods
- Shadow map is at bind group 3 for terrain but group 1 for everything else — prevents moving shadow bindings into common.wgsl
- Grass compute pipeline is proven and clean — direct template for trees/rocks/future compute work
- All 4 bind group slots used by terrain; trees/rocks/player have 2 free slots each

### Files Involved
- Shaders: `game-render/src/*.wgsl` (10 files)
- Renderers: `game-render/src/{terrain,trees,rocks,grass,player,sky,ssao,postprocess,shadow,scatter,atmosphere}.rs`
- Orchestration: `game-client/src/renderer.rs`, `game-snapshot/src/render.rs`
- Uniforms: `game-render/src/terrain.rs:12-36` (Rust) + common.wgsl + ssao.wgsl + postprocess.wgsl + grass_compute.wgsl

---

## Phase 1: Design Docs & Shader Refactoring

Refactor before adding features. Every subsequent phase adds shaders, compute passes, and bind groups. A clean foundation makes all of that easier and prevents the codebase from accumulating workarounds.

- [x] 1a: Create `docs/design-philosophy.md` — visual style principles (impressionism feel, ambiance over fidelity, RDR2-inspired lighting philosophy, low-res pixel art + low-poly charm, mobile/web-first, hardcoded engine), to be maintained and expanded over time
- [x] 1b: Create `docs/future-rendering-techniques.md` — document deferred techniques not in this plan: tiled/clustered forward lighting, screen-space reflections, GPU-driven terrain indirect draw, Hi-Z occlusion culling, depth of field, terrain geo-morphing, SSIL, full froxel volumetric fog. For each: what it is, when to add it, estimated effort
- [x] 1c: Unify Uniforms — extract `uniforms.wgsl`, prepend it to ALL shaders (including ssao, postprocess, grass_compute) using the existing `format!` concatenation pattern. Remove all 3 duplicate Uniforms struct declarations. Single source of truth.
- [x] 1d: Extract shared WGSL snippets — create `noise.wgsl` (hash2, value_noise, fbm3, cell_hash, ign) and `fullscreen.wgsl` (fullscreen triangle vertex shader). Compose shaders by concatenating the snippets they need. Eliminate all duplicated shader functions across the 10 WGSL files.
- [x] 1e: Standardize bind group layouts — restructure terrain to put shadow map at group 1 (matching all other geometry shaders). Move heightmap to group 2, chunk offset to group 3. Move shadow binding declarations into common.wgsl. Update uniform BGL to include COMPUTE visibility so grass_compute can reuse the shared BGL.
- [x] 1f: Pipeline builder helpers — extract `create_scene_pipeline()` and `create_shadow_pipeline()` helper functions with options (cull mode, depth compare, bias, vertex layouts). Extract `SHADOW_DEPTH_BIAS` constant. Reduce 11 pipeline creation blocks to helper calls.
- [x] 1g: InstancedMeshRenderer — extract shared abstraction for trees, rocks, player covering: struct fields (pipeline, shadow_pipeline, vertex/index/instance buffers, counts), buffer creation, draw(), draw_shadow(). Parameterized by vertex layout and instance type. Target: collapse ~500 lines of near-identical code.
- [x] 1h: Unify renderer orchestration — extract the 5-pass frame pipeline into a shared function or module in game-render that both game-client renderer.rs and game-snapshot render.rs call. New passes only need to be added in one place.

## Phase 2: Quick Visual Wins

Small shader changes, no new passes or pipelines. Outsized visual impact.

- [x] 2a: Cloud shadows on terrain — in hemisphere_lighting (common.wgsl), project fragment world_pos onto cloud plane from sun_dir, sample FBM cloud density, attenuate direct sun lighting. Moving cloud shadows across the landscape for near-zero cost.
- [x] 2b: Contact shadows — short-range screen-space ray march (12 steps) along light direction in postprocess.wgsl (depth texture already available). Catches fine shadow detail (grass/rock contact lines) that the shadow map resolution misses.
- [x] 2c: Better sun halo — replace `pow(sun_dot, 64)` with Henyey-Greenstein phase function in sky.wgsl. Dual-lobe (g=0.76 forward, g=-0.3 back) for physically correct Mie scattering glow. Especially better at dawn/dusk.
- [x] 2d: View-dependent fog color — blend fog color toward sun_color based on `dot(view_dir, sun_dir)` in apply_fog(). Adds atmospheric directionality — looking toward the sun gets a warm haze, looking away stays cool.

## Phase 3: Compute Bloom & Anti-Aliasing

First new post-processing passes. Bloom makes the HDR pipeline visible to the player.

- [x] 3a: Compute bloom — threshold extraction from HDR buffer, 5-6 level mip chain with compute downscale (13-tap) and upscale (9-tap tent) dispatches. Per-level bloom weights. Composite additively before tonemapping in postprocess. Makes sun, specular, dawn/dusk glow dramatically.
- [x] 3b: FXAA — single fullscreen fragment pass after tonemapping (~100 lines WGSL). Stopgap anti-aliasing until TAA in Phase 7. Immediately fixes aliasing on grass blades and tree silhouettes.

## Phase 4: GPU-Driven World

Port the proven grass_compute pattern to trees and rocks. Upgrade shadows.

- [x] 4a: Compute tree scattering — new `trees_compute.wgsl`. Camera-centered grid, deterministic hash, height/slope filtering, frustum cull, distance cull, LOD selection. Atomic append to storage buffer + indirect draw args. Replace CPU scatter and static instance buffer. Enable much higher tree density.
- [x] 4b: Compute rock scattering — same pattern for rocks. Share compute scatter helpers with trees where logic overlaps (hash, cull, heightmap sampling). Replace CPU scatter and static instance buffer.
- [x] 4c: Cascaded shadow maps — 3 cascades (near ~20u, mid ~60u, far ~200u). Texture array with per-cascade orthographic projection. Update sample_shadow() in common.wgsl for cascade selection based on view-space depth. 3 shadow render passes. Dramatically better near-camera shadow resolution + extended shadow distance.
- [x] 4d: Improved shadow filtering — 8-tap rotated Poisson disk PCF with IGN per-pixel rotation in sample_shadow(). Purely a common.wgsl change. Combine with CSM for excellent shadow quality.

**Parallel Tasks: 4a, 4b**

## Phase 5: Pixel Texture System

Add low-res pixel art textures. No external asset pipeline — generate procedurally in Rust. Nearest-neighbor filtering preserves the blocky pixel art aesthetic.

- [ ] 5a: Texture atlas infrastructure — procedurally generate 16x16 / 32x32 pixel art material tiles in Rust code (grass, dirt, sand, rock, bark, foliage). Create a `Texture2DArray` with Nearest filtering + Repeat addressing. Upload via `queue.write_texture()` following the heightmap pattern.
- [ ] 5b: Terrain texturing — world-space UV tiling in terrain.wgsl fragment shader. Sample pixel texture atlas using scaled world XZ coordinates. Blend texture with existing procedural color (texture provides detail, procedural provides biome variation). Height/slope-based material selection.
- [ ] 5c: Tree & rock texturing — triplanar mapping in fragment shaders (compute UVs from world position + surface normal, no mesh UV changes needed). Apply bark texture to tree trunks, foliage texture to canopy, rock texture to boulders. Instance color variation still modulates the result.

## Phase 6: Water & Atmosphere

Major new visual elements. Water is essential for the Valheim-like world.

- [ ] 6a: Water rendering — new water.rs + water.wgsl. Flat grid at water level, depth-based color (shallow turquoise → deep blue from depth buffer), animated FBM surface normals (reuse existing value_noise), Fresnel sky reflection, sun specular highlight, shoreline foam where depth ≈ 0. Renders after opaque geometry, before postprocess.
- [ ] 6b: Multi-layer clouds with self-shadowing — 2-3 cloud planes in sky.wgsl at different altitudes. Sun-direction density offset creates fake volumetric self-shadowing (dark cloud bases, bright tops). Composited front-to-back. Stepping stone toward full volumetric clouds in Phase 9.
- [ ] 6c: Compute auto-exposure — luminance histogram via compute shader (256 bins, atomic shared memory). Second compute pass reads histogram, computes weighted average (trimming top/bottom 5%), smooths toward target exposure with exponential moving average. Apply as multiplier before tonemapping. Makes dawn/dusk transitions dramatic.

## Phase 7: Temporal Quality

Replace fragment SSAO with compute GTAO. Add TAA for proper anti-aliasing.

- [ ] 7a: Compute SSAO (GTAO) — ground-truth ambient occlusion via horizon-angle tracing in 2-4 screen-space directions. Compute shader with 16x16 shared memory tiles (load depth tile once, reuse across all pixels). Compute bilateral blur pass. Replace existing fragment SSAO + fragment blur. Better quality, lower bandwidth.
- [ ] 7b: TAA — sub-pixel jitter (Halton sequence, 8 positions), per-frame history buffer (Rgba16Float), camera-derived motion vector reprojection, neighborhood clamping (3x3 min/max) to prevent ghosting. Remove FXAA. Add CAS (Contrast Adaptive Sharpening) pass to counter TAA blur.

## Phase 8: Physical Sky & Volumetric Light

Replace hand-tuned atmosphere keyframes with physics. Unified volumetric lighting.

- [ ] 8a: Atmospheric scattering — compute transmittance LUT (256x64, baked once at init) + sky-view LUT (192x108, per frame). Rayleigh + Mie single-scattering integral. Replace sky gradient, sun glow, and atmosphere.rs keyframe system. Aerial perspective for distant terrain replaces fog color with physically-derived scattering.
- [ ] 8b: Compute volumetric light — half-res compute pass. For each pixel, march 16-32 steps from camera toward world position, accumulate fog density × shadow map sample × phase function. Replaces both apply_fog() inline fog AND god rays in postprocess with a unified system. Light shafts through tree canopies, valley fog, height-varying density.

## Phase 9: Volumetric Clouds

The capstone visual upgrade. Raymarched 3D cloud volumes.

- [ ] 9a: 3D noise texture generation — compute shader generates 64³ Perlin-Worley shape noise at init (one-shot dispatch). Stored as Rgba8 3D texture. Optional 32³ detail noise for edge erosion.
- [ ] 9b: Quarter-res cloud raymarch — compute pass. March through cloud slab (base → top), sample 3D noise, height-based density gradient (flat bases, rounded tops), 2D coverage map, Beer's law self-shadowing (4-6 light march steps), Henyey-Greenstein phase. Blue noise ray jitter. Alpha accumulation with early-out.
- [ ] 9c: Temporal reprojection & upscale — blend current cloud frame with reprojected previous frame (camera motion vectors, 95% history blend). Upscale from quarter-res to full-res with depth-aware bilateral filter. Composite into sky. Critical for performance — quarter-res is only viable with temporal accumulation smoothing the noise.

---

## Open Questions

- **Texture generation vs. loading**: Pixel textures 100% procedural Rust, or allow loading small PNGs via `image` crate (already a dependency in game-snapshot, would need adding to game-render)?
- **Shared compute scatter module**: Should trees_compute and rocks_compute be separate WGSL files, or a single parameterized scatter_compute.wgsl?
- **CSM texture format**: Texture2DArray vs. single large atlas? Array is cleaner but verify WebGPU `textureSampleCompare` works with array layers.
- **Bind group pressure**: Adding textures + depth buffer to geometry shaders may require merging bindings within existing groups. Exact layout finalized during Phase 1e.

## Success Criteria

- Each phase: `make snapshot` produces correct dawn/noon/dusk/night images, no visual regressions
- Phase 1: zero duplicated Uniforms declarations, zero duplicated draw patterns, single-point renderer orchestration
- Phase 3+: bloom visible on sun/specular, no aliasing on grass/tree silhouettes
- Phase 4: trees/rocks GPU-culled (only visible instances drawn), shadow quality visibly better near camera
- Mobile/web: full pipeline under 16ms at 1080p on mid-range GPU (verify with browser profiling)
- No new external crates without explicit approval
