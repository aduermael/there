# Fixes & Polish — 2026-04-14

Seven issues: spawn position, noon lighting, night tones, settings button margins, chat width, player orientation, water animation.

---

## Phase 1: DRY — Move AnimState to game-core + remove Swim

### Problem

`AnimState` enum and `from_movement()` live in `game-render/src/animation.rs`, but the server in `game-server/src/game_loop.rs:70-82` duplicates the exact same logic with raw integers — and uses a *different* walk threshold (0.3 vs `WALK_ENTER_SPEED = 0.5`). This is a bug-prone DRY violation.

At the same time, the user wants water to have no impact on movement or animation. `Swim` is the only water-related behavior — removing it cleans up both `from_movement()` and the server's duplicate.

### Codebase context

| File | Role | Lines |
|------|------|-------|
| `game-render/src/animation.rs` | AnimState enum + from_movement + to/from_u8 + walk constants | 6-8, 86-143 |
| `game-render/src/clips.rs` | swim_clip() function + clip loaded at index 5 | ~221-260 |
| `game-server/src/game_loop.rs` | Duplicate anim-state logic with raw ints | 70-82 |
| `game-client/src/lib.rs` | Imports AnimState, WALK_ENTER/EXIT_SPEED from game-render | 14, 331-343 |
| `game-core/src/lib.rs` | Shared constants (both crates depend on game-core) | 1-17 |

### Changes

- [x] 1a: Create `game-core/src/anim_state.rs`. Move `AnimState` enum (Idle, Walk, Run, Jump, Fall — **no Swim**), `from_movement(speed, vertical_velocity)` (no `y`/`water_level` params), `to_u8()`, `from_u8()` (map 5→Idle fallback), `WALK_ENTER_SPEED`, `WALK_EXIT_SPEED`. Export via `game-core/src/lib.rs`.

- [x] 1b: Update `game-render/src/animation.rs` — re-export `AnimState` from game-core, remove the enum/methods/constants. Remove `swim_clip()` from the clips vector in `AnimationPlayer::new()` (index 5). Remove `swim_clip()` function from `game-render/src/clips.rs`. Remove `water_level` parameter from any remaining call sites.

- [x] 1c: Update `game-server/src/game_loop.rs:70-82` — replace the inline `if/else` chain with `game_core::AnimState::from_movement(horiz_speed, player.vertical_velocity).to_u8()`. Update `game-client/src/lib.rs:14` — import `AnimState`, `WALK_ENTER_SPEED`, `WALK_EXIT_SPEED` from `game_core` instead of `game_render::animation`. Remove `water_level` argument from `from_movement()` call at line 331-336.

### Success criteria

- `cargo build -p game-core -p game-render -p game-server -p game-client` passes
- No duplicate animation-state logic exists
- Walking into water plays normal walk/idle animation, not swim
- Server and client agree on walk threshold (0.5)

---

## Phase 2: Player spawn & orientation

### 2a: Spawn in clear area

**Problem:** Players spawn at world center (128, 128) where terrain height is ~16.2 — inside the tree-placement range [10, 17]. Trees are visual-only (GPU compute shader), so there's no CPU-side collision, but the player appears embedded in tree geometry.

**Tree-free guarantee:** The tree compute shader (`game-render/src/trees_compute.wgsl:56-57`) only places trees where height ∈ [10, 17]. Positions with height > 17 are guaranteed tree-free.

**Files:** `game-core/src/terrain.rs`, `game-server/src/game_loop.rs:114-117`, `game-client/src/lib.rs:404-408`

**Approach:** Add `find_clear_spawn(heightmap: &[f32]) -> (f32, f32)` to `game-core/src/terrain.rs`. Search outward from world center in a grid pattern for the nearest position with height > 17.0 (above tree range). Both server and client call this instead of hardcoding center coords.

- [x] 2a: Implement `find_clear_spawn` in `game-core/src/terrain.rs`. Use in both `game-server/src/game_loop.rs` (player join spawn) and `game-client/src/lib.rs` (initial local position).

### 2b: Fix player model orientation

**Problem:** User reports player faces the camera instead of the forward direction. Bind pose is documented as "facing -Z" (`game-render/src/skeleton.rs:47`, `clips.rs:5`). The shader rotation math (`player.wgsl:49-57`) is a correct standard Y-axis rotation. The most likely cause: the model vertex data has the visual front facing +Z, contradicting the documented convention.

**Files:** `game-client/src/lib.rs:322-324` (local player instance), `game-client/src/lib.rs:374-376` (remote player instance)

**Approach:** Add `PI` to the yaw when building `PlayerInstance` data. This is a single-point fix that propagates to both main and shadow shader passes without touching shader code. Apply to both local player (line 323) and remote players (line 375).

- [x] 2b: Add `std::f32::consts::PI` to yaw in `PlayerInstance.pos_yaw[3]` for both local and remote players in `game-client/src/lib.rs`. If visual inspection shows the model was already correct, revert and investigate further.

### Success criteria

- Player spawns on a hilltop or clearing with no tree geometry overlapping
- Player model's visual front faces the movement direction, back faces the camera

---

**Parallel Phases: 3, 4**

## Phase 3: Lighting improvements

### 3a: Brighter noon

**Problem:** Noon looks too dark (screenshots confirm). Current ambient intensity peaks at only 0.22 (13% base + 9% day). Sun color at noon is [1.20, 1.10, 0.92]. Ground bounce is [0.48, 0.38, 0.18].

**File:** `game-render/src/atmosphere.rs`

| Value | Current | Issue |
|-------|---------|-------|
| `ambient_intensity` (line 80) | `0.13 + 0.09 * day_factor` → 0.22 at noon | Too low — shadows eat detail |
| `noon_sun` (line 39) | `[1.20, 1.10, 0.92]` | Could be warmer/brighter |
| `noon_ground` (line 89) | `[0.48, 0.38, 0.18]` | Weak upward bounce |

- [x] 3a: In `atmosphere.rs`, increase ambient intensity formula, boost noon sun color, and strengthen noon ground bounce. Target: visibly sunnier midday with warm light and readable shadow detail.

### 3b: Blue-toned night

**Problem:** Night is too monochrome/grey (screenshot confirms). R and G channels are nearly equal in night colors, producing grey instead of blue. Postprocess desaturation compounds this — `sat_boost` drops to 0.20 at night, stripping any remaining color.

**Files:** `game-render/src/atmosphere.rs`, `game-render/src/postprocess.wgsl`

| Value | Current | Issue |
|-------|---------|-------|
| `night_sun` (line 42) | `[0.16, 0.18, 0.36]` | R≈G → grey moonlight |
| `night_zenith` (line 52) | `[0.04, 0.06, 0.18]` | Not enough blue separation |
| `night_horizon` (line 64) | `[0.03, 0.04, 0.12]` | Desaturated |
| `night_ground` (line 88) | `[0.06, 0.07, 0.12]` | Grey |
| `sat_boost` at night (postprocess.wgsl:246) | `mix(1.28, 0.20, night_factor)` | 0.20 = nearly greyscale |
| Blue fill (postprocess.wgsl:251) | `vec3(0.010, 0.014, 0.030)` | Subtle |

- [x] 3b: In `atmosphere.rs`, shift night colors toward blue: reduce R, slightly reduce G, increase B across night_sun, night_zenith, night_horizon, night_ground. In `postprocess.wgsl`, raise the night saturation floor from 0.20 to ~0.35-0.40 to preserve blue tones. Optionally strengthen the blue shadow fill.

### Success criteria

- Noon: bright, warm, readable — impressionist sunny feel
- Night: distinctly blue-toned, not grey or monochrome

---

## Phase 4: UI layout fixes

### 4a: Settings button margins + FPS position

**Problem:** Settings button (gear icon) has inconsistent margins: `top: max(12px, env(safe-area-inset-top))` + `margin-top: 32px` ≈ 44px top, `right: max(12px, env(safe-area-inset-right))` ≈ 12px right. Chat input has `bottom: 16px`, `left: 16px`. User wants them to match. Also, FPS is top-right — user wants it top-left.

**Files:** `web/components/game-menu.js:14-32` (trigger button), `web/components/game-hud.js:5-32` (FPS + info layout)

- [x] 4a: In `game-menu.js`, simplify trigger to `top: max(16px, env(safe-area-inset-top))`, `right: max(16px, env(safe-area-inset-right))`, remove `margin-top: 32px`. In `game-hud.js`, move FPS to top-left — reorder DOM so FPS div comes before info div, or place FPS inside the left-side info block.

### 4b: Chat input wider with right margin

**Problem:** Chat input max-width is `min(500px, 85vw)`. User wants 3x wider. On small screens, 85vw doesn't enforce a right margin.

**File:** `web/components/chat-console.js:37`

- [x] 4b: Change max-width to `min(1500px, calc(100vw - 32px))`. The `calc(100vw - 32px)` guarantees 16px margin on each side regardless of screen width, replacing the arbitrary 85vw cap.

### Success criteria

- Settings gear has equal 16px margins matching chat input's 16px insets
- FPS counter is top-left
- Chat input is wider (up to 1500px), with 16px margin enforced on both sides on small screens

---

## Open questions

- **Player orientation:** The fix assumes the model's visual front faces +Z despite the -Z convention. If adding PI produces a double-flip (model was already correct), revert and investigate the actual vertex data or camera-relative expectations.
- **Night blue intensity:** Exact values need visual tuning. The plan sets direction (more blue, less grey) but final numbers require iteration.
- **Spawn search radius:** If the terrain has no height > 17 within reasonable range of center, the fallback is center (current behavior). The terrain formula suggests hilltops exist near center.
