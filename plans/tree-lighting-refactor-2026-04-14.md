# Tree Lighting Refactor — 2026-04-14

Noon looks unrealistic because trees use screen-space derivative normals with an upward bias hack (`abs(y)+0.7`), making them uniformly glowing instead of properly shaded. Additionally, per-object color multipliers (trees ×1.7, rocks ×1.8, terrain ×1.0) create visual inconsistency that worsens at noon when postprocess saturation (×1.28) amplifies the differences.

Dawn/dusk/night look good because low sun angles and reduced saturation hide these issues. Noon exposes them.

---

## Phase 1: Proper per-vertex normals for trees

### Problem

Trees are 3 stacked cones + cylinder trunk generated in `generate_tree_mesh()` (trees.rs:346-420). The vertex format is `[x, y, z, r, g, b]` (24 bytes) — **no normals**. The fragment shader (trees.wgsl:101-102) reconstructs normals via `compute_flat_normal` (screen-space derivatives) then applies a hack:
```
let n = normalize(vec3(flat_n.x, abs(flat_n.y) + 0.7, flat_n.z));
```
This forces all normals upward, eliminating the natural shading that gives cones depth and form. At noon with overhead sun, every face gets nearly the same lighting → flat, glowing appearance.

### Contrast with terrain (correct)

Terrain (terrain.wgsl:39-45) computes proper per-vertex normals from heightmap finite differences → natural shading at all times of day.

### Fix

Extend the tree vertex to include pre-computed geometric normals. For cone geometry, the outward normal at any ring vertex is analytically trivial.

### Files

| File | Changes |
|------|---------|
| `game-render/src/trees.rs` | Extend `TreeVertex` to 9 floats `[x,y,z, nx,ny,nz, r,g,b]`. Compute cone outward normals analytically in `generate_tree_mesh()`. Update vertex buffer layout (stride 24→36, add normal attribute at location 1, shift color to location 2). |
| `game-render/src/trees.wgsl` | Add `normal: vec3<f32>` to `VertexInput` (location 1), shift `vert_color` to location 2. Add `normal` to `VertexOutput`. In vertex shader, rotate normal by instance yaw+scale and pass through. In fragment shader, replace `compute_flat_normal` + hack with `normalize(in.normal)`. |

### Normal computation for each mesh part

- **Cone ring vertices**: `radial = normalize(x, 0, z)`, `normal = normalize(radial + up * (radius/height))` — the up component accounts for cone slope
- **Cone tip vertex**: average of surrounding ring normals, or simply `(0, 1, 0)`
- **Cone base cap center**: `(0, -1, 0)` (downward-facing)
- **Cylinder trunk**: `normalize(x, 0, z)` (purely radial)

### Success criteria

- Trees show proper faceted shading: sunlit faces bright, opposite faces in shadow
- No per-object normal hacks in the shader
- Dawn/dusk/night still look good (natural shading works at all sun angles)

- [ ] 1a: Extend `TreeVertex` to include normals, compute cone/cylinder normals analytically in `generate_tree_mesh()`, update vertex buffer layout in `trees.rs`

- [ ] 1b: Update `trees.wgsl` — add normal to vertex input/output, rotate normal in vertex shader, use `normalize(in.normal)` in fragment shader. Remove `compute_flat_normal` hack. Update shadow vertex shader input if needed.

---

## Phase 2: Harmonize object color multipliers

### Problem

Per-object shaders apply inconsistent color boosts BEFORE lighting:

| Object | Boost | File:Line |
|--------|-------|-----------|
| Trees | `* 1.7` | trees.wgsl:109 |
| Rocks | `* 1.8` | rocks.wgsl:65 |
| Terrain | none | terrain.wgsl |
| Grass | `* (0.9 + 0.2 * grad)` ≈ ×1.0 | grass.wgsl:74 |

These multipliers were tuned for the old (broken) lighting. With proper tree normals and correct hemisphere lighting, they distort the lighting ratios: a ×1.8 rock next to a ×1.0 terrain tile looks wrong even though they receive identical light.

### Fix

Remove the raw multipliers from trees and rocks. If objects appear too dark after removal (because atlas textures are dark), adjust the texture mix ratio instead — `mix(vertex_color, texture, t)` with a tuned `t` gives correct brightness without breaking lighting ratios.

### Files

| File | Change |
|------|--------|
| `game-render/src/trees.wgsl` | Remove `* 1.7`, adjust mix ratio if needed |
| `game-render/src/rocks.wgsl` | Remove `* 1.8`, adjust mix ratio if needed |

### Success criteria

- All objects lit consistently at noon — no object type "pops" or "sinks" relative to others
- Dawn/dusk/night still look natural
- No per-object color multiplier hacks

- [ ] 2a: Remove `* 1.7` from trees.wgsl and `* 1.8` from rocks.wgsl. Adjust `mix()` ratios if objects appear too dark without the boost. Verify all 4 times of day with snapshots + critics.

---

## Open questions

- **Texture atlas brightness**: If atlas textures are calibrated dark (expecting the ×1.7/×1.8 boost), removing multipliers will darken objects. May need to re-author textures or adjust the mix ratio. Snapshots will tell.
- **Shadow vertex shader**: The tree shadow shader (inline in trees.rs) reads vertex attributes — needs to match the new layout (position at location 0, skip normal at 1, etc.)
