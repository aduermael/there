# Player Orientation, Camera Follow & Scenario System — 2026-04-12

## Context

Prior work (bugfixes-polish-2026-04-12.md, Phases 4–7) fixed player model facing:
- Player faces movement direction via `(-move_x).atan2(-move_z)` (`lib.rs:509`)
- Visual yaw smooth interpolation with shortest-arc wrap (`lib.rs:211-218`)
- Mesh winding, idle animation jitter, hysteresis

**What's still wrong:** The camera never auto-follows behind the player during movement. When the player moves forward, the camera stays in its current orbit position, so the player may appear to walk sideways or toward the camera depending on camera angle. The user expects: press forward → see the player's back as the camera swings behind.

**What this plan adds:**
1. **Camera auto-follow** — when moving, camera yaw smoothly rotates to position behind the player's movement direction. When idle, camera orbits freely.
2. **JSON scenario system** — snapshot tool loads test scenarios from JSON files. CLI flags override JSON values.
3. **Movement simulation in scenarios** — scenarios can define input sequences ("move forward 1 sec, take snapshot") for headless testing of orientation and camera behavior.

**DRY refactors bundled in:**
- Shortest-arc yaw interpolation duplicated in `lib.rs:211-218` and `game_loop.rs:41-50` → extract to `game_core`
- Movement direction vector duplicated in `movement.rs:19-24` and `lib.rs:504-508` → extract helper to `game_core::movement`

## Verification approach

At the end of each phase, run snapshot scenarios and give outputs to **3 sub-agent critics** reviewing from different angles:
1. **Correctness** — does the behavior match the spec?
2. **Side effects** — are there regressions in other visual/gameplay elements?
3. **Code quality** — is the implementation clean, DRY, simple?

The executing agent **iterates up to 5 times** per phase until all 3 critics pass.

---

## Phase 1: JSON Scenario System for Snapshot Tool

Build the testing foundation. Scenarios are JSON files in `snapshots/scenarios/` that replace CLI flags.

**Codebase context:**
- `game-snapshot/src/main.rs:5-67` — `Args` struct with clap derive (all current CLI flags)
- `game-snapshot/src/main.rs:80-158` — `main()` dispatches turntable vs single frame
- `game-snapshot/src/render.rs` — `render_frame()` and `render_turntable()` async functions
- Snapshot tool uses `clap`, `serde`, `image`, `pollster`, `glam`, `game-core`, `game-render`

**JSON schema (static snapshot):**
```json
{
  "width": 1920, "height": 1080,
  "sun_angle": 0.25,
  "output": "snapshots/test.png",
  "show_player": true,
  "player_pos": [128, -1, 128],
  "player_yaw": 0.0,
  "orbit": true,
  "orbit_yaw": 0.0,
  "orbit_pitch": 0.4,
  "orbit_distance": 8.0,
  "turntable": false,
  "turntable_cols": 4
}
```
All fields optional — defaults match current CLI defaults. `player_pos` Y=-1 means auto from heightmap.

**Contracts:**
- `--scenario <path>` loads JSON, merges with CLI defaults, CLI flags override JSON values
- Existing CLI-only usage unchanged (no `--scenario` = current behavior)
- Invalid JSON → clear error message with file path and parse details
- `serde_json` is the only new dependency needed (serde already in Cargo.toml)

**Success criteria:**
- `--scenario idle_front.json` produces identical output to equivalent CLI flags
- `--scenario idle_front.json --sun-angle 0.75` correctly overrides sun angle
- All 4 initial scenarios render valid PNGs

- [x] 1a: Create `snapshots/scenarios/` directory with 4 static scenarios: `idle_front.json` (orbit yaw=PI, camera facing player front), `idle_back.json` (orbit yaw=0), `idle_side.json` (orbit yaw=PI/2), `turntable.json` (turntable mode).
- [x] 1b: Add `serde_json` dependency. Create a `ScenarioConfig` struct with `#[derive(Deserialize, Default)]` matching the JSON schema. Add `--scenario` flag to `Args`. In `main()`, if scenario is provided: load JSON → deserialize → merge with CLI defaults (JSON wins, then explicit CLI flags override). Refactor main logic to work from the merged config.
- [x] 1c: Run all 4 scenarios, verify outputs. Test one CLI override (`--sun-angle`). Snapshot verification with 3 sub-agent critics.

---

## Phase 2: Movement Simulation in Scenarios

Add step-based input simulation so scenarios can describe "move forward 1 sec, take snapshot."

**Codebase context:**
- `game-core/src/movement.rs:7-44` — `apply_movement(pos, forward, strafe, yaw, dt, heightmap) -> Vec3`
- `game-core/src/lib.rs:7-8` — `TICK_RATE_HZ=20`, `TICK_INTERVAL_SECS=0.05`
- `game-core/src/terrain.rs` — `generate_heightmap()`, `sample_height()`

**Extended JSON schema (add `steps` array):**
```json
{
  "orbit": true, "show_player": true,
  "player_pos": [128, -1, 128],
  "orbit_pitch": 0.4, "orbit_distance": 8.0,
  "sun_angle": 0.25,
  "steps": [
    { "input": { "forward": 1.0 }, "duration_secs": 1.0 },
    { "snapshot": "walk_forward_1s.png" },
    { "input": { "forward": 0.0, "strafe": 1.0 }, "duration_secs": 0.5 },
    { "snapshot": "then_strafe_right.png" }
  ]
}
```

**Contracts:**
- When `steps` is present, simulation runs tick-by-tick at `TICK_RATE_HZ`
- For each `input` step: run `apply_movement()` for `duration_secs / TICK_INTERVAL_SECS` ticks, advancing player position. Player yaw tracks movement direction (same atan2 formula as client).
- For each `snapshot` step: compute orbit camera from current player pos + scenario orbit params, render frame, save PNG.
- When `steps` is absent: unchanged static snapshot behavior.
- Camera yaw during simulation: initially from `orbit_yaw` in config. Static for now (auto-follow comes in Phase 3).
- Step snapshot filenames are relative to the scenario file's directory (or CWD if no scenario).

**Success criteria:**
- A "walk forward 1 sec" scenario shows the player displaced ~5 units along -Z from start
- A "strafe right" scenario shows lateral displacement along +X
- Player yaw in snapshot matches movement direction

- [x] 2a: Add `steps` parsing to `ScenarioConfig`. Define step variants: `Input { forward, strafe, duration_secs }` and `Snapshot { output }`. All input fields default to 0.0.
- [x] 2b: Implement simulation loop in snapshot tool: generate heightmap once, then iterate steps. For `Input`: run `apply_movement()` N ticks, track `player_pos` and compute `player_yaw` from movement direction. For `Snapshot`: compute orbit camera from current player state, call `render_frame()`, save PNG.
- [x] 2c: Create 5 movement scenarios in `snapshots/scenarios/`: `walk_forward.json`, `walk_backward.json`, `strafe_left.json`, `strafe_right.json`, `diagonal.json`. Each moves 1 second then captures. Run all and verify with 3 sub-agent critics.

---

## Phase 3: DRY — Extract Shared Math to game-core

Two patterns are duplicated across client and server. Extract before adding camera follow logic (which will reuse them).

**Duplication 1 — Shortest-arc yaw interpolation:**
- `game-client/src/lib.rs:213-218` — visual yaw toward move_yaw
- `game-server/src/game_loop.rs:43-50` — server yaw toward move_yaw
- Pattern: `diff = wrap_to_pi(target - current); current += clamp(diff, -max_step, max_step); current = wrap_to_pi(current)`

**Duplication 2 — Movement direction vector:**
- `game-core/src/movement.rs:19-24` — inside `apply_movement()`
- `game-client/src/lib.rs:504-508` — for `local_move_yaw` computation
- Pattern: `move_x = -sin(yaw)*fwd + cos(yaw)*strafe; move_z = -cos(yaw)*fwd - sin(yaw)*strafe`

**Contracts:**
- `game_core::movement::move_direction(forward, strafe, yaw) -> (f32, f32)` — returns normalized (move_x, move_z)
- `game_core::movement::move_yaw(forward, strafe, camera_yaw) -> f32` — returns atan2 of movement direction
- `game_core::movement::lerp_angle(current, target, max_step) -> f32` — shortest-arc interpolation, returns wrapped result

**Success criteria:**
- No behavioral change — all call sites produce identical output
- `cargo build -p game-client --target wasm32-unknown-unknown` and `cargo build -p game-server` compile clean
- Movement direction computed in exactly one place; yaw interpolation logic not duplicated

- [x] 3a: Add `move_direction(forward, strafe, yaw) -> (f32, f32)` and `move_yaw(forward, strafe, camera_yaw) -> f32` to `game-core/src/movement.rs`. Refactor `apply_movement()` to call `move_direction()` internally. Update `lib.rs:504-508` to call `move_yaw()`.
- [x] 3b: Add `lerp_angle(current, target, max_step) -> f32` to `game-core/src/movement.rs` (or a new `math` module if preferred). Update `lib.rs:213-218` and `game_loop.rs:43-50` to call it. Verify both client and server compile.

---

## Phase 4: Camera Auto-Follow During Movement

When the player moves, the camera smoothly rotates to position behind the player's movement direction. When idle, the camera orbits freely (user-controlled).

**Codebase context:**
- `game-client/src/camera.rs:37-48` — `OrbitCamera` struct (yaw, pitch, desired_distance, etc.)
- `game-client/src/camera.rs:71-82` — `on_pointer_move()` updates yaw/pitch from drag
- `game-client/src/lib.rs:494-495` — `update_movement(dt)` then `update_camera(dt)`
- `game-client/src/lib.rs:502-510` — `local_move_yaw` computed from input + camera.yaw
- `game-client/src/lib.rs:527-531` — touch drag applied

**Design:**
- New method on `OrbitCamera`: `follow_behind(&mut self, move_yaw: f32, dt: f32)` — smoothly rotates `self.yaw` toward `move_yaw + PI` (behind the movement direction) using `lerp_angle` from Phase 3.
- `CAMERA_FOLLOW_SPEED: f32 = 4.0` — radians/sec, tunable. Fast enough to feel responsive, slow enough to avoid feedback spiral.
- Called in the frame loop AFTER computing `local_move_yaw`, only when input is active (`forward != 0 || strafe != 0`).
- When idle: `follow_behind` is not called, so camera yaw is controlled only by user drag/touch.
- Pitch and distance remain entirely user-controlled at all times.

**Frame loop order (updated):**
1. Compute `forward`, `strafe` from input
2. `update_movement(dt)` — applies movement using current `camera.yaw`
3. Compute `local_move_yaw` from movement direction
4. If moving: `camera.follow_behind(local_move_yaw, dt)` — camera yaw starts catching up
5. Apply touch drag (if any) — user can still fight the follow
6. `update_camera(dt)` — terrain collision + distance smoothing
7. `build_player_instances()` — visual yaw interpolates toward `local_move_yaw`

**Critical: compute movement direction BEFORE updating camera yaw** (step 2 before step 4) to break the feedback loop that would cause spiral rotation.

**Failure modes:**
- Feedback spiral: mitigated by computing movement BEFORE camera follow within the same frame
- Camera jump on first movement frame: `lerp_angle` handles large angle differences smoothly
- Backward movement (`forward=-1`): movement yaw points toward camera, camera follows behind = camera ends up where player was facing. Pressing backward continuously creates a 180° flip — this is correct and natural.

**Success criteria:**
- Press forward → camera smoothly swings behind player → player's back visible
- Strafe right → camera swings to be behind the rightward movement → smooth transition
- Stop moving → camera stays in current position, freely orbitable
- Rotate camera while idle → player doesn't rotate
- Start walking after camera rotation → player faces new movement direction, camera follows behind

- [x] 4a: Add `CAMERA_FOLLOW_SPEED` constant and `follow_behind(&mut self, move_yaw: f32, dt: f32)` method to `OrbitCamera` in `game-client/src/camera.rs`. Uses `game_core::movement::lerp_angle` to interpolate `self.yaw` toward `move_yaw + PI`.
- [x] 4b: Integrate into frame loop in `lib.rs`: call `camera.follow_behind(local_move_yaw, dt)` after computing `local_move_yaw` and before `update_camera(dt)`, only when movement input is active.
- [x] 4c: Add camera follow simulation to snapshot tool's step loop (Phase 2). During `Input` steps, after each movement tick, also interpolate the simulated orbit_yaw toward behind the movement direction at the same follow speed.
- [x] 4d: Create 3 auto-follow test scenarios in `snapshots/scenarios/`: `follow_forward.json` (walk forward 2s — camera should end up behind), `follow_strafe.json` (strafe right 2s — camera should swing right), `follow_then_idle.json` (walk 1s, stop, rotate camera in different orbit_yaw, verify player didn't move). Run all and verify with 3 sub-agent critics.

---

## Phase 5: Comprehensive Verification

Full scenario suite covering all movement + camera behaviors.

**Test matrix:**
| Scenario | Expected result |
|----------|----------------|
| Forward walk | Player back visible, camera behind |
| Backward walk | Player front visible initially, camera swings to behind backward direction |
| Strafe left | Player faces left, camera swings behind |
| Strafe right | Player faces right, camera swings behind |
| Diagonal (forward+right) | Player faces ~45° right of original forward |
| Walk then stop | Camera and facing retain last direction |
| Idle camera rotate | Player doesn't rotate |
| Walk after idle rotate | Player faces new direction, camera follows |

- [x] 5a: Create complete scenario suite (`snapshots/scenarios/verification/`) covering the full test matrix — 8 scenarios.
- [x] 5b: Run all scenarios and verify with 3 sub-agent critics. Iterate up to 5 times until all pass.
- [x] 5c: No issues found — all 3 critics passed on first iteration (8/8 correctness, 0 regressions, 0 DRY violations).

---

## Phase 6: Fix Camera/Player Rotation — Standard 3rd-Person

**Root cause:** Phase 4 implemented camera auto-follow (camera chases behind movement direction). The correct behavior is the opposite: camera stays user-controlled, player character faces camera direction when moving.

**Desired behavior (standard 3rd-person):**
- Camera stays where the user puts it (drag/touch orbit). No auto-rotation.
- When moving (any direction), the player character faces `camera.yaw` — the camera's look direction.
- When idle, camera orbits freely; player retains last facing direction.
- Strafing: player runs sideways relative to the camera but still faces the camera's forward direction.

**What to remove:**
- `camera.follow_behind()` call in client frame loop
- `camera_follow_yaw()` in snapshot simulation loop
- Dead code: `follow_behind()` method, `camera_follow_yaw()` function, `CAMERA_FOLLOW_SPEED` constant

**What to change:**
- Client: `local_move_yaw = move_yaw(forward, strafe, camera.yaw)` → `local_move_yaw = camera.yaw`
- Server send: `move_yaw` field already receives the right value since we'll send `camera.yaw`
- Snapshot sim: `player_yaw = move_yaw(...)` → `player_yaw = orbit_yaw`

- [x] 6a: Client fix — remove `follow_behind()` from frame loop, set `local_move_yaw = camera.yaw` when moving. Restore frame loop order: movement → set facing → touch drag → update_camera → build_instances.
- [x] 6b: Snapshot sim fix — remove `camera_follow_yaw()` from tick loop, set `player_yaw = orbit_yaw` when moving. Camera orbit_yaw stays fixed.
- [x] 6c: Dead code cleanup — remove `follow_behind()` from OrbitCamera, `camera_follow_yaw()` and `move_yaw()` from movement.rs, `CAMERA_FOLLOW_SPEED` from game-core. (move_yaw was also unused — server receives the value from client, doesn't compute it).
- [ ] 6d: Update scenarios — repurpose follow_* scenarios to test "player faces camera direction". Update verification suite. Run all 3 critics to verify.
