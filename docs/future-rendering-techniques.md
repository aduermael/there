# Future Rendering Techniques

Techniques NOT in the current rendering upgrade plan. Each entry describes what it is, when it would make sense to add, and rough effort. Ordered by likely priority.

---

## Tiled / Clustered Forward Lighting

**What**: Divide the screen into tiles (or 3D froxels) and assign point/spot lights to each. Shade only with lights that affect each tile, enabling dozens of dynamic lights without per-pixel cost explosion.

**When**: When the game adds torches, campfires, or other placed light sources. Currently there's only one directional light (the sun) — no benefit until point/spot lights exist.

**Effort**: Medium-high. Requires a compute pass to build the light tile list, a storage buffer of light data, and changes to all geometry fragment shaders to loop over per-tile lights.

---

## Screen-Space Reflections (SSR)

**What**: Ray-march the depth buffer in screen space to find reflections. Cheap approximation of planar and glossy reflections.

**When**: After water is in (Phase 6). Water's Fresnel reflection currently samples the sky — SSR would reflect terrain and trees in the water surface. Also useful for wet surfaces after rain.

**Effort**: Medium. Hi-Z acceleration (see below) helps performance. Needs the scene color buffer as input, so it runs after the scene pass and before compositing.

---

## GPU-Driven Terrain Indirect Draw

**What**: Use a compute shader to evaluate terrain chunk visibility (frustum + distance) and write draw commands into an indirect buffer. The CPU submits a single `draw_indirect` call instead of per-chunk draw calls.

**When**: When terrain chunk count grows large enough that CPU-side draw call overhead matters. Currently the terrain is a modest grid and CPU dispatch is fine.

**Effort**: Low-medium. The indirect draw pattern is already proven with grass compute. Mainly requires the terrain renderer to switch from direct draw calls to an indirect buffer filled by compute.

---

## Hi-Z Occlusion Culling

**What**: Build a hierarchical depth buffer (mip chain of the depth texture) and test object bounding boxes against it to skip drawing objects fully hidden behind terrain or other geometry.

**Effort**: Medium. Requires a depth downscale compute pass (mip chain), a culling compute pass per object category, and indirect draw buffers.

**When**: After compute scattering (Phase 4) puts trees and rocks on indirect draw. Hi-Z culling would then cull instances that are behind hills. Most beneficial in hilly terrain with dense forests.

---

## Depth of Field (DOF)

**What**: Blur foreground and background based on distance from a focal plane, simulating camera lens focus.

**When**: Probably as an optional cinematic/photo mode effect, not always-on. The game's low-poly style benefits from everything being sharp. DOF would work well for inventory/crafting UI backgrounds or cutscenes.

**Effort**: Low-medium. Circle-of-confusion from depth buffer, separable Gaussian blur weighted by CoC. Can be done as a compute post-process pass.

---

## Terrain Geo-Morphing

**What**: Smoothly interpolate terrain mesh LOD transitions so that vertices don't pop when LOD levels change. Each vertex blends between its current position and its lower-LOD position based on distance.

**When**: When terrain gets multiple LOD levels. Currently terrain is a single LOD grid. Geo-morphing matters when the camera moves and LOD rings shift — without it, visible popping occurs at LOD boundaries.

**Effort**: Low. Purely a vertex shader change. Requires the terrain to have pre-computed morph targets or a consistent subdivision scheme.

---

## Screen-Space Indirect Lighting (SSIL)

**What**: Extends SSAO by also gathering indirect color bounces from nearby surfaces. Approximates one-bounce global illumination in screen space.

**When**: After GTAO (Phase 7) is in place. SSIL uses the same hemisphere sampling pattern but reads color instead of just depth. Adds colored light bleeding (e.g., green grass tinting nearby rocks).

**Effort**: Medium. Similar to GTAO but samples the color buffer in addition to depth. Needs careful denoising to avoid noise from low sample counts.

---

## Full Froxel Volumetric Fog

**What**: Divide the view frustum into a 3D grid of voxels (froxels). For each froxel, accumulate inscattered light considering shadow maps and phase functions. Renders fog, god rays, and light shafts in a unified system with correct per-voxel density and lighting.

**When**: After the simpler volumetric light pass (Phase 8b). Phase 8b ray-marches per pixel. Froxel fog is the proper upgrade — temporal reprojection across froxels gives smoother results at lower cost, and supports spatially varying fog density (valley fog, height fog, localized effects).

**Effort**: High. 3D texture allocation, compute scatter/gather passes, temporal reprojection in 3D, integration with shadow cascades. Most complex single technique on this list.

---

## Subsurface Scattering Approximation

**What**: Cheap approximation of light passing through thin geometry (tree leaves, grass blades). When the sun is behind a leaf, it glows warm. Computed as wrap lighting or a transmission term based on `dot(-light_dir, view_dir)` and thickness.

**When**: After tree texturing (Phase 5c). The foliage canopy would benefit most — backlit trees at sunset are a major atmospheric moment.

**Effort**: Low. A few extra lines in the foliage fragment shader. No new passes or buffers.
