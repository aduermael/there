# Camera Polish: Stutter Fix, Target Offset & DRY Cleanup — 2026-04-12

## Context

Two issues reported:
1. **Camera stutters on jump** — `camera.target` tracks `local_pos` directly. During a jump, the orbit center bobs up/down with the player (JUMP_VELOCITY=8, GRAVITY=-20), causing the view to bounce and terrain collision to oscillate. Standard 3rd-person games smooth vertical camera tracking. Also, initial distance 15.0 is too large for the forested spawn area.
2. **Camera target feels low** — `TARGET_Y_OFFSET = 1.2` targets upper chest; user wants shoulder/head height.

Industry research (Valheim, GTA V, RDR2) confirms:
- Target point at 70-80% of character height (~1.3-1.4 for a 1.7m character = shoulder line)
- Default orbit distance 4-8m (Valheim ~4m, GTA/RDR2 preset system 2-8m)
- Fast snap-in on collision, slow ease-out on recovery (already implemented)
- Camera snaps to final position on spawn — never interpolates from a wrong starting position
- Input sensitivity is direct (no smoothing on yaw/pitch)

Code quality audit found several DRY violations and scattered constants worth consolidating.

---

## Phase 1: DRY — Consolidate Camera Constants & Remove Duplication

**Codebase context:**
- `game-core/src/camera.rs:4-8` — shared constants: `TARGET_Y_OFFSET`, `MIN_PITCH`, `MAX_PITCH`, `MIN_DISTANCE`, `MAX_DISTANCE`
- `game-client/src/camera.rs:7-8,33-35` — client constants: `SENSITIVITY`, `ZOOM_SPEED`, `APPROACH_RATE`, `RECOVER_RATE`
- `game-client/src/camera.rs:113-114` — `CLEARANCE` and `RAY_STEPS` hidden inside `update()` body
- `game-client/src/lib.rs:212` — `turn_speed = 12.0` as a let binding
- `game-client/src/lib.rs:310-314` — camera init with magic numbers `0.5, 0.35, 15.0`
- `game-client/src/lib.rs:536-537` — `camera.eye()` and `camera.look_target()` call `orbit_at()` twice
- `game-client/src/camera.rs:80-81` — `on_pointer_move` duplicates `apply_drag` logic
- `game-client/src/lib.rs:538-542` + `game-snapshot/src/render.rs:144,274` — FOV/near/far duplicated

**Success criteria:**
- All camera constants in one visible place per crate (game-core for shared, top of camera.rs for client-only)
- No duplicate trig calls in the render loop
- `on_pointer_move` delegates to `apply_drag`
- FOV/near/far defined once in game-core
- Zero behavioral change — all outputs identical

- [x] 1a: Add `DEFAULT_PITCH`, `DEFAULT_DISTANCE`, `FOV`, `NEAR_PLANE`, `FAR_PLANE` constants to `game-core/src/camera.rs`. Add `PLAYER_TURN_SPEED` to `game-core/src/lib.rs`. Use them from client init (`lib.rs:310-314`), client render (`lib.rs:538-542`), client animation (`lib.rs:212`), and snapshot tool (`render.rs:144,274`).
- [x] 1b: Hoist `CLEARANCE` and `RAY_STEPS` from inside `update()` body to module-level constants in `game-client/src/camera.rs`.
- [x] 1c: Add `eye_and_target(&self) -> (Vec3, Vec3)` method to `OrbitCamera`. Replace separate `camera.eye()` + `camera.look_target()` calls in `lib.rs` render loop with a single call.
- [x] 1d: Refactor `on_pointer_move` to compute deltas then delegate to `apply_drag`, eliminating the duplicated sensitivity+clamp lines.
- [x] 1e: Snapshot verification — run `idle_back.json` and `turntable.json`, verify output is unchanged. Build all 3 targets with zero warnings. 3 sub-agent critics validate.

---

## Phase 2: Fix Camera Stutter on Jump + Improve Initial Distance

**Root cause — jump stutter:** `camera.target` is set to `self.local_pos` every frame (`lib.rs:201`). During a jump, `local_pos.y` changes rapidly (JUMP_VELOCITY=8.0, GRAVITY=-20.0), so the camera orbit center bobs up and down with the player. This causes:
- The camera view to bounce vertically every jump
- Terrain collision conditions to change rapidly as the orbit center moves, causing `effective_distance` to oscillate
- A jarring, stuttery feel — standard 3rd-person games (Valheim, RDR2) smooth or decouple the camera's vertical tracking from the player's Y position during jumps

**Root cause — initial distance:** Default distance 15.0 is far too large for the forested spawn terrain. Camera clips into trees on the first frames.

**Design — smooth vertical tracking:**
- Add a `smoothed_target` field to `OrbitCamera` that tracks the player position with asymmetric vertical smoothing: fast follow when the player is on the ground or landing, slow/dampened follow when airborne (jumping)
- Horizontal (XZ) tracking stays instant — no lag when running
- Vertical (Y) tracking uses exponential smoothing: fast rate when grounded (~15-20/s), slower rate when airborne (~4-6/s) to absorb jump bobbing
- The smoothed target is used for `orbit_eye` instead of raw `target`
- On spawn/teleport: snap `smoothed_target` = `target` immediately (no interpolation)

**Contracts:**
- `OrbitCamera::update(dt, heightmap)` gains vertical smoothing of `self.target.y` into an internal `smoothed_target`
- `orbit_at()` uses `smoothed_target` instead of `target`
- New constant: `VERTICAL_FOLLOW_RATE` in `game-client/src/camera.rs` (tunable, ~6.0)
- `OrbitCamera::new()` initializes `smoothed_target = target` (no stutter on first frame)

**Success criteria:**
- Jump causes smooth, gentle camera rise/fall instead of 1:1 bobbing
- Running on flat ground: no visible vertical lag
- Walking up/down slopes: camera follows smoothly
- Teleport/spawn: no stutter (snap bypasses smoothing)
- Default distance at spawn shows the player clearly

- [x] 2a: Add `smoothed_target: Vec3` field to `OrbitCamera`. Initialize it to `target` in `new()`. In `update()`, smooth `smoothed_target.y` toward `target.y` using exponential decay (`VERTICAL_FOLLOW_RATE`). XZ components track instantly: `smoothed_target.x = target.x`, `smoothed_target.z = target.z`. Change `orbit_at()` to use `smoothed_target` instead of `target`.
- [x] 2b: Reduce `DEFAULT_DISTANCE` from 15.0 to 6.0 in `game-core/src/camera.rs`. Update client init to use the constant.
- [x] 2c: Snapshot verification — run idle_back, turntable, walk_forward scenarios. Build all 3 targets with zero warnings. 3 sub-agent critics validate.

---

## Phase 3: Tune Camera Target & Distance Parameters

**Design decisions from industry research:**
- Raise `TARGET_Y_OFFSET` from 1.2 to 1.4 (shoulder line — ~80% of 1.7m character). This gives a slightly more cinematic framing with more ground visible ahead of the player, matching Valheim/RDR2 feel.
- Reduce `MIN_DISTANCE` from 3.0 to 2.0 (allow tighter zoom for cramped areas)
- Reduce `MAX_DISTANCE` from 20.0 to 12.0 (prevent player becoming a speck; still generous for open-world)
- Reduce `CLEARANCE` from 1.8 to 1.0 (current value floats camera too high above gentle hills; with `TARGET_Y_OFFSET` increase the graduated clearance needs less headroom)

**Success criteria:**
- Camera frames the player at shoulder level — horizon roughly splits upper third
- Zoom range feels natural (not too close, not too far)
- Camera doesn't float visibly above gentle terrain

- [x] 3a: Update `TARGET_Y_OFFSET` to 1.4, `MIN_DISTANCE` to 2.0, `MAX_DISTANCE` to 12.0, `CLEARANCE` to 1.0. These are all single-line constant changes in `game-core/src/camera.rs` and `game-client/src/camera.rs`.
- [x] 3b: Snapshot verification — render idle_back, idle_side, turntable with new params. Compare framing before/after. 3 sub-agent critics validate that player framing is improved (higher target, tighter zoom range, camera closer to terrain on hills).

---

## Phase 4: Final Verification

Full scenario suite to confirm no regressions and overall quality.

- [x] 4a: Run all 8 verification scenarios from `snapshots/scenarios/verification/`. Run all 3 follow scenarios. Verify with 3 sub-agent critics.
- [x] 4b: Build all 3 targets (`game-client` wasm, `game-server`, `game-snapshot`) — zero warnings. Run CLI-only snapshot test.
