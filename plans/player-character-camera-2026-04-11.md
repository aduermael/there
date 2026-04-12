# Player Character & Camera Overhaul

---

## Context

### Current State
- **Player visual**: Procedural capsule (radius 0.3, height 1.2, 12 segments). Rendered as the visible model. Uses `compute_flat_normal()` from screen derivatives — creates faceted shading and interacts poorly with SSAO (dark halos at terrain contact). No per-vertex normals.
- **Collision**: Not separate from rendering. Player Y is snapped to terrain via `sample_height()`. The capsule shape is implicit — only the center point is tracked. Capsule dimensions (`CAPSULE_RADIUS`, `CAPSULE_CYL_HEIGHT`) only affect visuals, not physics.
- **Camera**: `OrbitCamera` in `game-client/src/camera.rs`. Spherical coordinates (yaw, pitch, distance). Terrain collision clamps `eye.y` to `terrain_y + 2.0` at the eye's XZ position. This causes the "can't look up" problem — as pitch increases, eye XZ drifts and hits nearby terrain. `MAX_DISTANCE = 200` (far too large for a 3rd person game). No smoothing on any camera parameter.
- **Protocol**: `PlayerState { id, x, y, z, yaw }` — no animation state.
- **Skeleton/Animation**: None. Vertex shader applies yaw rotation only.

### Goals
1. Replace capsule visual with a crash-test-dummy humanoid (procedurally generated, standard bone rig)
2. Keep capsule as invisible collider (collision logic unchanged, just stop rendering capsule)
3. Fix capsule shadow/AO artifacts (the humanoid replaces it; per-vertex normals fix SSAO)
4. Fix 3rd-person camera (terrain collision, smoothing, zoom limits)
5. Add skeletal animation (walk, run, idle, jump, swim) with network sync
6. No new dependencies

### Files Involved
- **Camera**: `game-client/src/camera.rs` (~112 lines)
- **Player rendering**: `game-render/src/player.rs` (~188 lines), `game-render/src/player.wgsl` (~44 lines)
- **Instanced mesh**: `game-render/src/instanced_mesh.rs` (~97 lines)
- **Frame pipeline**: `game-render/src/frame.rs` (player draw call, shadow pass)
- **Game loop**: `game-client/src/lib.rs` (camera integration, player instance assembly)
- **Protocol**: `game-core/src/protocol.rs`, `game-server/src/room.rs`, `game-server/src/game_loop.rs`
- **Exports**: `game-render/src/lib.rs`

---

## Phase 1: Camera Overhaul

Fix the camera before touching the player model. Immediately improves gameplay and is fully independent.

- [x] 1a: Raycast terrain collision — replace the `eye.y = max(eye.y, terrain_y + MIN_HEIGHT)` clamp with a ray-based approach: cast a ray from `target` (player) toward `raw_eye()`, sample terrain at intervals along the ray, pull camera closer if any sample is above the ray. This lets you look up freely because the ray stays above terrain rather than clamping the endpoint. Add a small clearance buffer (1.5–2m) above terrain along the entire ray path.
- [x] 1b: Smooth camera distance — when terrain collision forces the camera closer, lerp the effective distance toward the collision distance (fast approach ~10/s, slow recovery ~3/s). Store `effective_distance` separately from `desired_distance` (what the player set via scroll). This prevents jarring pops when orbiting around hills.
- [ ] 1c: Tighten zoom limits — reduce `MAX_DISTANCE` to ~20 (world is 256×256, 20m orbit is generous). Reduce `MIN_DISTANCE` to ~3 for close-up view. Adjust `ZOOM_SPEED` proportionally for the smaller range.
- [ ] 1d: Target offset — add a small upward offset to the camera target point (e.g. +1.0 Y above player feet). Currently the camera orbits the player's foot position, which makes looking up feel cramped. The target should be roughly chest height of the humanoid (~1.2m above ground).

## Phase 2: Skeleton & Humanoid Mesh

Build the bone system and procedural mesh. This is the foundation for animation.

- [ ] 2a: Bone hierarchy — define a standard humanoid skeleton in a new `game-render/src/skeleton.rs` module. ~15 bones: hips (root), spine, chest, neck, head, upper_arm_L/R, lower_arm_L/R, upper_leg_L/R, lower_leg_L/R, foot_L/R. Each bone stores: parent index, bind-pose local transform (translation + rotation as quat). Provide `compute_world_matrices()` that walks the hierarchy and produces `[Mat4; N]` world-space bone matrices. Use glam `Quat` and `Mat4` — no new deps.
- [ ] 2b: Procedural humanoid mesh — replace `generate_capsule()` with `generate_humanoid()` in player.rs. Build a crash-test-dummy from simple primitives: ellipsoid head, box torso, cylinder limb segments, small box feet. Each vertex gets: `position: vec3`, `normal: vec3`, `bone_index: u32`. Distinct colors per body region (yellow body, black joints — classic dummy look) encoded via bone index in the fragment shader. Total geometry target: ~400–600 vertices.
- [ ] 2c: GPU skinning — create a storage buffer for bone matrices (MAX_PLAYERS × NUM_BONES × mat4). Update player.wgsl vertex shader to read bone matrix from storage buffer using `instance_index` and vertex `bone_index`, then apply: `world_pos = bone_matrix * local_pos`. Add proper normals: pass `normal` from vertex buffer, transform via bone matrix, use in fragment shader instead of `compute_flat_normal()`. Update the pipeline layout to add the bone storage buffer bind group.
- [ ] 2d: Integrate humanoid into renderer — update `PlayerRenderer` to use the new mesh and skinning pipeline. Update `PlayerInstance` to carry animation pose data (or just index into the bone buffer). Each frame, compute bone matrices on CPU (from animation state), upload to storage buffer via `queue.write_buffer()`. Remove the old capsule mesh generation code. Verify humanoid renders correctly in T-pose.

## Phase 3: Animation System

Animate the humanoid. Client-side animation driven by movement state.

- [ ] 3a: Animation data structures — in `game-render/src/animation.rs`, define: `AnimationClip` (name, duration, looping flag, per-bone keyframe tracks), `BoneKeyframe` (time, rotation as quat, optional translation), `AnimationState` (current clip, elapsed time, playback speed). Implement `sample_clip(clip, time) -> Vec<Quat>` that interpolates between keyframes using `Quat::slerp`.
- [ ] 3b: Procedural animation clips — create basic clips in Rust code (no asset files): `idle` (subtle breathing sway), `walk` (standard bipedal walk cycle, arms swing opposite to legs), `run` (faster, wider stride, more arm pump), `jump` (arms up, legs tucked), `fall` (arms out, legs extended), `swim` (breaststroke-like arm motion, kick). Each clip is a function returning `AnimationClip` with hand-tuned keyframe rotations. ~4–8 keyframes per bone per clip.
- [ ] 3c: Animation state machine — define `AnimState` enum: `Idle, Walk, Run, Jump, Fall, Swim`. Derive state from movement inputs: `speed > 0 → Walk/Run`, `vertical_velocity > 0 → Jump`, `vertical_velocity < 0 → Fall`, `y < WATER_LEVEL → Swim`, else `Idle`. Add crossfade blending between states (blend two poses by lerping bone rotations over ~0.2s transition). Store `AnimationPlayer` per player instance (current state, blend state, elapsed times).
- [ ] 3d: Client-side animation playback — in the game loop, each frame: determine `AnimState` for local player from movement/physics state, advance animation time by dt, compute blended bone rotations, feed into `compute_world_matrices()`, upload resulting bone matrices to GPU. For remote players: derive `AnimState` from interpolated velocity (diff of interpolated positions between frames).

## Phase 4: Network Sync & Polish

Sync animation state across network. Clean up artifacts.

- [ ] 4a: Server-side animation state — in `game-server/src/room.rs`, add `anim_state: u8` to `Player` struct. In `game_loop.rs`, derive animation state each tick from player movement (same logic as client: speed, vertical_velocity, y vs water level). Cheap — just an enum discriminant, no heavy computation.
- [ ] 4b: Extend protocol — add `anim_state: u8` to `PlayerState` in `protocol.rs`. Update server snapshot building to include it. Update client `RemotePlayer` to store and interpolate animation state (discrete — use the latest received state, don't interpolate between states). Backward compatible: new field with sensible default.
- [ ] 4c: Remote player animation — on the client, use received `anim_state` to drive animation for remote players instead of velocity-derived state. This ensures all clients see the same animation state as the server's authoritative view. Retain velocity-based fallback for the local player (immediate responsiveness without network round-trip).
- [ ] 4d: Final cleanup — remove any remaining capsule rendering code. Verify no shadow/AO artifacts from player model (per-vertex normals should fix SSAO quality). Verify player model renders correctly in all 4 snapshot times of day. Clean up any dead code from the capsule system.

---

## Open Questions

- **Bone count**: 15 bones is standard minimum for humanoid. Could go to ~20 (add hands, toes) if animation quality demands it. Start with 15, expand if needed.
- **Animation quality vs. complexity**: Procedural walk/run cycles are notoriously hard to make look good. Start simple (blocky, robotic — fits the crash-test-dummy aesthetic), iterate visually.
- **Swim detection**: Currently just `y < WATER_LEVEL`. May need a more nuanced check (e.g., only when submerged past waist height). Decide during implementation.
- **Player shadow casting**: Currently players don't cast shadows (not in shadow pass). The humanoid could be added to shadow passes — decide based on visual impact vs. performance cost.

## Success Criteria

- Camera freely looks up/down without terrain collision fighting. Smooth zoom transitions.
- Cannot zoom out past ~20m. Camera target is at chest height, not feet.
- Humanoid crash-test-dummy visible in all lighting conditions, properly lit with per-vertex normals.
- Walk, run, idle, jump, swim animations play based on movement state.
- Remote players animate correctly based on server-broadcast animation state.
- No visual artifacts from old capsule (no weird AO halos, no phantom shadows).
- No new crate dependencies added.
- Code is clean: skeleton, animation, mesh generation in well-separated modules.
