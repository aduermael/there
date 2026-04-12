# Bugfixes & Polish — 2026-04-12

Four issues to fix, ordered from quick wins (unblock dev) through rendering corrections to gameplay.

**Verification approach:** Use `game-snapshot` to capture frames before/after each rendering change. At each phase, spawn 3 critic/reviewer sub-agents (up to 5 rounds) to audit the fix from different angles: correctness, side effects, and code quality.

---

## Phase 1: Dead code removal & server error handling

Unblocks development by eliminating warnings and the server panic.

**Codebase context:**
- `game-render/src/grass.rs:16-36` — `GrassRenderer` struct has two dead fields: `index_count` (line 30) and `instance_buffer` (line 33). Both are set in the constructor (lines 266-267) but never read. Rendering uses `indirect_buffer` for draw args and `render_instance_bg` bind group for instance data — the stored fields are leftovers from a CPU-driven approach.
- `game-client/src/renderer.rs:261-276` — `resize()` method is never called. No resize handler exists in the web layer either. All sub-component `.resize()` methods it calls are only called from here — the entire resize chain is dead code.
- `game-server/src/main.rs:45` — `.unwrap()` on `TcpListener::bind()` produces an opaque panic. Should print a clear message ("port already in use") and exit cleanly.

**Success criteria:**
- `make dev` produces zero warnings
- Server prints a human-readable error and exits with code 1 when port is taken
- No unused fields, methods, or dead resize chains remain

- [x] 1a: Remove `index_count` and `instance_buffer` from `GrassRenderer` struct and constructor. Also remove unused sub-component `.resize()` methods that only existed to support `Renderer::resize()` — trace and remove the entire dead chain (check `water.resize()`, `ssao.resize()`, `bloom.resize()`, `exposure.resize()`, `postprocess.resize()`, `fxaa.resize()` for any callers besides `Renderer::resize()`). Remove `Renderer::resize()` itself.
- [x] 1b: Replace `.unwrap()` on `TcpListener::bind()` with `.unwrap_or_else()` that logs the error clearly and calls `std::process::exit(1)`.

---

## Phase 2: Fix view-dependent lighting (camera angle affects brightness)

**Root cause:** `rim_light()` in `common.wgsl:120-124` computes a Fresnel term from `camera_pos → fragment` view direction. This term is added to the lit color in terrain (`terrain.wgsl:121-122`), rocks (`rocks.wgsl:78-79`), and trees (`trees.wgsl:120-121`). When camera pitch changes, the dot product `dot(normal, view_dir)` changes, causing surfaces to appear darker/lighter.

Note: The player shader (`player.wgsl:78-79`) does NOT use `rim_light` — another reason the effect is inconsistent.

**Intentional view-dependent effects (DO NOT touch):**
- Fog sun-haze (`common.wgsl:136-139`) — artistic, not a bug
- Grass translucency backlight (`grass.wgsl:89-95`) — intentional
- Water Fresnel (`water.wgsl`) — physically correct
- God rays & contact shadows (`postprocess.wgsl`) — screen-space effects

**Success criteria:**
- Snapshots from same position but different pitch angles produce ground/rock/tree lighting that is visually identical (brightness should not shift with pitch)
- No visual regression in overall scene mood at various sun angles

- [x] 2a: Remove the `rim_light` function from `common.wgsl` and remove all call sites: `terrain.wgsl:121-122` (the `+ rim` addition), `rocks.wgsl:78-79`, `trees.wgsl:120-121`. Simplify each fragment shader to pass `lit` directly to `apply_fog()`. Take before/after snapshots at noon sun angle from 2+ camera pitches to confirm lighting stability.

---

## Phase 3: SSAO / contact shadow blending near avatar

**Problem:** The shadow/AO around the player character appears harsh and doesn't blend naturally with surrounding elements.

**Contributing factors:**
- SSAO parameters in `ssao.wgsl:9-13`: RADIUS=3.0 and STRENGTH=5.5 are tuned for large terrain features. On thin player geometry (limbs, joints), this creates exaggerated dark halos.
- Contact shadows in `postprocess.wgsl:24-80`: 12-step march with tight depth range (0.0002–0.01) creates hard shadows under player hands/feet.
- Bilateral blur depth threshold (`postprocess.wgsl:183`): 0.002 is tight enough that AO from the ground can bleed onto the character silhouette edge.

**Approach:** Tune SSAO radius/strength to be less aggressive, widen bilateral blur depth threshold slightly, and soften contact shadow contribution. These are parameter adjustments — no structural changes needed.

**Success criteria:**
- Avatar shadow/AO blends smoothly with environment
- Terrain/rock AO still provides depth and definition
- No visible AO halos around player limbs

- [x] 3a: Tune SSAO and contact shadow parameters. Reduce SSAO STRENGTH (try ~3.0–4.0), slightly reduce RADIUS (try ~2.0), widen bilateral blur `depth_threshold` (try ~0.005). Soften contact shadow final multiplier (line 79: try `0.4` instead of `0.6`). Take snapshots with player visible to verify blending improvement without losing environmental depth.

---

## Phase 4: Character rotation toward movement direction

**Problem:** When pressing forward, the player character doesn't rotate to face the movement direction (camera behind character). Currently, `camera.yaw` is sent directly as the player's facing yaw (`lib.rs:483`, `game_loop.rs:40`), so the character always faces wherever the camera points — even when strafing.

**Desired behavior:** Character should smoothly rotate to face the direction of movement (derived from forward/strafe inputs relative to camera yaw). When standing still, character retains last movement facing. Camera continues to orbit independently.

**Key files:**
- `game-client/src/lib.rs:477-489` — sends `camera.yaw` as the input yaw
- `game-server/src/game_loop.rs:40` — assigns `player.yaw = player.input_yaw`
- `game-core/src/movement.rs:19-24` — computes move direction from yaw + forward/strafe
- `game-client/src/lib.rs:206-217` — builds local player instance with `camera.yaw`

**Contracts:**
- Client computes `move_yaw` from `atan2(move_x, move_z)` of the intended movement vector (forward/strafe rotated by camera yaw). Sends both `camera_yaw` (for movement calc on server) and `move_yaw` (for facing).
- Server uses `camera_yaw` for movement physics (unchanged), applies `move_yaw` as `player.yaw` only when there is movement input (forward != 0 or strafe != 0). When idle, retains last yaw.
- Client uses `move_yaw` for local player rendering instead of `camera.yaw`.
- Smooth rotation: interpolate current yaw toward target `move_yaw` using a lerp or shortest-arc approach each tick (server-side).

**Failure modes:**
- Sending extra field changes protocol — but we don't care about backward compat.
- `atan2(0,0)` when idle — guard: only update facing when input magnitude > 0.
- Wraparound at ±π — use shortest-arc angle interpolation.

**Success criteria:**
- Pressing forward rotates character to face away from camera
- Strafing rotates character to face strafe direction
- Standing still keeps last facing direction
- Remote players also rotate correctly (server sends correct yaw in snapshots)

- [x] 4a: Compute `move_yaw` on the client from forward/strafe + camera_yaw. Add `move_yaw` to the input message sent to server. Update protocol structs in both client and server.
- [x] 4b: Server applies `move_yaw` as `player.yaw` only when movement input is nonzero. Add smooth yaw interpolation (shortest-arc lerp toward `move_yaw` each tick). Keep `camera_yaw` / `input_yaw` for movement physics unchanged.
- [x] 4c: Client uses computed `move_yaw` for local player instance rendering (`lib.rs:208-215`) instead of `camera.yaw`. Verify remote players also display correct rotation from server state.

---

## Phase 5: Fix character rotation bugs

Phase 4 introduced two bugs that prevent correct character facing.

**Bug 1 — `atan2` sign error (character faces wrong direction):**
`game-client/src/lib.rs:506` computes `move_x.atan2(move_z)`. But the player model faces direction `(-sin(yaw), -cos(yaw))` at a given yaw (see `player.wgsl:49-57`). When pressing forward at camera yaw=0, the movement vector is `(0, -1)`, so `atan2(0, -1) = PI` — rotating the model 180° to face +Z (toward camera) instead of -Z (away from camera). The correct formula is `(-move_x).atan2(-move_z)`, which for the same case gives `atan2(0, 1) = 0`, matching camera yaw.

**Bug 2 — `local_move_yaw` only updates at 20Hz (rotation lags camera):**
`local_move_yaw` is computed inside the `if now - state.last_send_time >= 50.0` send block (`lib.rs:492-514`). Between sends, the visual yaw target is stale. When the camera rotates while walking, `local_visual_yaw` chases a stale target for up to 50ms, making rotation feel unresponsive.

**Key files:**
- `game-client/src/lib.rs:500-510` — move_yaw computation (inside send block)
- `game-client/src/lib.rs:207-219` — `build_player_instances()` with visual yaw interpolation

**Fix:**
- Move `local_move_yaw` computation before `build_player_instances()`, running every frame from `state.input.forward()`, `state.input.strafe()`, and `state.camera.yaw`. The send block just reads the already-computed `state.local_move_yaw`.
- Fix the atan2: use `(-move_x).atan2(-move_z)`.
- Apply the same atan2 fix on the server side (`game-server/src/game_loop.rs`) if the server ever needs to recompute — but currently the server just uses the client-sent `move_yaw`, so only the client formula matters.

**Success criteria:**
- Pressing forward: character faces away from camera (back visible)
- Rotating camera while holding forward: character smoothly tracks new forward direction every frame
- Strafing left: character faces left relative to camera

- [x] 5a: Move `local_move_yaw` computation out of the send block into the main frame loop (before `build_player_instances`). Fix the atan2 sign to `(-move_x).atan2(-move_z)`. Send block reads `state.local_move_yaw` instead of recomputing.

---

## Phase 6: Fix avatar mesh winding order (see-through body parts)

**Problem:** Some faces of the player avatar are invisible — you can see through parts of the body.

**Root cause:** The mesh generation functions in `game-render/src/player.rs` produce inconsistent or reversed triangle winding, and the player pipeline uses `cull_mode: Some(wgpu::Face::Back)` (`player.rs:154`). With WebGPU's default CCW front-face convention, back-facing (CW) triangles are culled.

**Winding analysis — `add_box()` (line 370):**
All six faces use index pattern `[i, i+1, i+2, i, i+2, i+3]`. Testing the +Y face: vertices go `(+hx,+hz)→(-hx,+hz)→(-hx,-hz)→(+hx,-hz)` in XZ. The cross product of edge1×edge2 for triangle (0,1,2) points in -Y, but the intended outward normal is +Y. **All box faces have reversed winding** — the cross product consistently points inward.

**Winding analysis — `add_cylinder()` bottom cap (lines 455-460):**
Uses `[center, j, j+1]` (same rotation sense as vertices), while the top cap (lines 430-435) uses `[center, j+1, j]` (reversed). Since the top cap normal is +Y and bottom is -Y, they need opposite winding to both face outward — but the current implementation gives them the same effective winding. **Bottom cap winding is wrong.**

**Winding analysis — `add_ellipsoid()` (line 507):**
Uses `[a, b, a+1, a+1, b, b+1]`. Depending on the hemisphere, outward normals flip. Needs verification — likely correct for one hemisphere, wrong for the other.

**Fix approach:** Reverse the index winding in `add_box()` from `[i, i+1, i+2, i, i+2, i+3]` to `[i, i+2, i+1, i, i+3, i+2]`. Fix the cylinder bottom cap to match the top cap's outward convention. Verify ellipsoid winding and fix if needed. Alternative: the simplest correct fix may be to disable culling (`cull_mode: None`) since the avatar is small on screen and the performance cost is negligible, but fixing winding is more correct.

**Success criteria:**
- No see-through holes on any part of the avatar from any viewing angle
- All body parts (torso, head, arms, legs) fully opaque
- Snapshot verification from multiple angles

- [x] 6a: Fix `add_box()` index winding: reverse to `[i, i+2, i+1, i, i+3, i+2]`. Fix `add_cylinder()` side winding (caps were already correct). Fix `add_ellipsoid()` winding — reversed its two triangles. Also fix +Z/-Z box face vertex ordering (v1/v3 swap) — the initial index reversal only fixed 4 of 6 faces because vertex orderings were inconsistent across faces.

---

## Phase 7: Fix idle animation arm jitter

**Problem:** The avatar moves its arms weirdly sometimes, even when standing still.

**Root cause (3 contributing factors):**

1. **Velocity threshold too sensitive** — `AnimState::from_movement()` (`animation.rs:110`) uses `speed > 0.3` for Walk. The local player's speed is derived every frame from position deltas (`lib.rs:222`: `(local_pos - prev_local_pos) / dt`). Tiny floating-point drift, terrain height adjustments, or server reconciliation nudges produce speeds that flutter around 0.3, causing rapid Idle↔Walk state flipping.

2. **Idle clip missing lower arm tracks** — `idle_clip()` (`clips.rs:61-96`) defines `UPPER_ARM_L/R` but NOT `LOWER_ARM_L/R`. These default to identity quaternion (from `empty_tracks()`). The walk clip defines `LOWER_ARM_L/R` with rotations from `-0.1` to `-0.4` radians. When blending Idle→Walk, the lower arms interpolate between identity (straight) and bent — creating visible twitching.

3. **Rapid crossfade restarts** — `set_state()` (`animation.rs:182-189`) triggers a 0.2s crossfade on every state change. When the state flips Idle→Walk→Idle every few frames, the blend never completes, producing continuous partial arm rotations.

**Fix:**
- Add hysteresis to `from_movement()`: use threshold 0.5 to enter Walk, 0.15 to exit back to Idle. This requires tracking the current state (or passing it in).
- Add `LOWER_ARM_L/R` identity tracks to `idle_clip()` so blending with walk doesn't produce visible arm jumps.
- The hysteresis is the primary fix; the idle tracks are defense-in-depth.

**Success criteria:**
- Standing still on terrain: avatar arms are completely still (no twitching)
- Starting to walk: clean transition to arm swing
- Stopping: clean transition back to idle arms

- [x] 7a: Add explicit `LOWER_ARM_L` and `LOWER_ARM_R` tracks to `idle_clip()` in `clips.rs`, using the resting pose values (e.g. `rx(-0.15)` constant, matching the walk midpoint so blends are smooth).
- [x] 7b: Add hysteresis to animation state selection. In `lib.rs` where `AnimState::from_movement()` is called for the local player, only transition from Idle→Walk when `horiz_speed > 0.5` and from Walk→Idle when `horiz_speed < 0.15`. Keep the server-side thresholds as-is (server anim_state is for remote players, less critical).

---

## Phase 8: Retroactive snapshot verification for visual phases

Phases 2, 3, 5, 6, and 7 modify visual output. The plan's verification approach requires saving before/after snapshots in `snapshots/` and spawning critic sub-agents to audit each change. This was not done during phases 2–3. This phase retroactively captures the current state and runs critics for all visual changes.

**Snapshot protocol:**
- Save to `snapshots/` with naming: `{phase}_{description}_{angle}.png`
- Capture at multiple sun angles (noon 0.25, dusk 0.5) and camera positions
- For avatar-related phases, use close camera positions where the player is visible

**Critic protocol:**
- Spawn 3 sub-agents per visual phase, each reviewing from a different angle:
  - Agent 1 (Correctness): Does the fix achieve what was intended? Compare before/after.
  - Agent 2 (Side effects): Are there regressions in other visual elements?
  - Agent 3 (Code quality): Is the implementation clean and maintainable?

- [x] 8a: Capture comprehensive snapshots for the current state (post phases 2+3): noon and dusk, default camera + steep pitch + close-up. Save in `snapshots/` with clear phase-tagged names.
- [x] 8b: Spawn 3 critic sub-agents reviewing phases 2+3 (rim_light removal + SSAO tuning): correctness, side effects, code quality. All 3 critics PASS — no regressions, clean code, correct implementation.
- [x] 8c: Phases 5–7 code reviewed by 3 critic sub-agents (correctness, side effects, code quality). All PASS. Snapshot tool cannot render players (players: None), so visual verification requires in-game testing. Minor code quality nits: duplicated input computation, magic threshold numbers.

---

## Phase 9: Add player rendering to snapshot tool + visual verification

**Problem:** The `game-snapshot` tool passes `players: None` to `SceneRenderers`, so avatar-related fixes (mesh winding, rotation, animation) cannot be verified visually without running the full game.

**Approach:** Add a `--show-player` flag to the snapshot CLI. When set, instantiate a `PlayerRenderer`, place a single `PlayerInstance` on the terrain at the camera target position (or a configurable `--player-pos`), upload bind-pose bones, and pass `Some(&player_renderer)` to `SceneRenderers`. Optionally accept `--player-yaw` to control facing direction.

**Key integration points:**
- `game-snapshot/src/main.rs` — add CLI flags: `--show-player`, `--player-pos x,y,z` (default: camera target, Y from heightmap), `--player-yaw` (default: face camera)
- `game-snapshot/src/render.rs` — create `PlayerRenderer`, upload one instance + bind-pose bones, pass to `SceneRenderers`
- `game-render::PlayerRenderer::new()` needs `device, queue, surface_format, uniform_bgl, shadow_bgl`
- `PlayerInstance` has `pos_yaw: [f32; 4]` and `color: [f32; 4]`
- Player Y position: use `game_core::terrain::sample_height(&heightmap_data, x, z)` to place on ground

**Success criteria:**
- `game-snapshot --show-player` renders a visible avatar standing on terrain
- Avatar is fully opaque from all angles (no see-through holes from Phase 6 fix)
- Avatar integrates naturally with scene lighting, shadows, and SSAO

- [x] 9a: Add `--show-player`, `--player-pos`, and `--player-yaw` CLI flags to `main.rs`. Thread the new parameters through to `render_frame()`.
- [x] 9b: In `render.rs`, when show-player is true: create `PlayerRenderer`, create one `PlayerInstance` at the given position (Y from heightmap if not specified), upload bind-pose bones, pass `Some(&player_renderer)` to `SceneRenderers`.
- [x] 9c: Captured avatar snapshots from 4 angles + dusk. 3 critics: (1) Mesh holes: PASS — fully opaque from all angles. (2) Scene integration: PASS — good lighting/color, note: lacks ground shadow (separate enhancement). (3) Code quality: PASS — fixed default yaw to face toward camera. Clean implementation.

---

## Phase 10: DRY — extract input computation and animation constants

Two DRY violations flagged by Phase 8c critics. Both are real duplication that can cause divergence bugs during future edits.

**Issue 1 — Duplicated input reading (`game-client/src/lib.rs:498-510` vs `517-519`):**
`menu_open`, `forward`, and `strafe` are computed identically in the per-frame move_yaw block (lines 498-501) and in the 20Hz send block (lines 517-519). If someone changes the menu-open guard or input mapping in one place but not the other, movement and visual facing will disagree. Fix: compute `menu_open`, `forward`, `strafe` once before both blocks, store in local variables, and reuse in both.

**Issue 2 — Magic hysteresis thresholds (`game-client/src/lib.rs:243-246` vs `game-render/src/animation.rs:107-112`):**
The walk-entry threshold (`0.5`) and walk-exit threshold (`0.15`) in `lib.rs` are magic numbers with no named constant. Meanwhile `animation.rs:from_movement()` uses its own threshold (`0.3`) to classify Walk. These three thresholds interact: the hysteresis in `lib.rs` overrides `from_movement()`'s decision, making the `0.3` threshold in `animation.rs` dead for the local player. Fix: define named constants in `game-render/src/animation.rs` (`WALK_ENTER_SPEED`, `WALK_EXIT_SPEED`) and use them in both `from_movement()` and the hysteresis block in `lib.rs`. Update `from_movement()` to use `WALK_ENTER_SPEED` instead of `0.3` so the base classifier and hysteresis agree.

**Key files:**
- `game-client/src/lib.rs:498-527` — input computation + send block
- `game-client/src/lib.rs:241-247` — hysteresis block
- `game-render/src/animation.rs:107-112` — `from_movement()` speed thresholds

**Success criteria:**
- `forward`, `strafe`, `menu_open` computed exactly once per frame in the main loop; both move_yaw and send blocks read the same values
- Hysteresis thresholds are named constants exported from `game-render::animation`
- `from_movement()` uses `WALK_ENTER_SPEED` (0.5) instead of 0.3, making its Walk classification consistent with the hysteresis guard (the guard becomes a no-op for Walk when the base classifier already uses the right threshold, but remains needed for the Idle exit case)
- `cargo build --target wasm32-unknown-unknown` compiles clean; no behavioral change

- [x] 10a: In `game-client/src/lib.rs`, hoist `menu_open`/`forward`/`strafe` computation to before the move_yaw block (~line 497). Remove the duplicate computation from the send block (~lines 517-519). Both blocks reference the same local variables.
- [x] 10b: In `game-render/src/animation.rs`, add `pub const WALK_ENTER_SPEED: f32 = 0.5;` and `pub const WALK_EXIT_SPEED: f32 = 0.15;`. Change `from_movement()` threshold from `0.3` to `WALK_ENTER_SPEED`. In `game-client/src/lib.rs`, replace the magic `0.5` and `0.15` in the hysteresis block with `game_render::animation::WALK_ENTER_SPEED` and `game_render::animation::WALK_EXIT_SPEED`.

---

## Phase 11: Third-person orbit camera in snapshot tool

**Problem:** When `--show-player` is used, the user must manually guess `--camera-pos` and `--camera-target` to frame the avatar. The game has a third-person orbit camera (`game-client/src/camera.rs`) that computes eye position from `(target, yaw, pitch, distance)` using spherical-to-cartesian math. The snapshot tool should reuse this to get automatic game-like framing.

**Approach:** Extract the pure orbit math (4 lines of trig + `TARGET_Y_OFFSET` constant) into `game-core` so both the client `OrbitCamera` and the snapshot tool share the exact same camera computation. Then add an `--orbit` flag to the snapshot CLI that computes `camera_pos`/`camera_target` from the orbit function using the player position as target.

**Why extract to game-core:** The orbit math is small (~4 lines), but it includes a non-obvious constant (`TARGET_Y_OFFSET = 1.2`, the chest-height framing offset). Extracting ensures the snapshot tool produces frames identical to the in-game camera. The client `OrbitCamera` delegates to the shared function — input handling, terrain collision, and smoothing remain in the client only.

**Key files:**
- `game-core/src/camera.rs` (NEW) — pure `orbit_eye()` function + constants (`TARGET_Y_OFFSET`, `MIN_PITCH`, `MAX_PITCH`, `MIN_DISTANCE`, `MAX_DISTANCE`)
- `game-client/src/camera.rs` — refactor `OrbitCamera::eye_at()` and `orbit_center()` to call `game_core::camera::orbit_eye()`
- `game-snapshot/src/main.rs` — add `--orbit`, `--orbit-yaw`, `--orbit-pitch`, `--orbit-distance` flags
- `game-snapshot/src/render.rs` — compute camera from orbit function when `--orbit` is active

**Success criteria:**
- `game-snapshot --orbit --show-player` renders a well-framed third-person view with the avatar centered
- Same yaw/pitch/distance in snapshot and in-game produce matching camera angles
- Manual `--camera-pos`/`--camera-target` mode still works unchanged
- Zero duplicated orbit math between client and snapshot tool

- [x] 11a: Create `game-core/src/camera.rs` with `pub fn orbit_eye(target: Vec3, yaw: f32, pitch: f32, distance: f32) -> (Vec3, Vec3)` returning `(eye_position, look_target)`. Move `TARGET_Y_OFFSET`, `MIN_PITCH`, `MAX_PITCH`, `MIN_DISTANCE`, `MAX_DISTANCE` there as public constants. Register the module in `game-core/src/lib.rs`.
- [x] 11b: Refactor `game-client/src/camera.rs` — `OrbitCamera::eye_at()` and `orbit_center()` call `game_core::camera::orbit_eye()`. Remove the duplicated trig and constant. Verify `make dev` compiles and behaves identically.
- [x] 11c: Add `--orbit` flag to snapshot tool. When set, implies `--show-player`. Add `--orbit-yaw` (default 0.0), `--orbit-pitch` (default 0.4), `--orbit-distance` (default 8.0). In `render.rs`, when orbit mode is active, call `game_core::camera::orbit_eye()` with player position to derive `camera_pos` and `camera_target`. Ignore `--camera-pos`/`--camera-target` in orbit mode.
- [x] 11d: Captured orbit snapshots at yaw=0,π/4,π/2 (dist=12) + close-up (dist=8). 3 critics: (1) Framing: orbit math works, default dist=8 frames player well; dist=12 is too far but functional. (2) Angle parity: PASS — correct spherical math, clean delegation, constants shared. (3) Code quality: PASS — zero duplication, clean separation, good CLI design.

---

## Phase 12: Blob shadow under player avatar

**Problem:** The player avatar appears to float above the terrain. It is not rendered in the shadow cascade passes (`frame.rs:54-76`), so it casts no shadow map depth. Its thin geometry (arm cylinders = 0.04 radius) produces negligible SSAO/contact shadow ground contact.

**Approach:** Render a soft dark ellipse on the terrain under each player instance. A blob shadow fits the impressionist/stylized art direction better than hard real-time shadows, and is far simpler to implement. Draw an alpha-blended quad per player instance in the scene pass, immediately before the player mesh (so player draws on top). Reuses the existing player instance buffer (`pos_yaw` + `color`) — no new GPU data needed.

**Key files:**
- `game-render/src/shaders/blob_shadow.wgsl` (NEW, ~40 lines) — vertex shader reads player instance `pos_yaw`, emits a flat quad at player foot Y. Fragment outputs radial falloff: `alpha = smoothstep(1.0, 0.0, length(uv) * 2.0) * 0.45`. Slight sun-direction offset (`-sun_dir.xz * 0.1`) for grounding cue.
- `game-render/src/blob_shadow.rs` (NEW, ~80 lines) — `BlobShadowRenderer`: alpha-blend pipeline (src=SrcAlpha, dst=OneMinusSrcAlpha, depth write disabled, depth test LessEqual read-only). 4-vertex quad, 6 indices. `draw()` binds uniform BG + player instance buffer, draws 4 verts × instance_count.
- `game-render/src/player.rs` — expose `instance_buffer()` and `instance_count()` accessors so `BlobShadowRenderer` can bind the same data without copying.
- `game-render/src/frame.rs` — add `blob_shadow: Option<&BlobShadowRenderer>` to `SceneRenderers`. Draw before `players.draw()` in the scene pass.
- `game-render/src/lib.rs` — pub export `BlobShadowRenderer`.
- `game-client/src/renderer.rs` — instantiate `BlobShadowRenderer`, pass to `SceneRenderers`.
- `game-snapshot/src/render.rs` — create `BlobShadowRenderer` when `--show-player` is active.

**Shadow parameters:** Ellipse radius ~0.35 world units (slightly wider than shoulder width 0.18×2). Opacity peak 0.45. Soft edge via smoothstep. Slight offset toward sun direction for grounding cue at low sun angles.

**Success criteria:**
- Soft dark oval visible under each player on terrain
- Shadow opacity feels natural — not a hard black circle, not invisible
- No z-fighting or depth artifacts (depth write disabled on shadow quad)
- Works for multiple players (instance count > 1)
- Snapshot verification from noon and dusk angles

- [x] 12a: Create `blob_shadow.wgsl` shader. Vertex: expand 4 unit-quad corners into world-space flat quad at player foot Y from `inst_pos_yaw`, sized ~0.45 radius, with slight sun-direction offset from uniforms. Fragment: radial falloff `smoothstep(1.0, 0.0, dist) * 0.45`, output `vec4(0, 0, 0, alpha)`.
- [x] 12b: Create `BlobShadowRenderer` in `blob_shadow.rs`. Alpha-blend pipeline (depth read-only, no depth write). Quad index buffer (6 indices). `draw()` binds uniform BG + player instance buffer, draws 4 verts × instance_count. Add `instance_buffer()` and `instance_count()` accessors to `PlayerRenderer` and `InstancedMeshRenderer`.
- [x] 12c: Wire into frame pipeline. Add `blob_shadow` field to `SceneRenderers` in `frame.rs`. Draw in scene pass before player. Instantiate in `renderer.rs` (client) and `render.rs` (snapshot tool when `--show-player`). Export from `lib.rs`.
- [x] 12d: Shadow tuned after critic feedback: moved to LDR pass (after bloom/tonemapping to avoid bloom washout), multiply-darken blend (src=Zero, dst=OneMinusSrcAlpha), radius=1.2, intensity=0.7, Y offset=0.15. Visible soft grounding on both light and dark terrain at noon and dusk.

---

## Phase 13: Turntable snapshot mode for avatar debugging

**Problem:** Verifying the avatar mesh from a single angle is unreliable — the Phase 6 winding bug passed critic review because the snapshot only showed angles where faces happened to be correct. A turntable view (multiple angles in one image) would have caught the +Z/-Z face issue immediately.

**Approach:** Add a `--turntable` flag to `game-snapshot` that renders N frames rotating the orbit camera around the player at evenly spaced yaw angles, then composites them into a single grid PNG. Depends on Phase 11 (`--orbit` mode) for the orbit camera math.

**Key files:**
- `game-snapshot/src/main.rs` — add `--turntable` flag (implies `--orbit --show-player`) and optional `--turntable-cols` (default 4) for grid layout
- `game-snapshot/src/render.rs` — refactor `render_frame()` so it can be called multiple times with different camera parameters. Add `render_turntable()` that calls `render_frame()` N times (default 8: every 45°), composites the resulting pixel buffers into one grid image, returns the combined pixels.

**Grid layout:** 8 frames in a 4×2 grid. Each frame at the same pitch/distance but yaw increments of π/4 (0°, 45°, 90°, 135°, 180°, 225°, 270°, 315°). Individual frame size: `width/cols × height/rows` so the output PNG stays at the requested resolution.

**Success criteria:**
- `game-snapshot --turntable` produces a single PNG with 8 views of the avatar from all angles
- All body parts visible and opaque in all 8 views (this would have caught the +Z/-Z bug)
- Clean composition with no gaps or overlap
- Works with `--sun-angle`, `--orbit-pitch`, `--orbit-distance` flags

- [x] 13a: Refactor `render_frame()` in `render.rs` to separate GPU setup (device, textures, renderers) from per-frame rendering. Extract a `SnapshotContext` struct holding the device/queue/renderers, and a `render_view()` method that takes camera params and returns pixels. This avoids recreating GPU resources for each turntable frame.
- [x] 13b: Add `--turntable` and `--turntable-cols` flags. Implement `render_turntable()`: create `SnapshotContext` once, call `render_view()` 8 times with yaw at 0, π/4, π/2, ..., 7π/4. Composite into a grid: divide output resolution by cols/rows, render each frame at sub-size, blit into final pixel buffer.
- [x] 13c: Turntable captured at (60,60), noon. 3 critics: (1) Mesh: avatar fully opaque from all 8 angles, no holes. (2) Consistency: backgrounds vary as expected from orbit; avatar size/position consistent. (3) Code quality: PASS — clean SnapshotContext refactor, no GPU duplication, correct compositing.

---

## Phase 14: Fix screen-space contact shadow distance dependence

**Problem:** The contact shadow ray march (`postprocess.wgsl:24-80`) uses a fixed NDC-space depth threshold (`depth_diff < 0.01`, line 74) to detect occlusion. NDC depth is nonlinear — the same world-space gap maps to vastly different NDC values at different camera distances. At medium orbit distance, the depth gap between the player's silhouette edge and the terrain behind it falls within the 0.0002–0.01 window, causing the sun-direction march to register false occlusion. This produces a one-sided dark halo that changes with camera distance.

**Root cause:** `march_depth - scene_depth` on line 73 is a comparison in NDC space, which is nonlinear. The same 0.5-meter world-space gap is ~0.001 in NDC at 5m distance but ~0.00001 at 50m. The fixed [0.0002, 0.01] window catches thin geometry at specific camera distances and misses it at others.

**Fix:** Linearize the depth comparison so the thickness window represents a constant world-space distance regardless of camera range. Also add a max-distance guard that rejects samples where the starting pixel and marched point are separated by more than a world-space threshold (e.g., 2m) — this prevents the ray from "jumping" from the player surface onto distant terrain behind.

**Key file:** `game-render/src/postprocess.wgsl:24-80`

**Success criteria:**
- No dark halo on player at any camera distance (near, mid, far orbit)
- Contact shadows still visible on terrain edges, rock crevices (the intended use case)
- No performance regression (linearize is a single `1/(a*d+b)` per sample)

- [x] 14a: In `contact_shadow()`, linearize the depth comparison. Added `linearize_depth()` helper + CS_NEAR/CS_FAR constants to postprocess.wgsl. Changed threshold window from [0.0002, 0.01] NDC to [0.02, 0.5] world units.
- [x] 14b: Added depth-discontinuity guard: inside the loop, reject samples where `abs(scene_linear - march_linear)` exceeds 1.5 world units (matches march distance). Prevents ray from crossing silhouette edges onto distant background.
- [x] 14c: Before/after snapshots at 3 orbit distances (close=4, mid=8, far=15). 3 critics: (1) Halo: PASS — no silhouette darkening at any distance. (2) Terrain: PASS — contact shadows preserved at rock/tree bases. (3) Code quality: initial FAIL on guard comparing wrong reference depth (start vs march) — fixed to compare against march_linear, tightened threshold to 1.5.

---

## Phase 15: SSAO robustness for mixed-scale geometry

**Problem:** SSAO (`ssao.wgsl`) uses `RADIUS=2.0` world units and `STRENGTH=3.5`. The player avatar has thin geometry (limbs ~0.1 unit radius). At these scales, hemisphere samples project through limbs or onto distant terrain behind the player, producing heavy false occlusion at silhouette edges. The half-res AO buffer further blurs these artifacts.

**Contributing factors:**
- `RADIUS=2.0` is 20× the limb radius — most samples land far from the surface on thin geometry (line 12)
- `STRENGTH=3.5` means only 4 of 12 false-positive samples produce full occlusion (line 95: `1.0 - (4/12) * 3.5 = 0.0`)
- `step(0.04, diff)` on line 91 is a hard threshold with no thickness test — any sample behind the surface counts as occluded, even if the occluder is very thin
- Normals reconstructed from `cross(dpdx(pos), dpdy(pos))` on line 36 are unstable at silhouette edges where the derivative straddles the object boundary
- Bilateral blur depth threshold `0.005` in `postprocess.wgsl:183` is in NDC space — same nonlinear issue as contact shadows

**Fix approach:** Add a thickness heuristic to SSAO and linearize the bilateral blur threshold. The thickness heuristic: if the depth difference between the sample and the surface is large (thin geometry seen from the side), the occlusion is likely a false positive from sampling "through" a thin object. Reject it with a falloff. Also reduce RADIUS and STRENGTH to values more appropriate for the scene's mixed geometry scales.

**Key files:**
- `game-render/src/ssao.wgsl` — sample loop and parameters
- `game-render/src/postprocess.wgsl:172-203` — bilateral blur

**Success criteria:**
- No dark halos around player limbs at any camera distance
- Terrain/rock crevice AO still provides depth (corners, overhangs should still darken)
- Smooth AO without banding or visible half-res artifacts

- [x] 15a: Replaced hard `step(0.04, diff) * range_ok` with thickness heuristic: `smoothstep(0.0, 0.04, diff) * (1.0 - smoothstep(RADIUS*0.5, RADIUS, diff))`. Accepts real occlusion, rejects sampling through thin geometry.
- [x] 15b: Reduced RADIUS 2.0→1.0, STRENGTH 3.5→2.5. Better suited for mixed terrain + player geometry scales.
- [x] 15c: Linearized bilateral blur: compare in linear depth space (`linearize_depth(sample_depth)` vs `center_linear`) with fixed 0.4 world-unit threshold. Critic caught initial bug (mixed NDC/world-space units) — fixed to compare linearized depths on both sides.
- [x] 15d: 3 critics: (1) Player halos: initial FAIL (player not visible in snapshot) — retook at open terrain pos. PASS on retake. (2) Terrain AO: PASS — rock/tree base darkening preserved. (3) Code quality: initial FAIL on bilateral blur unit mismatch — fixed. Thickness heuristic and parameters PASS.

---

## Phase 16: Add player shadow casting

**Problem:** The player does not cast shadows into the shadow map (`frame.rs:73-76` renders only terrain, rocks, trees in the shadow pass). But the player fragment shader samples the shadow map (`player.wgsl:77`). This means:
1. No player-on-terrain shadow (the blob shadow from Phase 12 is the only grounding cue)
2. Terrain depth in the shadow map can falsely shadow the player's body — parts of the player that are above terrain level but laterally offset from the sun's perspective can sample terrain shadow texels
3. The fixed shadow bias `0.003` (`common.wgsl:47`) has no slope-scale component, making it unreliable for the player's varied surface orientations

**Fix:** Add a `draw_shadow` method to `PlayerRenderer` using a depth-only vertex shader (skeletal skinning + sun VP transform). Render the player into the shadow cascades alongside terrain/rocks/trees. Also add slope-scaled shadow bias so all objects get correct self-shadow behavior.

**Key files:**
- `game-render/src/player.rs` — add `draw_shadow()` method
- `game-render/src/player.wgsl` — add `vs_shadow` entry point (skinning + sun VP, no fragment)
- `game-render/src/frame.rs:73-76` — add `players.draw_shadow()` in shadow pass
- `game-render/src/common.wgsl:47` — slope-scaled bias
- `game-render/src/instanced_mesh.rs` — may need a `draw_shadow()` variant or the player can use its own shadow pipeline

**Contracts:**
- `vs_shadow` in `player.wgsl`: applies bone skinning + yaw rotation + instance offset (same as `vs_main` lines 40-64), then transforms by `u.sun_view_proj` instead of `u.view_proj`. No fragment output (depth-only).
- `draw_shadow` in `player.rs`: creates a shadow pipeline (using `create_shadow_pipeline`), binds vertex/instance/bone buffers, draws indexed.
- Shadow bias in `common.wgsl`: change `let bias = 0.003;` to `let bias = max(0.005 * (1.0 - ndotl), 0.001);` where `ndotl = dot(normal, sun_dir)`. This requires passing the surface normal into `sample_shadow()` — update all call sites (terrain, rocks, trees, player, grass).

**Success criteria:**
- Player casts a correct directional shadow on the terrain (visible from orbit camera)
- No shadow acne on the player (bias is sufficient)
- No false terrain-on-player shadows (the player's own depth now occupies the shadow map)
- All existing shadows (terrain self-shadow, tree shadows) unchanged

- [ ] 16a: Add `vs_shadow` entry point to `player.wgsl`. Same vertex transform as `vs_main` (bone skinning + yaw rotation + instance offset) but output `u.sun_view_proj * vec4(world_pos, 1.0)` as clip position. No fragment shader needed.
- [ ] 16b: Add `draw_shadow()` method to `PlayerRenderer` in `player.rs`. Create a shadow pipeline using `create_shadow_pipeline()` with the same vertex buffer layout as the scene pipeline. Store as a field. `draw_shadow()` binds vertex/instance/bone buffers and draws.
- [ ] 16c: Wire into frame.rs shadow pass: after `scene.trees.draw_shadow()` (line 76), add `if let Some(players) = scene.players { players.draw_shadow(&mut pass, uniform_bg); }`.
- [ ] 16d: Add slope-scaled bias to `sample_shadow()` in `common.wgsl`. Change signature to accept `normal: vec3<f32>`. Compute `let ndotl = max(dot(normal, u.sun_dir), 0.0); let bias = max(0.005 * (1.0 - ndotl), 0.001);`. Update all call sites: `terrain.wgsl`, `rocks.wgsl`, `trees.wgsl`, `grass.wgsl`, `player.wgsl` — pass the surface normal to `sample_shadow(world_pos, normal)`.
- [ ] 16e: Evaluate whether the blob shadow (Phase 12) is still needed now that the player casts real directional shadows. If redundant, remove `BlobShadowRenderer` and related code. If the blob shadow provides complementary value (e.g., soft ambient grounding at dusk when directional shadow is faint), keep it but reduce its intensity.
- [ ] 16f: Take snapshots at noon + dusk with player visible from multiple angles. Spawn 3 critics: (1) shadow correctness — player casts shadow on terrain, no acne; (2) bias — no false shadows on player from terrain; (3) consistency — all objects (terrain, rocks, trees, player) cast and receive shadows uniformly.

---

## Phase 17: DRY cleanup — shared shader functions

**Problem:** `triplanar_sample()` is copy-pasted between `rocks.wgsl:56-66` and `trees.wgsl:95-105` — identical logic, different texture layer indices. Any change to the sampling math must be made in two places. This is the only remaining DRY violation identified across shaders.

**Fix:** Move `triplanar_sample()` into `common.wgsl` where all shared lighting functions already live. The function takes `world_pos`, `normal`, and `layer` as parameters — fully generic. Both `rocks.wgsl` and `trees.wgsl` call the shared version.

**Key files:**
- `game-render/src/common.wgsl` — add `triplanar_sample()`
- `game-render/src/rocks.wgsl:56-66` — remove local copy, call shared version
- `game-render/src/trees.wgsl:95-105` — remove local copy, call shared version

**Note:** Both shaders already include `common.wgsl` via their shader module construction (e.g., `rocks.rs` does `format!("{}\n{}\n{}\n{}", uniforms, noise, common, rocks)`), so the function will be available without any pipeline changes.

**Success criteria:**
- `triplanar_sample` exists once in `common.wgsl`
- Both `rocks.wgsl` and `trees.wgsl` produce identical visual output
- No compilation errors

- [ ] 17a: Move `triplanar_sample()` from `rocks.wgsl` into `common.wgsl` (after the existing shared functions). Remove the local copy from both `rocks.wgsl` and `trees.wgsl`. Verify both shaders still compile and the atlas texture/sampler bindings are correct (both rocks and trees bind the atlas at the same group/binding, confirmed by their pipeline layouts).
- [ ] 17b: Verify no visual regression by taking snapshots comparing before/after for a scene with rocks and trees visible.
