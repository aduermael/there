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

- [x] 6a: Fix `add_box()` index winding: reverse to `[i, i+2, i+1, i, i+3, i+2]`. Fix `add_cylinder()` side winding (caps were already correct). Fix `add_ellipsoid()` winding — reversed its two triangles. All three shapes now produce outward-facing normals for correct back-face culling.

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
