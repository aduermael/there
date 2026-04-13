# Safari Water Bug Fix + Performance Optimization

**Date:** 2026-04-12
**Goal:** Fix the water level bug on Safari, then optimize rendering to run smoothly on Safari/iOS without degrading visual quality. Prefer refactoring over patches; reduce code duplication (DRY).

---

## Context

**Water bug:** On Safari and iOS Safari, the water surface visually follows the camera up/down. Works correctly on Chrome/Firefox. Root cause: `water.wgsl:33-35` derives screen UVs from `@builtin(position)` (rasterized framebuffer coords divided by texture dimensions). Safari's WebGPU handles framebuffer coordinate conventions differently. The SSAO and postprocess shaders avoid this by using normalized UVs interpolated from the vertex shader via `fullscreen.wgsl`.

**Performance:** The game has ~23 GPU pass transitions per frame, heavy per-pixel noise (cloud shadows = 3 fbm3 calls in every geometry shader, water = 8+ fbm3 calls per fragment), and no quality scaling. Safari/Metal has higher per-pass overhead, slower atomics, and less optimized WGSL compilation than Chrome/Dawn.

**Shader assembly:** Shaders are built via `format!()` + `include_str!()` concatenation. Shared includes: `uniforms.wgsl`, `noise.wgsl`, `common.wgsl`, `fullscreen.wgsl`, `triplanar.wgsl`. New shared files can be added the same way.

---

## Phase 1: Fix Water Level Bug

The water fragment shader uses `in.clip_pos.xy / tex_size` to derive screen UVs. This is fragile and breaks on Safari. The fix: compute a proper `screen_uv` in the vertex shader (from clip-space coordinates before the rasterizer) and interpolate it to the fragment shader.

**Files:** `game-render/src/water.wgsl`

- [x] 1a: Add `screen_uv` output to water vertex shader, compute it from clip-space position (`clip.xy / clip.w * 0.5 + vec2(0.5, -0.5) + 0.5` or equivalent NDC-to-UV transform). In the fragment shader, replace `in.clip_pos.xy / tex_size` with the interpolated `in.screen_uv`. Derive `pixel` from `screen_uv * tex_size`. Verify: depth reconstruction should now be camera-independent and work identically on all browsers.

---

## Phase 2: DRY Shader Refactoring

Significant code duplication exists across shaders. This phase extracts shared utilities into reusable include files, reducing maintenance risk and making Phase 3-4 changes cleaner.

**Duplication found:**
- `linearize_depth` / `reconstruct_pos` / NDC-to-UV transform duplicated across ssao.wgsl, postprocess.wgsl, water.wgsl (and sky.wgsl for ray reconstruction)
- `get_height` / `get_height_world` / `compute_slope` duplicated identically across grass_compute.wgsl, rocks_compute.wgsl, trees_compute.wgsl
- Cloud layer parameters (altitude, scale, coverage, drift) duplicated between sky.wgsl and common.wgsl with sync risk
- `TAU` constant defined separately in grass_compute.wgsl and trees_compute.wgsl; used as inconsistent literals elsewhere (6.28318 vs 6.283185)
- `fbm3` is used everywhere but a 2-octave variant (`fbm2`) is needed for optimization and doesn't exist

**Files:** `game-render/src/noise.wgsl`, `game-render/src/ssao.wgsl`, `game-render/src/postprocess.wgsl`, `game-render/src/water.wgsl`, `game-render/src/sky.wgsl`, `game-render/src/common.wgsl`, `game-render/src/grass_compute.wgsl`, `game-render/src/rocks_compute.wgsl`, `game-render/src/trees_compute.wgsl`, plus new shared include files and Rust `format!()` assembly in each renderer's `.rs` file

- [x] 2a: **noise.wgsl** -- Add `const TAU: f32 = 6.2831853;` and `fn fbm2()` (2-octave variant). Remove standalone `TAU` definitions from grass_compute.wgsl and trees_compute.wgsl. Replace all bare `6.28318*` literals in ssao.wgsl and common.wgsl with `TAU`.

- [x] 2b: **New `depth_utils.wgsl`** -- Extract `linearize_depth(d) -> f32` (with shared `NEAR`/`FAR` constants), `reconstruct_pos(uv, depth) -> vec3<f32>`, and `ndc_to_uv(ndc_xy) -> vec2<f32>` (the `x*0.5+0.5, 1-(y*0.5+0.5)` pattern). Refactor ssao.wgsl, postprocess.wgsl, and water.wgsl to use these shared functions. Update the Rust-side `format!()` assembly to include `depth_utils.wgsl` for these shaders. Remove the duplicate `linearize`/`linearize_depth`/`CS_NEAR`/`CS_FAR`/`NEAR`/`FAR` definitions.

- [x] 2c: **New `heightmap_utils.wgsl`** -- Extract `get_height(tc)`, `get_height_world(wx, wz)`, and `compute_slope(tc)` from the compute shaders. Refactor grass_compute.wgsl, rocks_compute.wgsl, and trees_compute.wgsl to use the shared file. Update Rust-side `format!()` assembly. (Note: rocks_compute doesn't have `get_height_world` -- that's fine, the shared file exposes all three and each consumer uses what it needs.)

- [x] 2d: **Shared cloud layer constants** -- Define the cloud layer parameters (altitude, scale, coverage, opacity, drift_mult for each of the 3 layers) and the drift vector computation as shared constants in common.wgsl (or a new `cloud_params.wgsl`). Refactor sky.wgsl's `fs_main` and common.wgsl's `cloud_shadow()` to reference these shared constants instead of duplicating magic numbers. This eliminates the sync risk where cloud shadows could drift out of alignment with visible clouds.

---

## Phase 3: Bake Cloud Shadows to Texture

The single highest-impact optimization. Currently, `cloud_shadow()` runs 3 `fbm3` calls (36 hash evaluations) per pixel in **every geometry fragment shader** (terrain, grass, rocks, trees, player, water) via `hemisphere_lighting()`. Baking to a texture replaces all those per-pixel noise calls with a single texture sample.

**Approach:** A small compute pass (e.g., 256x256 or 512x512) renders the cloud shadow map once per frame before the scene pass. The texture covers the world from above, sampling the same cloud layers with the same parameters. All geometry shaders then read from this texture instead of calling `cloud_shadow()`.

**Files:** New `game-render/src/cloud_shadow_compute.wgsl`, new `game-render/src/cloud_shadow.rs`, modifications to `game-render/src/common.wgsl`, `game-render/src/water.wgsl`, `game-render/src/frame.rs`, and each renderer's Rust file for bind group changes

- [x] 3a: **Create cloud shadow compute shader + Rust renderer** -- New compute shader that writes cloud shadow values to a 2D texture. Each texel maps to a world-space XZ position (camera-centered, covering the visible area). Compute the same `cloud_shadow_layer()` calls using the shared cloud constants from Phase 2d. Output: R8Unorm texture where 1.0 = fully lit, lower = shadowed.

- [x] 3b: **Integrate into render pipeline** -- Add the cloud shadow compute pass to `frame.rs` before the scene pass. Add the cloud shadow texture as a new binding accessible to geometry shaders (via an existing or new bind group). Refactor `hemisphere_lighting()` in common.wgsl to sample the cloud shadow texture instead of calling `cloud_shadow()`. Refactor water.wgsl's inline cloud shadow call similarly. Remove the now-unused `cloud_shadow()` and `cloud_shadow_layer()` functions from common.wgsl (the logic lives in the compute shader now).

---

## Phase 4: Reduce Shader Cost

Targeted reductions to per-pixel work that have minimal or no visual impact. These benefit all browsers but disproportionately help Safari.

**Files:** `game-render/src/water.wgsl`, `game-render/src/postprocess.wgsl`, `game-render/src/ssao.wgsl`, `game-render/src/sky.wgsl`

- [x] 4a: **Water shader** -- Reduce fragment noise cost. Currently 6 fbm3 for normals (2 layers x 3 finite-difference samples) + 2 fbm3 for foam = 8 fbm3 per pixel. Changes: (1) Drop the second ripple layer (ws2/wo2, lines 57-62) -- it contributes only 0.3 amplitude and is barely visible; fold a slight frequency variation into the primary layer instead. (2) Reduce foam from 2 fbm3 to 1 (single noise sample is sufficient for foam edge detection). Target: ~4 fbm3 per fragment (3 for normals + 1 for foam), down from 8.

- [x] 4b: **Postprocess god rays** -- Reduce from 20 to 12 steps. The per-pixel IGN jitter already breaks banding, so fewer steps produce visually identical results. Also add an early-out when the sun elevation is high (`angle_intensity < 0.01` check after line 118) to skip the loop entirely during midday -- saves the full 40 texture ops for roughly half the day cycle.

- [x] 4c: **Postprocess contact shadows** -- Reduce from 12 to 8 steps. The march distance is only 1.5 world units, so 8 steps at ~0.19 unit spacing still captures fine contact detail. Adjust the occlusion divisor accordingly.

- [x] 4d: **SSAO samples** -- Reduce from 12 to 8. The golden-angle spiral with IGN rotation is specifically designed for low sample counts, and the bilateral blur in postprocess smooths the result. 8 samples at half-resolution is well within quality thresholds.

- [ ] 4e: **Sky cloud self-shadow** -- Use `fbm2` (from Phase 2a) instead of `fbm3` for the self-shadow density sample in `sample_cloud_layer()`. The third octave contributes 0.125 amplitude at cloud scale (350-700 unit domains) -- sub-pixel detail that's invisible. This saves 1 value_noise (4 hashes) per cloud layer = 12 hashes per sky pixel.

---

## Phase 5: Render Scale for Safari/Mobile

Add the ability to render at a reduced internal resolution. This is the most impactful structural optimization for iOS (Retina iPads render at 4K+ natively). The FXAA pass already acts as the final blit and can upscale from lower resolution.

**Files:** `game-client/src/renderer.rs`, `game-client/src/lib.rs`, `game-render/src/postprocess.rs`, `game-render/src/ssao.rs`, `game-render/src/bloom.rs`

- [ ] 5a: **Add render_scale factor** -- Add a `render_scale: f32` (0.5-1.0) to the renderer. Internal textures (HDR intermediate, depth, AO, bloom) are created at `width * render_scale x height * render_scale`. The final FXAA output targets the full canvas size, performing the upscale. Default to 1.0 on desktop browsers, detect Safari/iOS via user agent (already in WASM land) and default to 0.75 or similar. This reduces total pixel work by ~44% at 0.75x scale.

- [ ] 5b: **Reduce bloom mip levels on low scale** -- When render_scale < 1.0, reduce `BLOOM_MIP_COUNT` from 5 to 4 (saves 2 compute pass transitions). The reduced resolution already shrinks the bloom source, so fewer mips are needed for the same visual radius.

---

## Success Criteria

- Water level is rock-solid stable regardless of camera position on Safari, iOS Safari, Chrome, and Firefox
- No visible quality regression on Chrome/Firefox (side-by-side with current build)
- Safari frame rate improves meaningfully (target: smooth 30+ fps on recent iOS devices)
- All shared shader code lives in include files with zero duplication of `linearize_depth`, `reconstruct_pos`, `get_height`, `compute_slope`, cloud parameters, or `TAU`
- `cargo build --target wasm32-unknown-unknown` compiles clean for game-client, game-server, and game-snapshot

## Open Questions

- **Cloud shadow texture resolution:** 256x256 covers the visible area at ~1.5m per texel (for a 384m world). Is that enough resolution, or does 512x512 look better? Test both.
- **Cloud shadow world extent:** Should it cover the full world or a camera-centered window? Camera-centered is cheaper but needs re-centering logic.
- **Render scale detection:** Is user-agent sniffing sufficient for Safari detection, or should we measure frame times and auto-adjust? Start simple (UA sniffing), iterate if needed.
- **Bloom mip count:** Verify that 4 mip levels still produce acceptable bloom radius at 0.75x scale.
