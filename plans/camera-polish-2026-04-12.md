# Camera Polish: Stutter Fix, Target Offset & DRY Cleanup — 2026-04-12

## Context

Two issues reported:
1. **Camera stutters on join** — initial distance 15.0 is far too large for the forested spawn area (128,128). The terrain collision system aggressively snaps the camera closer across several frames, causing visible jumps. At distance 15 the player is invisible among trees.
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

- [ ] 1a: Add `DEFAULT_PITCH`, `DEFAULT_DISTANCE`, `FOV`, `NEAR_PLANE`, `FAR_PLANE` constants to `game-core/src/camera.rs`. Add `PLAYER_TURN_SPEED` to `game-core/src/lib.rs`. Use them from client init (`lib.rs:310-314`), client render (`lib.rs:538-542`), client animation (`lib.rs:212`), and snapshot tool (`render.rs:144,274`).
- [ ] 1b: Hoist `CLEARANCE` and `RAY_STEPS` from inside `update()` body to module-level constants in `game-client/src/camera.rs`.
- [ ] 1c: Add `eye_and_target(&self) -> (Vec3, Vec3)` method to `OrbitCamera`. Replace separate `camera.eye()` + `camera.look_target()` calls in `lib.rs` render loop with a single call.
- [ ] 1d: Refactor `on_pointer_move` to compute deltas then delegate to `apply_drag`, eliminating the duplicated sensitivity+clamp lines.
- [ ] 1e: Snapshot verification — run `idle_back.json` and `turntable.json`, verify output is unchanged. Build all 3 targets with zero warnings. 3 sub-agent critics validate.

---

## Phase 2: Fix Camera Stutter on Join

**Root cause:** Initial orbit distance (15.0) is far too large for the forested spawn terrain. The collision system (`update()`) detects the camera clipping terrain/trees on the first frames and rapidly shrinks `effective_distance`, causing visible stutter. Additionally, there is no mechanism to skip smoothing on the initial frame.

**Design:**
- Reduce default distance to a value that works well at spawn (6.0, matching Valheim-like framing)
- Add a `snap()` method to `OrbitCamera` that sets `effective_distance = desired_distance` (bypasses smoothing). Call it once at camera creation time so the first frame renders at the collision-adjusted distance with no transition.
- In `snap()`, also run one collision pass so the very first render uses a valid collision distance — no multi-frame settling.

**Contracts:**
- `OrbitCamera::snap(heightmap)` — run terrain collision once, set effective_distance = collision result. No smoothing.
- Called once after `OrbitCamera::new()` in client init
- Subsequent frames use normal smoothed collision as before

**Success criteria:**
- First frame renders with camera at a stable position (no visible snap/stutter)
- Camera at spawn shows the player clearly with good framing
- Subsequent distance smoothing behavior unchanged

- [ ] 2a: Reduce `DEFAULT_DISTANCE` from 15.0 to 6.0. Add `OrbitCamera::snap(&mut self, heightmap: &[f32])` that runs one collision pass and sets `effective_distance` to the result (reuse the same raycast logic from `update`). Call `snap()` immediately after camera creation in client init.
- [ ] 2b: Snapshot verification at spawn — render at player_pos=[128,-1,128] with orbit_distance=6.0. Verify player is visible and well-framed. 3 sub-agent critics validate.

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

- [ ] 3a: Update `TARGET_Y_OFFSET` to 1.4, `MIN_DISTANCE` to 2.0, `MAX_DISTANCE` to 12.0, `CLEARANCE` to 1.0. These are all single-line constant changes in `game-core/src/camera.rs` and `game-client/src/camera.rs`.
- [ ] 3b: Snapshot verification — render idle_back, idle_side, turntable with new params. Compare framing before/after. 3 sub-agent critics validate that player framing is improved (higher target, tighter zoom range, camera closer to terrain on hills).

---

## Phase 4: Final Verification

Full scenario suite to confirm no regressions and overall quality.

- [ ] 4a: Run all 8 verification scenarios from `snapshots/scenarios/verification/`. Run all 3 follow scenarios. Verify with 3 sub-agent critics.
- [ ] 4b: Build all 3 targets (`game-client` wasm, `game-server`, `game-snapshot`) — zero warnings. Run CLI-only snapshot test.
