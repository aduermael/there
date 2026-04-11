# Rendering Push — Immersive Impressionism

**Date**: 2026-04-10
**Goal**: Push the renderer from "solid POC" to "beautiful and immersive." Each visual element should convey light, atmosphere, and mood — like an impressionist painting or RDR2's timeless approach. Every surface should feel intentional. Runs on higher-end mobile WebGPU (no need to support low-end).

**Iteration protocol**: Each phase allows up to 5 iterations. Each iteration runs `make snapshot`, then 3 critic sub-agents review the output (art direction, technical quality, comparative improvement). Feedback is synthesized and applied before the next iteration. Stop when critics agree the phase is shippable (7+/10).

**Snapshot archiving**: After each phase is complete (all tasks in the phase committed), copy the final snapshots into a phase-specific directory so the user can follow the visual evolution:
```
mkdir -p snapshots/2026-04-10-rendering-push/phase-N
cp snapshots/*.png snapshots/2026-04-10-rendering-push/phase-N/
```
Where N is the phase number (1, 2, 3…). Include this copy in the phase's final commit. The `snapshots/` root always holds the latest state; the plan subdirectory preserves history.

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

- [x] 2a: Increase shadow map resolution and improve sampling quality

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

- [x] 2b: Tune shadow contrast and atmospheric integration

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

- [x] 3a: Implement depth-aware bilateral blur for SSAO in postprocess

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

- [x] 4a: Redesign grass blade geometry for a softer, tuftier look

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

- [x] 4b: Improve grass distribution, color matching, and ground integration

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

- [x] 5a: Add size, shape, and color variety to tree generation

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

- [x] 6a: Add screen-space volumetric light scattering to postprocess

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

- [x] 6b: Final atmosphere and integration pass

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

---

## Phase 7: Color Depth & Soul

All four times of day look washed out — low contrast, low saturation, fog eats depth. Noon is the worst: flat desaturated terrain, pale sky, no punch. The scenes need more "soul" — richer colors, stronger contrast, each time of day should feel emotionally distinct. All four should be balanced in quality.

- [x] 7a: Rework atmosphere for deeper, richer colors across all times of day

  **Root causes**: (1) Fog density too high, especially at dawn/noon — pushes everything toward pale horizon color. (2) Noon sky zenith too dim, horizon too washed. (3) Ambient intensity too high, flattening contrast (sun vs shadow). (4) Night was over-brightened in Phase 6 tuning.

  **Contracts**:
  - Noon sun: cleaner, brighter — shift from warm-golden toward bright warm-white so greens pop
  - Noon sky zenith: deeper, more saturated blue (visible clear sky, not hazy white)
  - Noon horizon: less washed, more blue (currently near-white)
  - Fog density: reduce base by 30-40%, reduce dawn/dusk haze contribution
  - Ambient intensity: reduce — let direct sun create more contrast (shadows should be clearly darker)
  - Night: pull back zenith/horizon/ambient brightness ~20% from current — darker and moodier
  - Dawn/dusk: maintain warm atmosphere but with less haze washing out silhouettes

- [x] 7b: Rework postprocess color grading for punch and soul

  **Root causes**: (1) Exposure boost (1.08×) before ACES pushes values into ACES compression zone, losing saturation. (2) Saturation boost (1.18×) doesn't compensate enough. (3) S-curve contrast (0.25 blend) is too gentle — doesn't separate darks from brights. (4) Dark fill too aggressive.

  **Contracts**:
  - Remove or reduce exposure boost (1.08 → 1.0 or lower)
  - Increase saturation boost significantly (1.18 → 1.25-1.35)
  - Increase S-curve contrast (0.25 → 0.35-0.45) for more visual depth
  - Reduce dark fill to prevent purple wash on night geometry
  - Result: each time of day should have rich, saturated midtones with clear light/shadow separation
  - Must look good at ALL four times of day — not just noon

---

## Phase 8: Noon Polish & Night Moonlight

Noon still reads as flat with sandy-yellow terrain and navy rock shadows. Night is a monotone purple blanket with no depth separation. These are the last two weak links.

- [x] 8a: Fix noon — greener terrain, warmer rocks, better camera angle

  **Root causes**: (1) Noon camera overlooks low-elevation terrain that's still in the sand zone. (2) Rock shadow faces receive mostly blue sky ambient with insufficient warm bounce. (3) Sand albedo still too pale/yellow despite darkening.

  **Contracts**:
  - Move noon camera to show more interesting terrain (higher elevation = more grass)
  - Further lower sand-grass transition or make sand more olive/earthy so it blends with grass
  - Result: noon should have rich greens, warm earthy tones, blue sky — a vivid summer day

- [x] 8b: Fix night — elevated moonlight for depth and silhouette separation

  **Root causes**: (1) Night "moon" direction is near-horizontal +z, perpendicular to camera — most visible surfaces don't receive moonlight. (2) Ambient is uniform, creating a flat purple blanket.

  **Contracts**:
  - Elevate the moon direction at night so it's higher in the sky (illuminates upward-facing surfaces)
  - Smooth transition from sun to moon as day_factor drops (no discontinuity at twilight)
  - Result: rocks and terrain tops catch cool moonlight, creating silhouette separation against darker shadow sides

---

## Phase 9: Night Lighting Overhaul — Dark Blue, Not Green/Purple

**Problem**: Night grass is still visibly green — unrealistic. Overall palette leans purple when it should be cool desaturated blue-gray. Reference (RDR2 night): nearly monochrome dark blue, grass is dark silhouettes barely visible, moon casts cool silver-blue highlights on terrain tops. The scene should be **dark and mysterious**, not a purple-washed version of daytime.

**Root causes**: (1) Grass base color is baked green at scatter time — `hemisphere_lighting` multiplies green by blue-purple ambient, so green survives because ambient.g is nonzero. (2) Night sun color `[0.24, 0.32, 0.58]` is too purple — should be silver-blue. (3) Night ground ambient `[0.14, 0.12, 0.22]` leans purple (high blue, low green). (4) Postprocess dark fill `[0.018, 0.024, 0.055]` adds purple to dark pixels. (5) Saturation boost (1.28×) amplifies any remaining hue differences at night when everything should be nearly monochrome. (6) Night sky zenith/horizon are purple-tinted (`[0.12, 0.10, 0.35]` / `[0.10, 0.08, 0.24]`).

- [x] 9a: Rework night atmosphere for cold blue moonlight

  **Context**: `atmosphere.rs` lines 42 (night_sun), 52 (night_zenith), 64 (night_horizon), 88 (night_ground). All lean purple.

  **Contracts**:
  - Night sun (moonlight): shift from `[0.24, 0.32, 0.58]` toward cold silver-blue `[0.18, 0.22, 0.38]` — less intensity overall, cooler, no purple
  - Night sky zenith: shift from `[0.12, 0.10, 0.35]` toward dark navy `[0.04, 0.06, 0.18]` — much darker, less purple
  - Night sky horizon: shift from `[0.10, 0.08, 0.24]` toward `[0.03, 0.04, 0.12]` — very dark
  - Night ground ambient: shift from `[0.14, 0.12, 0.22]` toward cold blue-gray `[0.06, 0.07, 0.12]` — cooler, darker, no purple
  - Reduce overall ambient_intensity at night from 0.20 toward 0.12-0.15 range (much darker shadows)
  - Moon lift: keep elevated moon direction from Phase 8 — it provides good silhouette separation
  - Fog at night: reduce density (night fog is thinner in RDR2), shift fog color toward dark blue-gray
  - Must maintain smooth dawn↔night transition (no color pops)

  **Failure modes**: Too dark = unreadable. RDR2 reference shows that terrain is barely visible with moonlight hitting tops — dark but not pitch black. Key is silhouette separation via moonlight on upward surfaces vs very dark shadow sides.

- [x] 9b: Night-aware postprocess and grass desaturation

  **Context**: `postprocess.wgsl` lines 192-194 (dark fill), 188-190 (saturation boost). `grass.wgsl` lines 68-74 (base-to-tip gradient), 79-85 (translucency). Grass has no night-specific color correction.

  **Contracts**:
  - Postprocess dark fill: change from purple `[0.018, 0.024, 0.055]` to cold blue `[0.010, 0.014, 0.030]` — subtler, bluer, less purple
  - Night desaturation: when scene luminance is very low (night), reduce the saturation boost. Currently 1.28× always. At night, drop toward 0.8-1.0× (scotopic vision = less color perception). Gate on average scene luminance or pass a `day_factor` uniform to postprocess.
  - Grass shader: the `lift` variable (line 88) already gates on `sun_up` which is good. But translucency should be completely off at night (already gated). The tip warmth (+0.10 green, +0.07 red on tips) should fade at night — tips shouldn't glow warm under moonlight.
  - Consider passing `day_factor` as a new uniform field so both grass.wgsl and postprocess.wgsl can condition on time of day
  - Alternative: use existing `sun_dir.y` as a proxy for day/night in shaders (elevation < 0.1 = night)

  **Success criteria**: Night snapshot shows a dark, near-monochrome blue scene. Grass reads as dark shapes on dark terrain — barely any green visible. Moonlit terrain tops have cool silver highlights. Rocks are dark silhouettes with subtle top-lit edges. The mood is "quiet, mysterious night" like the RDR2 reference — not "purple disco."

---

## Phase 10: Noon Terrain & Grass Saturation — Richer Greens

**Problem**: The ground still appears too light/sandy in places at noon. The grass should be more vividly green (like the RDR2 daytime reference: warm golden-green grass, earthy brown paths, deep green trees). Dawn/dusk grass color is good — the issue is specifically noon.

**Root causes**: (1) Terrain sand albedo `[0.38, 0.42, 0.22]` is pale olive — needs to be darker/earthier. (2) Terrain grass albedo `[0.28, 0.52, 0.18]` could use more saturation. (3) Scatter `terrain_color_at` uses different sand values `[0.76, 0.70, 0.50]` (line 295) — these are much paler than the shader's sand, causing bright blades on darker terrain. (4) Sand-grass transition `smoothstep(4.0, 10.0, h)` means terrain below height 7 is still mostly sand — camera at noon likely shows mid-elevation terrain that's in this transition zone.

- [ ] 10a: Darken and enrich terrain colors (shader + scatter sync)

  **Context**: `terrain.wgsl` lines 63-65 (sand/grass/rock albedos), `scatter.rs` lines 295-297 (terrain_color_at sand/grass/rock) — these MUST stay in sync.

  **Contracts**:
  - Terrain grass albedo: boost green saturation — shift from `[0.28, 0.52, 0.18]` toward `[0.22, 0.56, 0.14]` (deeper, more saturated green)
  - Terrain sand: darken and make earthier — shift from `[0.38, 0.42, 0.22]` toward `[0.32, 0.34, 0.16]` (darker olive-brown, less pale)
  - **Critical**: `scatter.rs` `terrain_color_at()` lines 295-297 must use matching values. Currently sand is `[0.76, 0.70, 0.50]` in scatter (2× the shader value!) — this is a color mismatch bug. Align scatter sand/grass/rock to match shader values exactly.
  - Sand-grass transition: consider lowering from `smoothstep(4.0, 10.0, h)` to `smoothstep(3.0, 8.0, h)` in both shader and scatter — more grass coverage at lower elevations
  - Flat terrain boost (terrain.wgsl line 103): increase from 0.08 to 0.10-0.12 for greener flats
  - All changes must be mirrored between `terrain.wgsl` and `scatter.rs::terrain_color_at()`

  **Failure modes**: Over-saturating grass makes it look like AstroTurf. The RDR2 reference shows warm, natural greens — not neon. Sand becoming too dark removes the path/clearing contrast. Keep sand lighter than grass but earthier.

- [ ] 10b: Grass blade color enrichment

  **Context**: `grass.wgsl` lines 68-74 (base-to-tip gradient), `scatter.rs` lines 218-224 (blade color from terrain).

  **Contracts**:
  - Blade base: currently 90% of terrain color — keep
  - Blade tip: shift tip warmth from +0.10 green / +0.07 red toward +0.12 green / +0.04 red (more green, less warm at noon — warmer at dawn/dusk is fine, that's already good)
  - Gate tip warmth on sun elevation: at noon (high sun), tips should be bright green; at dawn/dusk (low sun), tips warm amber-gold (current behavior, which the user likes)
  - Per-blade color variation in scatter: increase from ±4% to ±6% for more natural variety
  - Consider a slight green boost for in-patch grass (denser patches = lusher green) vs sparse strays (slightly drier/yellower)
  - Result: noon grass should read as lush, saturated green meadow — "summer afternoon" like the RDR2 reference

  **Success criteria**: Noon snapshot shows rich green terrain with clearly differentiated grass/soil. Grass meadows are lush and green. Sandy paths between them are earthy brown (not pale). The impression is "warm summer day in a green valley" — not "sandy plains with green spots."

---

## Phase 11: Dense Grass Fields — BotW-Style Fluffy Volume

**Problem**: Grass looks sparse — individual blades are visible rather than forming a lush carpet. The BotW reference shows extremely dense, fluffy grass that reads as a continuous field with visible individual blades only up close. This is the biggest visual gap between our scene and a polished game.

**Technique**: The standard approach for dense grass in games is **massive GPU instancing with multi-blade tufts**. Each scatter point spawns 3-5 blades at slight offsets (a "tuft"), and the total instance count increases significantly. Performance stays manageable because: grass is already view-distance culled (50-80 unit fade), the vertex shader is simple, and each blade is only 4 triangles. WebGPU handles 48k-64k instances easily on modern hardware.

- [ ] 11a: Multi-blade tufts and increased density

  **Context**: `scatter.rs` lines 176-234 (grass placement), `grass.rs` line 5 (`MAX_GRASS = 24000`), lines 184-207 (blade geometry). Each scatter point currently places 1 blade.

  **Contracts**:
  - Increase `MAX_GRASS` from 24,000 to 64,000 (conservative for WebGPU; 64k instances × 6 verts × 4 tris = 256k tris, well within budget)
  - Multi-blade tufts: each scatter point that passes the acceptance check spawns 2-4 blades (tuft) at slight position offsets (0.1-0.3 unit radius). Each blade in the tuft gets a random rotation + slight height variation.
  - In-patch density: inside patch, spawn 3-4 blades per point. Outside patch (sparse strays): spawn 1-2.
  - Height variety within tuft: mix of shorter (0.5-0.7 scale) and taller (0.9-1.3 scale) blades for volume
  - Blade width: slightly narrower for dense fields — reduce base_hw from 0.16 to 0.12, mid_hw from 0.11 to 0.08, tip_hw from 0.03 to 0.02. More numerous thinner blades = fluffy rather than chunky.
  - Keep distance fade at 50-80 units (critical for performance)
  - Scatter grid step: may need to increase from 2 to 3 texels to compensate for multi-blade (same spatial density, more blades per point)
  - Instance buffer size must match new MAX_GRASS

  **Failure modes**: 64k instances may slow lower-end devices. If `make snapshot` shows frame time > 30ms, reduce to 48k. Multi-blade tufts with bad offsets → visible repeating patterns. Use per-blade hash for variety. Too narrow blades at distance = z-fighting shimmer — tune with critics.

- [ ] 11b: Grass LOD and ground coverage blending

  **Context**: `grass.wgsl` lines 36-38 (distance fade), `terrain.wgsl` lines 62-69 (ground color).

  **Contracts**:
  - Distance LOD: between 30-50 units, reduce blades per tuft (far tufts = 1-2 blades, close = 3-4). This can be done in scatter by encoding a priority byte, or simply by distance-based instance rejection.
  - Ground color integration: terrain beneath dense grass should be slightly greener (grass roots tint the soil). Add a subtle green shift to terrain.wgsl for areas within the grass height band (h 8-17) on flat terrain — so when grass fades at distance, the ground beneath "takes over" with a tint that matches.
  - Blade alpha: consider a soft alpha falloff at blade tips (bend_factor > 0.8) for softer silhouettes at distance — but only if performance allows (alpha blending requires sorting or OIT, may be too complex). Skip if too expensive.
  - Alternatively, use the existing tip-narrowing (tip_hw = 0.02) as a natural soft fade — no alpha needed.

  **Success criteria**: Close-up shots show dense, fluffy grass tufts with visible volume — blades overlap and create depth. Mid-range shows a continuous grassy meadow (individual blades merge into texture). Distance shows smooth terrain color taking over. The BotW reference's lush, carpet-like grass feel is achieved. Performance stays under 16ms per frame on target hardware (higher-end mobile WebGPU).
