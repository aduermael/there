# Rendering Push — Immersive Impressionism

**Date**: 2026-04-10
**Goal**: Push the renderer from "solid POC" to "beautiful and immersive." Each visual element should convey light, atmosphere, and mood — like an impressionist painting or RDR2's timeless approach. Every surface should feel intentional. Runs on higher-end mobile WebGPU (no need to support low-end).

**Iteration protocol**: Each phase allows up to 5 iterations. Each iteration runs `make snapshot`, then 3 critic sub-agents review the output (art direction, technical quality, comparative improvement). Feedback is synthesized and applied before the next iteration. Stop when critics agree the phase is shippable (7+/10).

---

## Current State (Phase 15 baseline)

The renderer has clouds, hemisphere lighting, procedural terrain color, patchy grass, multi-layered trees, exponential fog, SSAO with colored warm tinting, ACES tone mapping, shadow mapping, and vignette. The bones are good. What's missing is **polish depth** — the rendering reads as "technically complete" rather than "emotionally immersive."

### Specific Issues (from user feedback)

| Problem | Root Cause |
|---------|-----------|
| Noon is washed out / low saturation | ACES tone mapping + weak saturation boost (1.08x) desaturates highlights. Noon sun color is too neutral. |
| Shadows barely visible | 512x512 shadow map over 200x200 units = ~2.5 px/unit. Single-sample comparison. Shadow floor at 30% sun. Combined with strong ambient, shadows lack contrast. |
| SSAO still "dirty" | Non-bilateral blur bleeds AO across depth edges. Half-res staircase at rock silhouettes. |
| Grass not fluffy / doesn't match ground | Thin rectangular blades (0.08 width). Color computed at scatter time doesn't account for terrain noise. No softness or tuft feel. |
| Trees all same size | Scatter uses uniform 1.0-2.0 scale range. No shape variation in crown. All three foliage cones identical across instances. |
| No visible god rays | No volumetric light scattering implemented. |
| Grass sometimes darker than ground | Grass blade base color (from scatter.rs) uses simplified height-based terrain color that doesn't match terrain.wgsl's per-pixel noise variations. |

### Key Files

| File | Role |
|------|------|
| `game-render/src/atmosphere.rs` | Time-of-day sun, sky, fog parameters |
| `game-render/src/postprocess.wgsl` | Tone mapping, color grading, SSAO blur, vignette |
| `game-render/src/postprocess.rs` | Postprocess pipeline setup, bind groups |
| `game-render/src/common.wgsl` | Shared uniforms, lighting, fog, shadow sampling |
| `game-render/src/ssao.wgsl` | SSAO hemisphere sampling |
| `game-render/src/ssao.rs` | SSAO pipeline, half-res AO texture |
| `game-render/src/shadow.rs` | Shadow map (512x512), orthographic sun projection |
| `game-render/src/grass.wgsl` | Blade vertex + fragment shader |
| `game-render/src/grass.rs` | Blade geometry (4-vertex quad), instancing (16384 max) |
| `game-render/src/trees.wgsl` | Tree vertex + fragment, wind sway |
| `game-render/src/trees.rs` | Tree mesh (3-cone + cylinder), instancing (1536 max) |
| `game-render/src/scatter.rs` | Deterministic placement of rocks, trees, grass |
| `game-render/src/terrain.wgsl` | Height-based coloring, slope detail, lighting |
| `game-render/src/sky.wgsl` | Sky gradient, sun disc/glow, procedural clouds |
| `game-client/src/renderer.rs` | Main render loop (4 passes: shadow, scene, SSAO, postprocess) |
| `game-snapshot/src/render.rs` | Headless snapshot renderer (same 4 passes) |

### Current Rendering Pipeline

```
Pass 0: Shadow Depth (512x512, sun POV)
  -> terrain (LOD1), rocks, trees

Pass 1: Scene -> HDR Intermediate (Rgba16Float)
  -> sky, terrain, grass, rocks, trees, players
  -> hemisphere_lighting + shadow_sampling + rim_light + fog

Pass 2: SSAO -> Half-res AO (R8Unorm)
  -> 12 samples, IGN noise, TBN hemisphere, RADIUS=3.0, STRENGTH=5.5

Pass 3: Postprocess -> Final Output (Rgba8UnormSrgb)
  -> 9-tap AO blur + colored AO + ACES + grading + vignette
```

### Uniforms Struct (shared by all geometry + SSAO shaders)

All time-of-day parameters are computed in `atmosphere.rs` and flow through this struct. Adding fields requires updating `terrain.rs` (struct definition), `common.wgsl` (WGSL mirror), `ssao.wgsl` (independent copy), and both renderers.

---

## Phase 1: Color Vibrancy & Noon Exposure

Noon looks washed out. The overall palette across all times of day lacks the rich, saturated quality of impressionist paintings. This is the foundation — every subsequent phase looks better on a vibrant base.

- [x] 1a: Rework atmosphere colors for vibrancy across all times of day

  **Context**: `atmosphere.rs` (lines 33-61) defines sun_color, sky_zenith, sky_horizon, fog_color, sky_ambient, and ground_ambient as time-of-day interpolated colors. Noon sun is `[0.85, 0.82, 0.76]` (neutral), sky_zenith noon is `[0.40, 0.60, 0.90]`.

  **Goal**: Noon should feel like a warm, vivid summer day — rich greens, warm golden sunlight, clear blue sky. Dawn/dusk should be dramatic with deep oranges and purples. Night should be moody with deep blues but readable.

  **Contracts**:
  - Noon sun: warmer, more golden (shift toward amber, increase intensity differential)
  - Noon sky: deeper, more saturated blue
  - Dawn/dusk: more dramatic orange-to-purple gradient
  - Ground ambient: warmer earth tones (impacts shadow color everywhere)
  - All changes must maintain smooth time-of-day lerping (no color pops between presets)
  - Fog color must harmonize with sky (it's the "atmospheric perspective" color)

  **Failure modes**: Over-saturating noon makes it look like a mobile game ad. The goal is "oil painting vibrancy" not "candy." Test with all 4 snapshots.

- [x] 1b: Adjust postprocess color grading for richer output

  **Context**: `postprocess.wgsl` (lines 57-69) applies ACES, then warm shadow shift (+0.02 red), saturation boost (1.08x), S-curve contrast (0.3 blend), and vignette (0.3).

  **Goal**: Colors should "pop" without looking garish. Midtones should be rich and saturated. Shadows should be warm and deep, not muddy. Highlights should have character (not just white).

  **Contracts**:
  - Saturation boost: increase from 1.08 toward 1.15-1.20 range (tune with critics)
  - S-curve contrast: may need adjustment to avoid crushing dark greens
  - Shadow warmth: currently +0.02 red — may need more color shift for painterly depth
  - ACES parameters: standard, but consider pre-multiplying scene color to control exposure
  - Must look good at ALL four times of day (not just noon)

  **Success criteria**: Noon terrain shows rich, differentiated greens. Dawn has warm golden hues. Dusk has deep amber/rose. Night has readable blue-purple.

---

## Phase 2: Shadow Quality & Visibility

Shadows provide critical depth cues and grounding. Currently they're barely visible — the 512x512 map over 200 units gives ~2.5 px/unit resolution, and the 30% shadow floor is too generous when combined with hemisphere ambient.

- [ ] 2a: Increase shadow map resolution and improve sampling quality

  **Context**: `shadow.rs` (line 3) defines `SHADOW_MAP_SIZE = 512`. The shadow pass renders terrain (LOD1) + rocks + trees from an orthographic sun projection covering 200x200 units (lines 80-100). `common.wgsl` `sample_shadow()` uses a single comparison sample with bias 0.005.

  **Goal**: Shadows should be clearly visible as dark patches on the ground behind rocks and trees. Rock and tree shadows should have recognizable silhouettes, not just blurry blobs.

  **Contracts**:
  - Increase shadow map to 1024x1024 (4x texel density, still lightweight)
  - Add PCF (Percentage Closer Filtering) — sample shadow map at 4-9 neighboring texels, average the results. This gives soft, anti-aliased shadow edges that fit the painterly aesthetic.
  - Reduce shadow coverage area if needed to increase texel density near camera (e.g., 150x150 instead of 200x200)
  - Shadow pass must still skip grass and players (too thin / too dynamic)
  - Update shadow bias for new resolution (higher res = smaller bias needed)
  - Verify shadow pipeline works in both snapshot and client renderers

  **Failure modes**: PCF adds 4-9 texture samples per fragment per geometry shader. On mobile this is notable — benchmark. If too expensive, fall back to 4-tap PCF. High shadow map res increases VRAM (1024x1024 x 4 bytes = 4MB, acceptable).

- [ ] 2b: Tune shadow contrast and atmospheric integration

  **Context**: `common.wgsl` `hemisphere_lighting()` (line 86) applies shadow via `mix(0.3, 1.0, shadow)` — shadowed areas keep 30% of sun contribution. Combined with hemisphere ambient (sky_ambient + ground_ambient), shadowed areas still receive substantial light.

  **Goal**: Shadows should be clearly darker than lit areas. The contrast should be enough to read tree shadows on terrain, rock shadows on grass. But shadows shouldn't be black — they should be colored (cool blue from sky ambient, warm from ground bounce).

  **Contracts**:
  - Reduce shadow floor from 0.3 to a lower value (0.1-0.2 range — tune with critics)
  - Shadow color should come from hemisphere ambient (already works — ambient is unaffected by shadow, only direct sun is)
  - Time-of-day variation: noon shadows are crisp and cool-blue. Dawn/dusk shadows are warm and long. Night has no shadow pass.
  - Shadow edges should feel soft and painterly (PCF + low resolution = natural softness)

  **Success criteria**: In noon snapshot, tree shadows are clearly visible as blue-tinted dark patches on green terrain. Rock shadows visible on ground. Shadows have recognizable shape but soft edges.

---

## Phase 3: SSAO Final Polish

SSAO is "much better but still a bit dirty." The remaining issues are: blur bleeds across depth edges (rock silhouettes against sky get haloing), and some flat surfaces show faint noise patterns.

- [ ] 3a: Implement depth-aware bilateral blur for SSAO in postprocess

  **Context**: `postprocess.wgsl` (lines 34-50) does a 9-tap Gaussian blur on the half-res AO texture at 1.5 texel spread. This blur doesn't know about depth edges — it averages AO values from the rock and from the sky behind it, creating visible haloing at silhouettes.

  **Goal**: Blur should be wide and soft within continuous surfaces but stop at depth discontinuities (e.g., rock edge vs. sky). This eliminates haloing while keeping the painterly softness.

  **Approach**: Bind the depth texture to the postprocess pass (requires adding it to the bind group in postprocess.rs). For each blur tap, compare the depth at that tap with the center depth. If the depth difference exceeds a threshold, reduce the tap's weight. This is a bilateral blur — smooth within surfaces, sharp across depth edges.

  **Contracts**:
  - New bind group entry in `postprocess.rs`: depth texture (Depth2D, non-comparison sampling)
  - Postprocess shader reads depth at center pixel + each blur tap
  - Weight reduction: if `|depth_tap - depth_center| > threshold`, weight → 0 (or near-zero)
  - Threshold should be tuned relative to the scene scale (~0.01-0.05 in normalized depth)
  - Must not break existing postprocess pipeline (HDR + AO + depth all bound)
  - Performance: adds 9 depth texture reads per fragment (half the cost of a new pass)

  **Failure modes**: If depth threshold is too tight, the blur becomes no-op and noise returns. If too loose, it's the same as the current non-bilateral blur. The depth texture is full-res while AO is half-res — need to map UV correctly when reading depth.

  **Success criteria**: Rock silhouettes against sky show no dark AO fringe. Flat terrain is clean. Contact shadows at rock bases are preserved. No visible per-pixel noise.

---

## Phase 4: BotW-Style Grass

The current grass is thin rectangular blades that don't integrate well with the ground. The user wants "fluffy, patchy, not everywhere" — like Breath of the Wild's iconic grass fields that sway in the wind as soft, luminous tufts.

- [ ] 4a: Redesign grass blade geometry for a softer, tuftier look

  **Context**: `grass.rs` (lines 183-198) generates a 4-vertex quad: base width 0.08, tip width 0.04, height 0.6. Each blade is a flat rectangle with wind bend. The grass.wgsl fragment applies hemisphere lighting + fog.

  **Goal**: Blades should be wider, softer, more like little bushels of grass. Each "blade" should read as a tuft or clump, not a thin line. The overall impression at distance should be a soft, luminous meadow — not individual sticks poking up.

  **Contracts**:
  - Wider blades: base width ~0.15-0.25, tip width ~0.08-0.12
  - Slightly shorter or same height: 0.4-0.6 units
  - Consider adding a slight curve (extra vertex pair at mid-height with slight offset) for a natural bend shape
  - Blade count may decrease if they're wider (fewer blades needed to cover same area)
  - Both sides rendered (no face culling, as currently)
  - Wind animation preserved — wider blades sway more dramatically
  - Fragment: softer gradient from dark base to bright saturated tip

  **Open question**: Whether to use a curved 6-vertex blade (3 segments) or keep 4-vertex quads. 6-vertex gives nicer wind deformation but doubles triangle count. Curved may be needed for the "fluffy" feel — decide during implementation based on visual result.

- [ ] 4b: Improve grass distribution, color matching, and ground integration

  **Context**: `scatter.rs` (lines 105-187) places grass in 3 passes: patch detection (45% of 10-texel grid), dense fill within patches (85%), sparse fill outside (12%), and rock rings. Blade color (lines 246-262) is computed from height-based terrain zones with ±7.5% variation.

  **Goal**: Grass should appear in natural meadow-like patches with clear bare terrain between them. Blade colors should closely match the terrain at their location — no sudden dark blades on light terrain. The transition from grass to bare ground should feel organic.

  **Contracts**:
  - Patch system: fewer but denser patches for clearer "meadow clusters" effect
  - Bare ground between patches should be clearly visible (terrain noise from Phase 3 carries visual weight there)
  - Blade base color must match the terrain.wgsl fragment logic MORE closely — consider sampling the same noise functions at scatter time, or at minimum using the same height-based color bands with matching variation
  - Blade tip color: brighter and more saturated than base (sunlit tops, shaded roots)
  - Grass around rocks: should blend with rock-adjacent terrain color, not create dark patches
  - Max instances: can increase from 16384 to 24000+ if needed for wider blade coverage at lower density
  - Distance fade (50-80 units) preserved

  **Failure modes**: Over-patching creates a "spotty" look. Under-patching returns to uniform carpet. Color mismatch is the #1 issue — if blade color is close to terrain, integration happens naturally even with imperfect distribution.

  **Success criteria**: Meadow areas have visible grass tufts with bare earth between. Grass colors blend seamlessly with terrain beneath. The overall impression is "impressionist field" not "green carpet" or "scattered sticks."

---

## Phase 5: Tree Variety & Forest Character

All trees currently use the same mesh with uniform 1.0-2.0 scale. This creates a "tree factory" look. Real forests have character — large anchor trees, small saplings, bushy specimens, tall narrow ones.

- [ ] 5a: Add size, shape, and color variety to tree generation

  **Context**: `trees.rs` (lines 280-359) builds a single mesh: cylinder trunk + 3 stacked cones. All instances use this same mesh at uniform scale. `scatter.rs` (lines 64-103) places trees at 6-texel grid, height 10-17, 35% acceptance, scale 1.0-2.0 with green color variation.

  **Goal**: Forest should have visible variety — a mix of tall mature trees, medium specimens, small saplings. The foliage should vary in width and density. Some trees should be distinctly larger, creating "anchor" points in the landscape. Tree clusters should have natural grouping — not evenly spaced.

  **Contracts**:
  - Scale range: widen from 1.0-2.0 to 0.6-3.0 (small saplings to large canopy trees)
  - Crown shape variation: per-instance variation in foliage cone radius/height ratios — wider/bushier vs. tall/narrow. This can be achieved by varying the foliage_color.a (currently unused) to encode a shape factor, then adjusting vertex positions in the vertex shader.
  - Color variation: wider green range — some trees deeper/bluer green, some more yellow-green (seasonal variation within same species)
  - Clustering: occasional tight groups of 3-5 trees at close spacing (modify scatter logic to create mini-clusters)
  - Size distribution: more small trees than large (exponential distribution, not uniform)
  - Instance cap: may need increase from 1536 to 2048+ for added small trees
  - Shadow pipeline must render all trees (still using same shadow vertex shader)

  **Failure modes**: Too much size variation looks chaotic. The variety should feel natural — a few large landmark trees, many medium, scattered small. Color variation too extreme → carnival. Keep all greens within a natural range.

  **Success criteria**: Forest skyline shows varied heights. Individual trees have distinct sizes. Close-up clusters feel like a natural forest edge. The overall impression is "ancient forest" not "tree plantation."

---

## Phase 6: God Rays & Atmospheric Light

Volumetric light scattering ("god rays") is the final atmosphere layer. Subtle shafts of light radiating from the sun through clouds and around objects create the single most immersive lighting effect in outdoor scenes.

- [ ] 6a: Add screen-space volumetric light scattering to postprocess

  **Context**: `postprocess.wgsl` currently reads HDR scene + AO. The sun position is available in uniforms (`sun_dir`). The sky shader renders clouds that partially occlude the sun.

  **Goal**: Subtle light shafts radiating from the sun position, visible especially at dawn/dusk when the sun is near the horizon and objects create clear silhouettes. Should feel atmospheric and dreamlike — not over-the-top "church window" rays.

  **Approach**: Screen-space radial blur technique:
  1. Compute sun screen position from `sun_dir` via `view_proj`
  2. For each pixel, march toward the sun position in screen space (6-12 steps)
  3. Accumulate scene luminance along the march — bright pixels near the sun contribute rays
  4. Blend the accumulated rays with the scene as an additive layer
  5. Rays are tinted by sun_color for time-of-day integration

  **Contracts**:
  - New uniforms needed: `sun_screen_pos` (vec2, computed per-frame from sun_dir + view_proj) — or compute it in the shader from existing uniforms
  - Ray intensity scales with sun elevation (strong at low angles, subtle at noon, off at night)
  - Rays should be visible through/around clouds (clouds partially block = partial rays)
  - Total cost: 6-12 texture samples per pixel along the radial direction. At full-res this is notable — consider rendering at half-res and upscaling, or sampling the half-res AO buffer (which encodes scene depth occlusion)
  - Effect must be subtle and tunable — a multiplier constant controls overall ray strength
  - Must work in both client and snapshot renderers

  **Failure modes**: Too many march steps → slow on mobile. Too few → banding. Radial blur at full-res with 12 steps = 12 extra HDR samples per pixel — consider using a lower-res intermediate. Sun behind camera → no rays (handle gracefully). Over-bright rays at dawn = blinding — needs intensity cap.

  **Success criteria**: Dawn snapshot shows subtle warm light shafts radiating from sun. Dusk has amber rays through tree silhouettes. Noon rays are minimal (sun overhead, less dramatic). Night has no rays. Effect is subtle and atmospheric — "you notice the mood, not the technique."

- [ ] 6b: Final atmosphere and integration pass

  **Goal**: With all rendering systems in place (color, shadows, SSAO, grass, trees, god rays), do a final tuning pass to harmonize everything. Each time of day should be a cohesive, immersive scene.

  **Contracts**:
  - Dawn: golden-pink atmosphere, warm shadows, gentle god rays, dew-like freshness
  - Noon: vivid greens and blues, crisp shadows, clear sky, saturated but not garish
  - Dusk: deep amber/rose, long dramatic shadows, prominent god rays, nostalgic mood
  - Night: deep blue-purple, subtle moonlight fill, readable but mysterious
  - All transitions between times of day should be smooth (colors lerp naturally)
  - Final snapshots should be "screenshot-worthy" — something you'd pause and admire
  - Run the full 3-critic iteration loop one final time across all 4 shots

  **Success criteria**: Each of the 4 time-of-day snapshots could work as a game promotional screenshot. The overall mood conveys "timeless, immersive world" — not "tech demo" or "student project."
