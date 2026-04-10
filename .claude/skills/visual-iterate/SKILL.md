---
name: visual-iterate
description: Use when the user asks about visual improvements, tweaking visuals, atmosphere, lighting, colors, fog, sky, trees, rocks, terrain appearance, or any rendering/aesthetic change to the game scene. Also use when the user wants to see what the game looks like.
---

# Visual Iteration Workflow

This project has a headless snapshot CLI (`game-snapshot`) that renders the game scene to a PNG without a browser. Use it to iterate on visual changes: tweak code, render, inspect, repeat.

## Quick reference

```
# Render a single frame (defaults: 1920x1080, noon)
cargo run --release -p game-snapshot -- --output frame.png

# Render with specific settings
cargo run --release -p game-snapshot -- \
  --sun-angle 0.25 \
  --camera-pos 160,25,160 \
  --camera-target 128,15,128 \
  --width 1920 --height 1080 \
  --output frame.png

# Render all 4 times of day at once
make snapshot
# Produces: snapshots/dawn.png, snapshots/noon.png, snapshots/dusk.png, snapshots/night.png
```

**Sun angle values:** 0.0 = dawn, 0.25 = noon, 0.5 = dusk, 0.75 = night, 1.0 = dawn again.

## Iteration loop

1. Make the code change in `game-render/` (shared rendering crate)
2. Run `cargo run --release -p game-snapshot -- --output frame.png` (or `make snapshot` for all times of day)
3. Read the output PNG to inspect the result visually
4. Repeat until it looks right
5. Verify the browser client still builds: `cd game-client && wasm-pack build --target web`

The full cycle (code change → rebuild → render) takes ~2 seconds in release mode.

## Snapshot storage within a plan

When working within a plan (e.g., `plans/2026-04-10-rendering-polish.md`), store phase snapshots in a plan-specific folder:

```
snapshots/<plan-name>/phaseN/dawn.png
snapshots/<plan-name>/phaseN/noon.png
snapshots/<plan-name>/phaseN/dusk.png
snapshots/<plan-name>/phaseN/night.png
```

For example: `snapshots/2026-04-10-rendering-polish/phase9/noon.png`

This keeps snapshot history organized per-plan and avoids overwriting snapshots from other plans. Never overwrite previous phase snapshots — accumulate history so you can compare across phases.

## Architecture

- **`game-render/`** — shared rendering crate used by both browser and snapshot. All visual changes go here.
- **`game-snapshot/`** — native CLI binary that creates a headless wgpu device, renders one frame, saves a PNG.
- **`game-client/`** — browser WASM client. Imports renderers from `game-render`.

## Current tunable properties

When making visual changes, these are the key files and parameters:

### Atmosphere & lighting (`game-render/src/atmosphere.rs`)
- Sun direction, color (noon/dawn/night sun color vectors)
- Sky zenith and horizon colors (noon/dawn/night variants)
- Fog color (derived from sky horizon)
- Ambient intensity (0.15 night → 0.3 day)
- All driven by a single `sun_angle` parameter

### Terrain shading (`game-render/src/terrain.wgsl`)
- Height-based color bands: sand (< 8), grass (8–18), rock (> 18)
- Smoothstep transition ranges between biomes
- Fog distance (`fog_far` in Uniforms, currently 300.0)

### Scene objects (`game-render/src/scatter.rs`)
- Rock placement: height > 18, slope < 0.7, ~40% acceptance rate, grid step = 8
- Tree placement: height 10–17, slope < 0.4, ~35% acceptance rate, grid step = 6
- Rock colors: grey-brown with slight random variation
- Tree foliage colors: green with slight random variation
- Scale ranges: rocks 0.5–2.0, trees 1.0–2.0

### Rock mesh (`game-render/src/rocks.rs`)
- Deformed icosphere (1 subdivision, ±30% vertex displacement)

### Tree mesh (`game-render/src/trees.rs`)
- Cone foliage (radius 0.8, height 2.0) + cylinder trunk (radius 0.15, height 1.0)
- Foliage base starts at 60% trunk height

### Sky gradient (`game-render/src/sky.wgsl`)
- Fullscreen triangle at far plane
- Gradient curve: `pow(uv.y, 1.5)` from zenith to horizon

### Uniforms (`game-render/src/terrain.rs` — `Uniforms` struct)
- `fog_far`: fog distance (currently 300.0)
- All atmosphere fields are populated by `compute_atmosphere(sun_angle)`

## Self-maintenance

**This skill must be kept up to date.** When you add new visual properties, renderers, shader parameters, or CLI flags to the snapshot tool:
- Update the "Current tunable properties" section above with the new parameters
- Add new CLI flags to the "Quick reference" section if applicable
- Keep file paths accurate if files are moved or renamed
