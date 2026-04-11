# Controls & UI Improvements

## Context

The game has a working third-person camera + movement system, atmosphere rendering with full day/night support, and DOM-based web components for UI. This plan adds proper camera-relative movement, jump, FPS display, daylight cycle, and an ESC menu.

### Codebase overview

- **Movement**: `game-core/src/movement.rs` — `apply_movement()` pure function, shared by client & server
- **Camera**: `game-client/src/camera.rs` — `OrbitCamera` with yaw/pitch/distance, eye = target + spherical offset
- **Input**: `game-client/src/input.rs` — `InputState` (WASD + joystick), thread-local `JOY_INPUT`
- **Game loop**: `game-client/src/lib.rs` — ~200-line render closure, `GameState` struct (11 fields)
- **Server loop**: `game-server/src/game_loop.rs` — 20Hz tick calling `apply_movement` per player
- **Protocol**: `game-core/src/protocol.rs` — `ClientMsg::Input { forward, strafe, yaw }`, `PlayerState { id, x, y, z, yaw }`
- **Atmosphere**: `game-render/src/atmosphere.rs` — `compute_atmosphere(sun_angle)` already supports full 0-1 cycle
- **UI**: `web/components/` — shadow DOM web components: `game-hud.js`, `connect-screen.js`, `virtual-joystick.js`, `camera-control.js`
- **JS bridge**: `#[wasm_bindgen(inline_js)]` pattern for WASM-to-DOM calls

---

## Phase 1: Refactor — render loop extraction & DRY fixes

Before adding features, make the game loop manageable and fix existing DRY issues.

- [x] 1a: Extract render loop into GameState methods — `process_server_messages()`, `update_movement()`, `build_player_instances()`, reducing the closure from ~200 lines to ~50 lines of orchestration
- [x] 1b: Fix spawn position DRY — client hardcodes `128.0, 128.0` instead of `WORLD_SIZE / 2.0`; use the constant like the server does

### Contracts
- `process_server_messages(&mut self, messages: Vec<ServerMsg>, now: f64)` — handles Welcome, Snapshot (with correction), PlayerLeft
- `update_movement(&mut self, dt: f32)` — reads input, calls `apply_movement`, updates camera target
- `build_player_instances(&mut self, now: f64)` — rebuilds `self.players` vec from local + interpolated remotes
- Render loop becomes: process messages -> update movement -> build instances -> compute uniforms -> render

### What NOT to change
- `apply_movement` function signature — leave untouched
- `OrbitCamera`, `Renderer`, `Connection` — stable, no changes needed
- Web component files — not touched in this phase

---

## Phase 2: Camera-relative movement fix

**Root cause**: Sign error in `game-core/src/movement.rs` lines 23-24.

The camera eye is at `target + (dist*cos(pitch)*sin(yaw), dist*sin(pitch), dist*cos(pitch)*cos(yaw))`. The camera looks from eye toward target, so its forward direction in the XZ plane is `(-sin(yaw), -cos(yaw))`. But the movement code computes forward as `(+sin(yaw), -cos(yaw))` — the X component is negated. This is a reflection, not a rotation. It coincidentally works at yaw=0 (since sin(0)=0) but diverges at all other angles.

- [x] 2a: Fix the movement rotation matrix — two sign changes in `apply_movement`:
  - `move_x = sin_yaw * forward` becomes `move_x = -sin_yaw * forward`
  - `move_z = sin_yaw * strafe` becomes `move_z = -sin_yaw * strafe`
  - Resulting: `move_x = -sin_yaw * forward + cos_yaw * strafe; move_z = -cos_yaw * forward - sin_yaw * strafe`
  - Fix lives in `game-core`, automatically applies to both client and server

### Verification
- yaw=0: forward=(0, -1) matches camera facing -Z. Right=(1, 0) is +X. Correct.
- yaw=pi/2: forward=(-1, 0) matches camera facing -X. Right=(0, -1) is -Z. Correct.
- yaw=pi/4: forward=(-0.707, -0.707), right=(0.707, -0.707). Correct diagonal.

### Failure mode
- If the sign fix is wrong, all movement will feel inverted. Quick visual test: press W, character should move away from camera toward where camera looks.

---

## Phase 3: FPS display

Purely additive, no existing code modified beyond extending game-hud.

- [x] 3a: Extend `game-hud.js` to full-width top bar — change `:host` to span `left: 0; right: 0`, use flexbox `justify-content: space-between`. Add `.fps` div on right side with `set fps(value)` setter. Same style (white, text-shadow, system-ui, pointer-events: none).
- [x] 3b: Add FPS calculation and bridge — add `frame_count: u32` and `fps_accum: f32` to `GameState`. Each frame: accumulate. Every 0.5s: compute `fps = frame_count / fps_accum`, call `hud_set_fps(fps)`, reset. Add `hud_set_fps` to existing `inline_js` block.

### Design
- Rolling 0.5s average — stable enough to read, responsive enough to notice drops
- Top-right positioning respects `env(safe-area-inset-*)` for mobile notch
- Subtle, non-intrusive — matches existing HUD style

---

## Phase 4: Daylight cycle

Replace the hardcoded noon atmosphere with an advancing day/night cycle.

- [x] 4a: Add daylight state and cycle logic — add `sun_angle: f32` (init 0.0 = dawn) and `cycle_active: bool` (init true) to `GameState`. Each frame when active: `sun_angle = (sun_angle + dt / 120.0) % 1.0`. Replace `compute_atmosphere(0.25)` with `compute_atmosphere(state.sun_angle)`. Add `DAYLIGHT_CYCLE_SECS: f32 = 120.0` constant to `game-core/src/lib.rs`.
- [x] 4b: Add JS bridge for daylight state — expose `window.__daylightCycle` (bool) and `window.__sunAngle` (f32) as window globals. WASM reads/writes these each frame. When cycle is active, WASM advances and writes back to `__sunAngle`. When cycle is paused, WASM reads `__sunAngle` as the fixed value. Add inline_js bridge functions: `js_is_daylight_cycle()`, `js_get_sun_angle()`, `js_set_sun_angle()`.

### Timing
- 120 seconds per full cycle = 24 hours in 2 minutes
- Starting at dawn (0.0) gives a nice sunrise experience on load
- `compute_atmosphere` already handles the full range with proper dawn/dusk/night transitions

---

## Phase 5: ESC menu

New web component with daylight controls and input pausing.

- [x] 5a: Create `game-menu.js` web component — new file `web/components/game-menu.js`, add to `index.html`. Shadow DOM with:
  - Persistent gear icon trigger (top-right, below FPS, 44px+ tap target for mobile, pointer-events: auto)
  - Full-screen semi-transparent overlay (hidden by default, z-index: 50)
  - Centered panel with: "Daylight Cycle" toggle (default ON), time-of-day presets (Dawn/Noon/Dusk/Night buttons), "Resume" close button
  - ESC key listener on window to toggle open/close
  - Sets `window.__menuOpen = true/false` on open/close
  - Daylight toggle sets `window.__daylightCycle`; presets set `window.__sunAngle` and pause cycle
- [x] 5b: Add menu-aware input pausing in WASM — add `js_is_menu_open()` inline_js bridge. In game loop, when menu is open: zero out movement input (forward=0, strafe=0, no jump). Camera drag is naturally blocked by overlay's pointer-events. Rendering continues unpaused.

### Input flow when menu is open
- ESC key handled in JS (game-menu component) — toggles overlay, sets `window.__menuOpen`
- WASM reads `__menuOpen` each frame and suppresses movement
- Pointer events absorbed by overlay — camera drag, joystick naturally blocked
- Game continues rendering — player sees the scene behind the semi-transparent backdrop

### Mobile
- Gear icon always visible for touch users (no ESC key on mobile)
- Touch-friendly button sizes (min 44px)
- Safe-area insets on trigger positioning

---

## Phase 6: Jump

Most complex change — touches shared crate, protocol, and both client/server.

- [x] 6a: Add `apply_vertical` to game-core — new pure function in `movement.rs`: `pub fn apply_vertical(y: f32, velocity: f32, terrain_y: f32, jump_pressed: bool, dt: f32) -> (f32, f32)`. Applies gravity (`GRAVITY = -20.0`), integrates velocity, initiates jump when on ground and jump_pressed (`JUMP_VELOCITY = 8.0`), lands when `y <= terrain_y`. Returns `(new_y, new_velocity)`. Add constants to `game-core/src/lib.rs`.
- [x] 6b: Add jump input handling — add `space: bool` to `InputState` with `on_key_down`/`on_key_up` for "Space". Add `pub fn jump_pressed(&self) -> bool` method. For mobile: add a jump button web component (`web/components/jump-button.js`) that calls `window.onJumpPressed()` → WASM `on_jump_pressed()` (thread-local flag, same pattern as joystick).
- [x] 6c: Add jump to protocol — add `jumping: bool` to `ClientMsg::Input`. Add `input_jump: bool` and `vertical_velocity: f32` to server's `Player` struct. Update `RoomEvent::Input` to include `jump: bool`.
- [x] 6d: Integrate jump in client game loop — add `vertical_velocity: f32` to `GameState`. After XZ movement: call `apply_vertical(pos.y, velocity, terrain_y, jump_pressed, dt)`, update pos.y and velocity. Send `jumping` in `ClientMsg::Input`. Consume `jump_pressed` flag after use (set to false so holding space doesn't re-trigger). Ensure movement runs every frame when airborne (not just when forward/strafe != 0), so gravity applies.
- [ ] 6e: Integrate jump in server game loop — each tick per player: call `apply_vertical` after `apply_movement`. Set `JUMP_VELOCITY` on `vertical_velocity` when `input_jump` is true and player is grounded. Consume `input_jump` after use.

### Physics values
- `GRAVITY = -20.0` units/s^2 — strong enough to feel grounded, not floaty
- `JUMP_VELOCITY = 8.0` units/s — apex height ~1.6 units, air time ~0.8s
- XZ movement works while airborne (standard for the genre)
- Remote player jumps visible via existing Y interpolation in snapshots — no changes needed

### Client-server sync
- Client predicts locally (immediate response)
- Server runs same `apply_vertical` at 20Hz (authoritative)
- Existing snap-correction (delta > 5m: snap, 0.1-5m: blend 30%) handles Y discrepancies
- `jump_pressed` flag latches true until consumed by send loop — quick taps aren't lost between 50ms sends

### Failure modes
- Jump desync: handled by existing correction system
- Double-jump: prevented by `y <= terrain_y + epsilon` check
- Gravity while standing still: only runs when airborne (`velocity != 0.0`), static terrain won't cause issues

---

## Open questions

1. Should the daylight cycle start at dawn (0.0) for a sunrise experience or noon (0.25) for immediate visibility? (Plan assumes dawn)
2. Should the ESC menu include any other options beyond daylight controls? (e.g., camera sensitivity, sound)
3. Mobile jump button placement — bottom-right? Should it be part of the virtual joystick area?

## Success criteria

- Pressing W moves the character in the direction the camera faces at all yaw angles
- Space triggers a visible arc jump that lands back on terrain
- FPS counter displays smoothly in top-right, updating every ~0.5s
- Scene starts with dynamic day/night cycle (2-minute full rotation)
- ESC opens/closes a menu overlay that pauses movement and allows daylight control
- All features work on both desktop (keyboard+mouse) and mobile (touch+joystick)
- Server correctly validates movement and jump (authoritative snapshots)
- Codebase remains clean: render loop is readable, no DRY violations, no new dependencies
