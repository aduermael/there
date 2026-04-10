# Visual Iteration Loop — Headless Frame Capture + Scene Foundation

**Date**: 2026-04-10
**Goal**: One CLI command renders the game scene to a PNG. Modify engine/settings, re-run, compare. This is the foundation for agentic visual iteration — an AI agent (or human) can close the loop: tweak parameters, render, inspect, repeat until the ambiance is right across all times of day.

**Builds on**: Phase 1–5 POC (multiplayer terrain + capsule players, all complete)

---

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                    game-core/                         │
│  Shared logic: terrain heightmap, movement, protocol │
└────────────┬──────────────────┬──────────────────────┘
             │                  │
      ┌──────▼──────┐   ┌──────▼──────┐
      │ game-render │   │             │
      │ Shared GPU  │   │             │
      │ pipelines   │   │             │
      └──┬──────┬───┘   │             │
         │      │       │             │
   ┌─────▼──┐ ┌▼───────▼──┐  ┌──────▼──────┐
   │ game-  │ │ game-     │  │ game-server │
   │snapshot│ │ client    │  │ (unchanged) │
   │ Native │ │ WASM      │  │             │
   │ CLI    │ │ Browser   │  │             │
   └────────┘ └───────────┘  └─────────────┘
```

`game-render` is a new workspace crate containing the platform-agnostic wgpu rendering code (terrain pipeline, player pipeline, shaders, uniforms, scene objects). Both the browser client and the native snapshot CLI use it.

`game-snapshot` is a new native CLI binary. It creates a headless wgpu device (Metal on macOS, Vulkan on Linux), renders one frame to an offscreen texture, reads the pixels back, and saves a PNG. No window, no browser.

---

## Codebase Context

**Files that move to `game-render`:**
- `game-client/src/terrain.rs` — `TerrainRenderer`, `Uniforms`, `create_depth_texture`, mesh generation, frustum culling. Already pure wgpu, no web_sys.
- `game-client/src/player.rs` — `PlayerRenderer`, `PlayerInstance`, capsule generation. Already pure wgpu, no web_sys.
- `game-client/src/terrain.wgsl` and `player.wgsl` — embedded via `include_str!`.

**Files that stay in `game-client`:**
- `renderer.rs` — browser surface creation (`HtmlCanvasElement`), present logic. Becomes a thin shell that delegates to `game-render` types.
- `lib.rs` — game loop (requestAnimationFrame), input, networking. Imports from `game-render` instead of local modules.
- `camera.rs`, `input.rs`, `net.rs` — browser-specific, unchanged.

**Key contract:** `TerrainRenderer::new()` and `PlayerRenderer::new()` take `&wgpu::Device`, `wgpu::TextureFormat`, and `&wgpu::BindGroupLayout` — no platform types. They compile on both WASM and native targets without changes.

---

## Success Criteria

1. `cargo run -p game-snapshot -- --output frame.png` produces a valid PNG of the terrain scene
2. Changing a shader constant (e.g., fog distance, sun direction) and re-running produces a visibly different PNG
3. CLI accepts `--sun-angle` (0.0–1.0) and produces different lighting/atmosphere for dawn, noon, dusk
4. Browser client still works identically after the refactor
5. The full render-inspect cycle (build + render) completes in under 10 seconds on a dev machine

---

## Phase 1: Extract shared rendering crate (`game-render`)

Move platform-agnostic rendering code into a new workspace crate. This is a pure refactor — no visual changes.

**What moves:**
- `Uniforms` struct (currently in `terrain.rs`)
- `TerrainRenderer` — pipeline creation, mesh generation, frustum culling, draw
- `PlayerRenderer` — capsule generation, instanced rendering, draw
- `create_depth_texture` helper
- Both `.wgsl` shader files

**What stays in `game-client/src/renderer.rs`:**
- Surface creation from `HtmlCanvasElement`
- Heightmap texture upload
- `render()` method that calls `surface.get_current_texture()` + `output.present()`
- But it now holds `game_render::TerrainRenderer` and `game_render::PlayerRenderer` instead of local types

**Failure modes:**
- `include_str!` paths break when shaders move — the macro uses paths relative to the source file, so shaders must be in or under `game-render/src/`
- `log::info!` in terrain/player renderers needs `log` as a dependency of `game-render`
- `bytemuck` derive macros on `Uniforms` and `PlayerInstance` — need `bytemuck` dep in `game-render`

- [ ] 1a: Create `game-render` crate with terrain renderer, player renderer, shaders, and shared types
- [ ] 1b: Refactor `game-client` to depend on `game-render`, verify browser build (`wasm-pack build`)

---

## Phase 2: Headless snapshot CLI (`game-snapshot`)

New native binary crate. Renders one frame to an offscreen RGBA8 texture, copies pixels to a staging buffer, reads them back, saves as PNG.

**Native wgpu headless rendering approach:**
1. Create `wgpu::Instance` with native backends (Vulkan/Metal/DX12 — no `webgpu` feature)
2. `request_adapter()` with no compatible surface (headless)
3. Create offscreen render target: `TextureUsages::RENDER_ATTACHMENT | COPY_SRC`
4. Run the same render pass as the browser client (terrain + players via `game-render`)
5. `encoder.copy_texture_to_buffer()` → map staging buffer → read pixels
6. Save via `image` crate (`image::save_buffer()` with RGBA PNG)

**CLI interface (via `clap`):**
- `--width <u32>` / `--height <u32>` — output resolution (default: 1920x1080)
- `--camera-pos <x,y,z>` — camera eye position (default: 160,25,160)
- `--camera-target <x,y,z>` — look-at point (default: 128,15,128)
- `--sun-angle <0.0-1.0>` — time of day, drives sun direction + sky/fog color (default: 0.5 = noon)
- `--output <path.png>` — output file path (default: `frame.png`)

**Failure modes:**
- Native wgpu may pick a different texture format than browser WebGPU. The renderers accept `surface_format` as a parameter, so pass `Rgba8UnormSrgb` explicitly for the offscreen target.
- `copy_texture_to_buffer` requires the buffer size to be aligned to `COPY_BYTES_PER_ROW_ALIGNMENT` (256 bytes). Must pad row stride.
- CI/headless Linux needs a Vulkan-capable GPU or `llvmpipe` (Mesa software renderer). Document this.
- Async `buffer.map_async()` — use `pollster::block_on` for native.

- [ ] 2a: Create `game-snapshot` crate with native wgpu headless device + offscreen render target
- [ ] 2b: Wire up `game-render` pipelines, render terrain to offscreen texture
- [ ] 2c: Pixel readback (copy texture → staging buffer → map → read) and PNG save
- [ ] 2d: CLI argument parsing (`clap`), sun-angle → uniforms, verify end-to-end: one command produces a PNG

---

## Phase 3: Scene objects — rocks and trees

Low-poly procedural meshes scattered on the terrain. Implemented in `game-render` so both browser and snapshot see them.

**Rocks:**
- Deformed icosphere: start from regular icosphere (1 subdivision ≈ 42 vertices), randomly displace vertices by ±30% along normals using a deterministic seed per rock variant
- 3–4 size variants, 3–4 shape variants (seed-based)
- Grey-brown color, slightly randomized per instance
- Placed on terrain where height > 18 (rocky/mountain zones)

**Trees:**
- Cone (foliage) + narrow cylinder (trunk) — 2 meshes drawn together
- ~40 triangles per tree total
- Green foliage with slight color variation, brown trunk
- Placed on terrain where height is 10–17 (grass zones), avoiding steep slopes

**Rendering approach:**
- Same instanced rendering pattern as players: one draw call per mesh type, per-instance position/color/scale
- Scatter positions computed deterministically from heightmap (hash grid cells → placement candidates → filter by height/slope rules)
- Instance data baked at init time (not per-frame) — static objects don't move

**Object counts (performance budget):**
- ~200 rocks, ~300 trees — well within the triangle budget
- 2 additional draw calls (rocks + trees)

- [ ] 3a: Procedural rock mesh generation + instanced rock renderer in `game-render`
- [ ] 3b: Procedural tree mesh generation + instanced tree renderer in `game-render`
- [ ] 3c: Deterministic scatter placement system (height/slope rules, seed-based)
- [ ] 3d: Integrate into both snapshot CLI and browser client, verify visually

---

## Phase 4: Atmosphere and lighting system

Time-of-day driven atmosphere. A single `sun_angle` parameter (0.0 = dawn, 0.25 = noon, 0.5 = dusk, 0.75 = night, 1.0 = dawn again) controls everything.

**Derived parameters (computed in `game-render`):**
- **Sun direction**: orbits on an east-west arc. At noon the sun is high (elevation ~70°), at dawn/dusk it's near the horizon.
- **Sun color**: warm orange at dawn/dusk, white at noon, dim blue at night
- **Sky color**: gradient from zenith to horizon. Dawn: orange-pink horizon, blue zenith. Noon: bright blue. Dusk: deep orange-red horizon. Night: dark blue.
- **Fog color**: matches sky at the horizon line (what you'd see looking at the far distance)
- **Ambient intensity**: 0.15 at night, 0.3 at day, smooth transitions at dawn/dusk

**Shader changes:**
- Background (clear color) replaced by sky gradient: `mix(zenith_color, horizon_color, pow(1.0 - abs(view_dir.y), 3))`
- Fog color uses the horizon sky color instead of a constant
- Diffuse lighting uses sun color as a multiplier
- Add new uniform fields: `sun_color`, `sky_zenith`, `sky_horizon`, `ambient_intensity`

**Open questions:**
- Should the sky be a fullscreen quad shader pass or just the clear color + fog blending? A fullscreen quad gives a proper gradient but adds one draw call. Worth it for the visual quality.
- Night rendering: do we dim everything or add a moon direction? Start simple — just dim ambient + dark blue sky.

- [ ] 4a: Atmosphere parameter computation from `sun_angle` (sun direction, colors, ambient) in `game-render`
- [ ] 4b: Update terrain + player shaders for dynamic sun color, ambient, and sky-aware fog
- [ ] 4c: Sky gradient rendering (fullscreen quad or clear-color approach)
- [ ] 4d: Integrate in both browser (default noon) and snapshot CLI (already has `--sun-angle`), verify across dawn/noon/dusk/night

---

## Phase 5: Verify the agentic iteration workflow

End-to-end validation that the render loop is fast, scriptable, and produces meaningful visual differences.

- [ ] 5a: Makefile target `make snapshot` that builds + runs `game-snapshot` for multiple times of day (dawn, noon, dusk, night → 4 PNGs)
- [ ] 5b: Verify iteration speed: modify a visual parameter (e.g., fog distance, tree density, sun color), re-run `make snapshot`, confirm the output changes. Document the workflow.
