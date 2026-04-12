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
- [ ] 4c: Client uses computed `move_yaw` for local player instance rendering (`lib.rs:208-215`) instead of `camera.yaw`. Verify remote players also display correct rotation from server state.
