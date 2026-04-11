/// Procedural 16×16 pixel-art material tiles stored in a Texture2DArray.
///
/// Six materials — grass, dirt, sand, rock, bark, foliage — generated entirely
/// in Rust with deterministic hashing (no external assets). Nearest-neighbor
/// filtering + repeat addressing preserves the blocky pixel-art aesthetic.

const TILE_SIZE: u32 = 16;
const NUM_MATERIALS: u32 = 6;

pub const MAT_GRASS: u32 = 0;
pub const MAT_DIRT: u32 = 1;
pub const MAT_SAND: u32 = 2;
pub const MAT_ROCK: u32 = 3;
pub const MAT_BARK: u32 = 4;
pub const MAT_FOLIAGE: u32 = 5;

pub struct TextureAtlas {
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl TextureAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let pixel_data = generate_atlas();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Material Atlas"),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: NUM_MATERIALS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixel_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TILE_SIZE * 4),
                rows_per_image: Some(TILE_SIZE),
            },
            wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: NUM_MATERIALS,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Atlas BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atlas BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        log::info!(
            "Texture atlas: {}x{} tiles, {} materials",
            TILE_SIZE,
            TILE_SIZE,
            NUM_MATERIALS,
        );

        Self {
            bind_group,
            bind_group_layout,
        }
    }
}

// ---------------------------------------------------------------------------
// Procedural tile generation
// ---------------------------------------------------------------------------

fn generate_atlas() -> Vec<u8> {
    let layer_bytes = (TILE_SIZE * TILE_SIZE * 4) as usize;
    let mut data = vec![0u8; layer_bytes * NUM_MATERIALS as usize];

    gen_grass(&mut data[layer_bytes * MAT_GRASS as usize..]);
    gen_dirt(&mut data[layer_bytes * MAT_DIRT as usize..]);
    gen_sand(&mut data[layer_bytes * MAT_SAND as usize..]);
    gen_rock(&mut data[layer_bytes * MAT_ROCK as usize..]);
    gen_bark(&mut data[layer_bytes * MAT_BARK as usize..]);
    gen_foliage(&mut data[layer_bytes * MAT_FOLIAGE as usize..]);

    data
}

/// Deterministic integer hash → [0, 1].
fn ph(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(1274126177));
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h = h ^ (h >> 16);
    (h & 0xFFFF) as f32 / 65535.0
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn set_px(data: &mut [u8], x: u32, y: u32, r: f32, g: f32, b: f32) {
    let i = ((y * TILE_SIZE + x) * 4) as usize;
    data[i] = to_u8(r);
    data[i + 1] = to_u8(g);
    data[i + 2] = to_u8(b);
    data[i + 3] = 255;
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

// ---------------------------------------------------------------------------
// Grass — meadow ground: mixed green shades, blade tips, occasional dirt
// ---------------------------------------------------------------------------

fn gen_grass(data: &mut [u8]) {
    let base = [0.26, 0.42, 0.18];
    let bright = [0.34, 0.54, 0.22];
    let dark = [0.18, 0.32, 0.12];
    let dirt = [0.30, 0.24, 0.14];

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let h0 = ph(x, y, 0);
            let h1 = ph(x, y, 17);
            let h2 = ph(x, y, 53);

            // Base with smooth variation between dark and bright
            let mut c = lerp3(dark, bright, h0);

            // Mid-tone pull toward base
            c = lerp3(c, base, 0.4);

            // Fine per-pixel variation
            let v = (h1 - 0.5) * 0.08;
            c[0] += v * 0.7;
            c[1] += v;
            c[2] += v * 0.5;

            // Blade tips: staggered bright pixels
            let blade_col = (x + (y / 2) * 3) % 5;
            let blade_row = y % 3;
            if blade_col == 0 && blade_row == 0 && h0 > 0.3 {
                c[1] += 0.10;
                c[0] -= 0.02;
            }

            // Occasional dirt showing through
            if h2 > 0.91 {
                c = lerp3(c, dirt, 0.7);
            }

            set_px(data, x, y, c[0], c[1], c[2]);
        }
    }
}

// ---------------------------------------------------------------------------
// Dirt — warm brown earth with pebbles and fine grain
// ---------------------------------------------------------------------------

fn gen_dirt(data: &mut [u8]) {
    let base = [0.33, 0.25, 0.16];
    let light = [0.42, 0.34, 0.22];
    let dark = [0.24, 0.18, 0.11];

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let h0 = ph(x, y, 100);
            let h1 = ph(x, y, 111);
            let h2 = ph(x, y, 123);

            // Base with gentle variation
            let mut c = lerp3(dark, light, h0 * 0.6 + 0.2);

            // Pull toward mid-tone
            c = lerp3(c, base, 0.35);

            // Fine grain
            let grain = (h1 - 0.5) * 0.06;
            c[0] += grain;
            c[1] += grain * 0.8;
            c[2] += grain * 0.5;

            // Pebble highlights (small bright spots)
            if h2 > 0.87 {
                let peb = ph(x, y, 130) * 0.08 + 0.04;
                c[0] += peb;
                c[1] += peb * 0.9;
                c[2] += peb * 0.7;
            }

            // Dark specks (tiny stones)
            if h2 < 0.06 {
                c[0] -= 0.06;
                c[1] -= 0.05;
                c[2] -= 0.03;
            }

            set_px(data, x, y, c[0], c[1], c[2]);
        }
    }
}

// ---------------------------------------------------------------------------
// Sand — light tan with subtle grain, minimal variation
// ---------------------------------------------------------------------------

fn gen_sand(data: &mut [u8]) {
    let base = [0.38, 0.33, 0.20];
    let warm = [0.42, 0.36, 0.22];
    let cool = [0.34, 0.30, 0.19];

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let h0 = ph(x, y, 200);
            let h1 = ph(x, y, 217);

            // Very gentle variation
            let mut c = lerp3(cool, warm, h0);
            c = lerp3(c, base, 0.5);

            // Subtle individual grain
            let grain = (h1 - 0.5) * 0.04;
            c[0] += grain;
            c[1] += grain * 0.9;
            c[2] += grain * 0.6;

            // Sparse lighter grains
            if h0 > 0.92 {
                c[0] += 0.04;
                c[1] += 0.03;
                c[2] += 0.02;
            }

            // Sparse darker grains
            if h0 < 0.05 {
                c[0] -= 0.03;
                c[1] -= 0.02;
                c[2] -= 0.01;
            }

            set_px(data, x, y, c[0], c[1], c[2]);
        }
    }
}

// ---------------------------------------------------------------------------
// Rock — gray surface with cracks and rough speckle
// ---------------------------------------------------------------------------

fn gen_rock(data: &mut [u8]) {
    let base = [0.46, 0.42, 0.37];
    let light = [0.54, 0.50, 0.44];
    let dark = [0.36, 0.32, 0.28];
    let crack = [0.24, 0.22, 0.20];

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let h0 = ph(x, y, 300);
            let h1 = ph(x, y, 311);
            let h2 = ph(x, y, 322);

            // Base with variation
            let mut c = lerp3(dark, light, h0 * 0.7 + 0.15);
            c = lerp3(c, base, 0.3);

            // Surface roughness
            let rough = (h1 - 0.5) * 0.07;
            c[0] += rough;
            c[1] += rough * 0.95;
            c[2] += rough * 0.85;

            // Crack pattern: two diagonal-ish lines across the tile
            // Crack 1: runs roughly from (2,0) to (13,15)
            let crack_x1 = 2.0 + (y as f32) * 0.7 + ph(0, y, 340) * 2.0;
            let dist1 = (x as f32 - crack_x1).abs();
            // Crack 2: runs roughly from (10,0) to (5,15)
            let crack_x2 = 10.0 - (y as f32) * 0.3 + ph(0, y, 350) * 2.0;
            let dist2 = (x as f32 - crack_x2).abs();

            if dist1 < 0.8 || dist2 < 0.8 {
                c = lerp3(c, crack, 0.7);
            } else if dist1 < 1.5 || dist2 < 1.5 {
                // Slight darkening near cracks
                c = lerp3(c, crack, 0.15);
            }

            // Subtle warm/cool variation (lichen hint)
            if h2 > 0.88 {
                c[1] += 0.02;
                c[2] -= 0.01;
            }

            set_px(data, x, y, c[0], c[1], c[2]);
        }
    }
}

// ---------------------------------------------------------------------------
// Bark — dark brown with vertical grooves
// ---------------------------------------------------------------------------

fn gen_bark(data: &mut [u8]) {
    let ridge = [0.28, 0.18, 0.11];
    let groove = [0.16, 0.10, 0.06];
    let highlight = [0.34, 0.23, 0.14];

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let h0 = ph(x, y, 400);
            let h1 = ph(x, y, 413);

            // Vertical groove pattern: ~3–4 px wide bands
            // Use a sine-like pattern that wraps at TILE_SIZE
            let phase = (x as f32) * std::f32::consts::TAU / TILE_SIZE as f32 * 3.5;
            let band = (phase.sin() + 1.0) * 0.5; // 0..1, 0 = groove, 1 = ridge

            // Slight horizontal wobble per row
            let wobble = ph(0, y, 420) * 0.3 - 0.15;
            let band = (band + wobble).clamp(0.0, 1.0);

            let mut c = lerp3(groove, ridge, band);

            // Ridge highlights
            if band > 0.7 {
                c = lerp3(c, highlight, (band - 0.7) * 1.5 * h0);
            }

            // Fine per-pixel noise
            let noise = (h1 - 0.5) * 0.05;
            c[0] += noise;
            c[1] += noise * 0.8;
            c[2] += noise * 0.6;

            // Occasional knot-like dark spot
            let kx = (x as f32 - 7.0).abs();
            let ky = ((y as f32 - 8.0).abs()) * 0.6;
            if kx * kx + ky * ky < 3.0 && h0 > 0.7 {
                c = lerp3(c, groove, 0.4);
            }

            set_px(data, x, y, c[0], c[1], c[2]);
        }
    }
}

// ---------------------------------------------------------------------------
// Foliage — leaf clusters with depth: bright canopy, dark gaps
// ---------------------------------------------------------------------------

fn gen_foliage(data: &mut [u8]) {
    let leaf_bright = [0.28, 0.48, 0.16];
    let leaf_mid = [0.20, 0.38, 0.12];
    let leaf_dark = [0.12, 0.24, 0.08];
    let highlight = [0.36, 0.52, 0.20];

    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let h0 = ph(x, y, 500);
            let h1 = ph(x, y, 517);

            // Leaf cluster pattern: hash at coarse grid determines cluster center
            let cx = x / 3;
            let cy = y / 3;
            let cluster = ph(cx, cy, 530);
            let cluster_next = ph(cx.wrapping_add(1), cy, 530);

            // Local position within cluster cell
            let lx = (x % 3) as f32 / 2.0; // 0..1
            let ly = (y % 3) as f32 / 2.0;
            let dist_center = ((lx - 0.5) * (lx - 0.5) + (ly - 0.5) * (ly - 0.5)).sqrt();

            let mut c;
            if cluster > 0.35 && dist_center < 0.6 {
                // Inside a leaf cluster — bright canopy
                c = lerp3(leaf_mid, leaf_bright, h0);
            } else if cluster_next > 0.5 && dist_center > 0.5 {
                // Edge/transition — mid tone
                c = lerp3(leaf_dark, leaf_mid, h0);
            } else {
                // Gap between clusters — dark shadow
                c = lerp3(leaf_dark, leaf_mid, h0 * 0.4);
            }

            // Specular highlight on some cluster pixels
            if cluster > 0.65 && h1 > 0.8 && dist_center < 0.4 {
                c = lerp3(c, highlight, 0.5);
            }

            // Fine variation
            let v = (h1 - 0.5) * 0.06;
            c[0] += v * 0.6;
            c[1] += v;
            c[2] += v * 0.4;

            set_px(data, x, y, c[0], c[1], c[2]);
        }
    }
}
