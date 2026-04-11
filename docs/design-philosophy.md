# Visual Design Philosophy

This document captures the core visual and artistic principles guiding the game's rendering and art direction. It is a living document — update it as the style evolves.

## Core Principle: Feeling Over Fidelity

The general feeling is everything. Think the impressionism movement — mood and atmosphere matter more than individual asset detail. A scene should evoke a place and a time of day, not showcase polygon counts or texture resolution.

## Inspirations

- **Red Dead Redemption II** — still beautiful after almost 10 years because Rockstar nailed lights and ambiances, not because of high-res assets. Our north star for atmospheric rendering: god rays through trees, fog rolling through valleys, warm dusk light on terrain.
- **Valheim** — proof that low-poly + pixel textures + strong atmosphere = compelling world. Players remember the feeling of a foggy morning, not the triangle count.

## Art Style

- **Low-res pixel textures**: 16x16 and 32x32 pixel art tiles. Nearest-neighbor filtering to preserve crisp blocky edges. Charm over realism.
- **Low-poly meshes**: Simple geometry. Detail comes from lighting and atmosphere, not mesh complexity.
- **Procedural variation**: Base colors and noise-driven variation keep the world from looking tiled or repetitive without adding texture memory.

## Rendering Priorities

Techniques are evaluated on **visual impact per GPU cycle**. The budget is tight — the game must run well on mobile/web.

1. **Lighting & shadows** — the single biggest contributor to mood. Cascaded shadows, contact shadows, time-of-day color shifts.
2. **Atmosphere & fog** — depth cues, aerial perspective, volumetric light shafts. These sell the world's scale.
3. **Bloom & HDR** — the sun should glow, specular should pop, dawn/dusk should feel warm. Bloom makes HDR visible to the player.
4. **Cloud & sky** — the sky is half the screen. Layered clouds with self-shadowing, physical scattering at horizon.
5. **Water** — essential for the world to feel alive. Depth-based color, animated normals, shoreline foam.
6. **Textures & detail** — pixel art textures add tactile detail up close. Triplanar mapping avoids UV seams.

## Technical Constraints

- **Mobile/web-first**: every technique must justify its GPU cost. Target 16ms per frame at 1080p on mid-range hardware.
- **Game + engine = one**: the engine is fully hardcoded for this game. No generic abstractions needed — optimize for exactly what we render.
- **No new dependencies without approval**: rendering improvements come from shader work, not library imports.
- **WebGPU**: all rendering through the WebGPU API. Compute shaders are available and proven (grass pipeline). Use them for scattering, bloom, SSAO, and other parallel workloads.

## Anti-Patterns

- Chasing photorealism — wrong art direction, wrong performance budget.
- Adding detail through mesh complexity instead of lighting/atmosphere.
- Linear (bilinear/trilinear) filtering on pixel art textures — destroys the aesthetic.
- Generic engine abstractions "in case we need them later" — hardcode for what exists now.
