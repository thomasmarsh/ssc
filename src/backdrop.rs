//! The in-game backdrop: a deterministic nebula and starfield character for every point of
//! the universe, in the same colours the sector map (`sectormap`) paints. Pure and headless:
//! the Bevy side (`presentation.rs`) only draws what `backdrop_at` returns.
//!
//! A sector contributes a `Look` (its dominant range hue, biome hue, life, rock, belt depth and
//! so on, read from the same ecology the map samples). `backdrop_at` blends the four sector
//! centres around a world position bilinearly (eased, so a place keeps its own look until you
//! near a border), which makes every value continuous across sector borders. Looks are cached
//! per sector id, because `ecology` and `region` are not cheap.

use crate::biome::BiomeKind;
use crate::range::{ecology, ring};
use crate::region::region;
use crate::world::{SECTOR_SIZE, SectorId, hash2};
use bevy::prelude::Vec2;
use std::cell::RefCell;
use std::collections::HashMap;

/// Colour with alpha: sRGB components in [0, 1].
pub type Rgba = [f32; 4];

/// The map's hue for each kind of country (`BIOHUE` in `sectormap/template.html`), in
/// `BiomeKind::ALL` order.
pub const BIOME_HUES: [f32; 8] = [200.0, 130.0, 95.0, 0.0, 285.0, 45.0, 25.0, 170.0];
/// The map's rock brown (dark theme), as sRGB.
pub const ROCK: [f32; 3] = [0.76, 0.65, 0.54];
/// The starfield's base tint (the nearest layer) when nothing colours it.
pub const STAR_BASE: [f32; 3] = [0.5, 0.64, 0.78];

/// No cloud layer is ever more opaque than this: ships, rocks and shots stay legible.
pub const MAX_LAYER_ALPHA: f32 = 0.13;
/// Side of the generated noise textures, in texels.
pub const TEXTURE_SIZE: usize = 256;
/// How much of a border's blend happens near it: the blend is eased over the middle
/// `2 * BLEND_HALF` of the way between two sector centres.
const BLEND_HALF: f32 = 0.3;

/// The four noise textures the renderer tiles. Every one is seamless.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grain {
    /// Wide soft clouds.
    Wisp,
    /// Long sharp streaks.
    Streak,
    /// Thin branching filaments.
    Filament,
    /// Fine dust speckle.
    Grit,
}

/// How much of each grain a place has; the weights add up to one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Texture {
    pub wisp: f32,
    pub streak: f32,
    pub filament: f32,
    pub grit: f32,
}

impl Texture {
    fn array(self) -> [f32; 4] {
        [self.wisp, self.streak, self.filament, self.grit]
    }

    fn from_array(a: [f32; 4]) -> Self {
        Self {
            wisp: a[0],
            streak: a[1],
            filament: a[2],
            grit: a[3],
        }
    }

    /// The grain with the most weight.
    pub fn dominant(self) -> Grain {
        let a = self.array();
        let mut best = 0;
        for i in 1..4 {
            if a[i] > a[best] {
                best = i;
            }
        }
        [Grain::Wisp, Grain::Streak, Grain::Filament, Grain::Grit][best]
    }
}

/// What the sky looks like at one point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backdrop {
    /// The predominant nebula colour (the map's range hue, shaded by life and rock).
    pub tint: [f32; 3],
    /// The second colour (the map's biome hue; a confluence's second species).
    pub secondary: [f32; 3],
    /// How much cloud there is, in [0, 1].
    pub density: f32,
    pub texture: Texture,
    /// How dusty the place is (rock and belts), in [0, 1].
    pub dust: f32,
    /// Warm glow of an oasis' planetoid, in [0, 1].
    pub glow: f32,
    /// How much of a civilization's haze lies here, in [0, 1], and its colour.
    pub haze: f32,
    pub haze_tint: [f32; 3],
    /// Stars per area relative to the usual field, about 0.4 to 1.5, and their tint.
    pub star_density: f32,
    pub star_tint: [f32; 3],
}

impl Backdrop {
    /// The plain look HOME has: used when effects are reduced.
    pub const NEUTRAL: Backdrop = Backdrop {
        tint: [0.3, 0.4, 0.6],
        secondary: [0.3, 0.4, 0.6],
        density: 0.0,
        texture: Texture {
            wisp: 1.0,
            streak: 0.0,
            filament: 0.0,
            grit: 0.0,
        },
        dust: 0.0,
        glow: 0.0,
        haze: 0.0,
        haze_tint: [0.3, 0.4, 0.6],
        star_density: 1.0,
        star_tint: STAR_BASE,
    };

    /// The renderer's recipe: a colour and an opacity (never above `MAX_LAYER_ALPHA`) for each
    /// layer, derived from the character. Rendering multiplies these by the grain textures.
    pub fn layers(&self) -> Layers {
        let cap = |a: f32| a.clamp(0.0, MAX_LAYER_ALPHA);
        let t = self.texture;
        let d = self.density;
        let mid = mix3(self.tint, self.secondary, 0.45);
        Layers {
            far_wisp: rgba(self.tint, cap(MAX_LAYER_ALPHA * d * (0.35 + 0.65 * t.wisp))),
            mid_wisp: rgba(mid, cap(MAX_LAYER_ALPHA * 0.8 * d * t.wisp)),
            near_grit: rgba(
                mix3([0.62, 0.52, 0.42], self.tint, 0.25),
                cap(MAX_LAYER_ALPHA * (0.35 * t.grit * d + 0.45 * self.dust)),
            ),
        }
    }
}

/// The colour and opacity of each cloud layer at a point (back to front).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layers {
    pub far_wisp: Rgba,
    pub mid_wisp: Rgba,
    pub near_grit: Rgba,
}

fn rgba(c: [f32; 3], a: f32) -> Rgba {
    [c[0], c[1], c[2], a]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// HSL (hue in degrees) to sRGB.
pub fn hsl(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

/// One sector's character, before blending.
#[derive(Clone, Copy, Debug)]
struct Look {
    tint: [f32; 3],
    secondary: [f32; 3],
    density: f32,
    texture: [f32; 4],
    dust: f32,
    haze: f32,
    haze_tint: [f32; 3],
    star_density: f32,
    star_tint: [f32; 3],
}

/// The texture mix of a kind of country.
fn biome_texture(kind: BiomeKind) -> [f32; 4] {
    match kind {
        BiomeKind::Open => [0.8, 0.1, 0.1, 0.0],
        BiomeKind::Plains => [1.0, 0.0, 0.0, 0.0],
        BiomeKind::Grazing => [0.8, 0.0, 0.1, 0.1],
        BiomeKind::Predator => [0.35, 0.65, 0.0, 0.0],
        BiomeKind::Strange => [0.2, 0.0, 0.8, 0.0],
        BiomeKind::Keen => [0.3, 0.0, 0.7, 0.0],
        BiomeKind::Brutish => [0.3, 0.5, 0.0, 0.2],
        BiomeKind::Hardy => [0.5, 0.1, 0.0, 0.4],
    }
}

fn look(seed: u64, id: SectorId) -> Look {
    let eco = ecology(seed, id);
    let reg = region(seed, id);
    let life = eco.life;
    let hue = eco
        .presence
        .first()
        .map(|p| (hash2(p.species.lineage, 17, 29) % 360) as f32);
    let mut tint = match hue {
        Some(h) => hsl(h, 0.3 + 0.5 * life, 0.52),
        None => hsl(220.0, 0.15, 0.45),
    };
    let secondary = hsl(BIOME_HUES[eco.biome.kind.index()], 0.6, 0.5);
    // Rock shades the cloud toward the map's brown; a belt is nearly all dust.
    let rock = ((eco.matter - 0.4) / 0.6).clamp(0.0, 1.0) * 0.5;
    tint = mix3(tint, ROCK, (rock + 0.4 * eco.belt).min(0.8));
    let _ = (reg, ring(id));
    Look {
        tint,
        secondary,
        density: (0.25 + 0.6 * life).clamp(0.0, 1.0),
        texture: biome_texture(eco.biome.kind),
        dust: (0.3 * rock + 0.8 * eco.belt).clamp(0.0, 1.0),
        haze: 0.0,
        haze_tint: tint,
        star_density: 1.0,
        star_tint: STAR_BASE,
    }
}

thread_local! {
    static LOOKS: RefCell<HashMap<(u64, SectorId), Look>> = RefCell::new(HashMap::new());
}

/// Looks kept before the cache is dropped and refilled (a few screens of sectors at most are
/// ever asked for at once).
const CACHE_LIMIT: usize = 2048;

fn cached_look(seed: u64, id: SectorId) -> Look {
    LOOKS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(l) = cache.get(&(seed, id)) {
            return *l;
        }
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        let l = look(seed, id);
        cache.insert((seed, id), l);
        l
    })
}

fn ease(t: f32) -> f32 {
    let u = ((t - (0.5 - BLEND_HALF)) / (2.0 * BLEND_HALF)).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// The backdrop at world position `pos`. A pure function of the seed and position; cheap after
/// the first call near a sector (its look is cached).
pub fn backdrop_at(seed: u64, pos: Vec2) -> Backdrop {
    let g = pos / SECTOR_SIZE;
    let base = g.floor();
    let f = g - base;
    let (bx, by) = (base.x as i32, base.y as i32);
    let (ex, ey) = (ease(f.x), ease(f.y));
    let looks = [
        cached_look(seed, SectorId { x: bx, y: by }),
        cached_look(seed, SectorId { x: bx + 1, y: by }),
        cached_look(seed, SectorId { x: bx, y: by + 1 }),
        cached_look(
            seed,
            SectorId {
                x: bx + 1,
                y: by + 1,
            },
        ),
    ];
    let w = [
        (1.0 - ex) * (1.0 - ey),
        ex * (1.0 - ey),
        (1.0 - ex) * ey,
        ex * ey,
    ];
    let sum =
        |get: &dyn Fn(&Look) -> f32| -> f32 { looks.iter().zip(w).map(|(l, w)| get(l) * w).sum() };
    let sum3 = |get: &dyn Fn(&Look) -> [f32; 3]| -> [f32; 3] {
        let mut out = [0.0; 3];
        for (l, w) in looks.iter().zip(w) {
            let c = get(l);
            for (o, v) in out.iter_mut().zip(c) {
                *o += v * w;
            }
        }
        out
    };
    let mut tex = [0.0; 4];
    for (l, w) in looks.iter().zip(w) {
        for (t, v) in tex.iter_mut().zip(l.texture) {
            *t += v * w;
        }
    }
    Backdrop {
        tint: sum3(&|l| l.tint),
        secondary: sum3(&|l| l.secondary),
        density: sum(&|l| l.density),
        texture: Texture::from_array(tex),
        dust: sum(&|l| l.dust),
        glow: 0.0,
        haze: sum(&|l| l.haze),
        haze_tint: sum3(&|l| l.haze_tint),
        star_density: sum(&|l| l.star_density),
        star_tint: sum3(&|l| l.star_tint),
    }
}

// ---------------------------------------------------------------------------------------------
// Noise textures
// ---------------------------------------------------------------------------------------------

const TEXTURE_SALT: u64 = 0x4E45_4255_4C41_0001;

fn lattice(seed: u64, x: i32, y: i32, px: i32, py: i32) -> f32 {
    (hash2(seed, x.rem_euclid(px), y.rem_euclid(py)) >> 40) as f32 / 16_777_216.0
}

/// Value noise that repeats every `px` by `py` lattice cells.
fn periodic(seed: u64, x: f32, y: f32, px: i32, py: i32) -> f32 {
    let (cx, cy) = (x.floor(), y.floor());
    let (tx, ty) = (x - cx, y - cy);
    let (tx, ty) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (ix, iy) = (cx as i32, cy as i32);
    let c = |dx, dy| lattice(seed, ix + dx, iy + dy, px, py);
    let top = c(0, 0) + (c(1, 0) - c(0, 0)) * tx;
    let bottom = c(0, 1) + (c(1, 1) - c(0, 1)) * tx;
    top + (bottom - top) * ty
}

/// Fractal periodic noise over the unit square, in [0, 1].
fn fractal(seed: u64, u: f32, v: f32, px: i32, py: i32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
    for o in 0..octaves {
        let k = 1 << o;
        sum += amp
            * periodic(
                seed ^ o as u64,
                u * (px * k) as f32,
                v * (py * k) as f32,
                px * k,
                py * k,
            );
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A seamless `size` by `size` coverage texture (0 to 255, row-major) for a grain: generated
/// once at startup and tinted per layer. Pure and seed-free: the same cloud shapes everywhere,
/// the places differ in colour, grain and amount.
pub fn texture(grain: Grain, size: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = (x as f32 / size as f32, y as f32 / size as f32);
            let value = match grain {
                Grain::Wisp => {
                    let n = fractal(TEXTURE_SALT ^ 1, u, v, 6, 6, 4);
                    smoothstep(0.36, 0.68, n)
                }
                Grain::Streak => {
                    // Long in x, short in y: horizontal streaks the renderer turns.
                    let warp = 0.04 * (periodic(TEXTURE_SALT ^ 9, u * 3.0, v * 3.0, 3, 3) - 0.5);
                    let n = fractal(TEXTURE_SALT ^ 2, (u + warp).rem_euclid(1.0), v, 2, 28, 3);
                    smoothstep(0.52, 0.72, n)
                }
                Grain::Filament => {
                    // Ridges of a warped noise: thin bright lines.
                    let warp = 0.06 * (periodic(TEXTURE_SALT ^ 8, u * 4.0, v * 4.0, 4, 4) - 0.5);
                    let n = fractal(
                        TEXTURE_SALT ^ 3,
                        (u + warp).rem_euclid(1.0),
                        (v - warp).rem_euclid(1.0),
                        6,
                        6,
                        3,
                    );
                    let ridge = 1.0 - (2.0 * n - 1.0).abs();
                    smoothstep(0.86, 0.98, ridge)
                }
                Grain::Grit => {
                    let n = fractal(TEXTURE_SALT ^ 4, u, v, 48, 48, 1);
                    smoothstep(0.5, 0.85, n)
                }
            };
            out.push((value * 255.0).round() as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x53_5343;

    fn dist(a: &Backdrop, b: &Backdrop) -> f32 {
        let c = |p: [f32; 3], q: [f32; 3]| (0..3).map(|k| (p[k] - q[k]).abs()).sum::<f32>();
        c(a.tint, b.tint)
            + c(a.secondary, b.secondary)
            + (a.density - b.density).abs()
            + (a.dust - b.dust).abs()
            + (a.star_density - b.star_density).abs()
    }

    #[test]
    fn the_backdrop_is_deterministic_and_seeded() {
        let p = Vec2::new(13_579.0, -24_680.0);
        assert_eq!(backdrop_at(SEED, p), backdrop_at(SEED, p));
        assert_ne!(backdrop_at(SEED, p), backdrop_at(SEED ^ 0xFFFF, p));
        // A fresh cache gives the same answer.
        let first = backdrop_at(SEED, p);
        LOOKS.with(|c| c.borrow_mut().clear());
        assert_eq!(first, backdrop_at(SEED, p));
    }

    #[test]
    fn it_is_continuous_across_borders() {
        // Walk a long line crossing many borders in small steps: no step may jump.
        for (from, to) in [
            (Vec2::new(-30_000.0, 7_000.0), Vec2::new(60_000.0, 9_000.0)),
            (Vec2::new(5_000.0, -50_000.0), Vec2::new(8_000.0, 40_000.0)),
        ] {
            let steps = 3000;
            let mut prev = backdrop_at(SEED, from);
            let mut worst: f32 = 0.0;
            for i in 1..=steps {
                let p = from.lerp(to, i as f32 / steps as f32);
                let b = backdrop_at(SEED, p);
                worst = worst.max(dist(&prev, &b));
                prev = b;
            }
            // About 30 units per step; the whole palette moves by far less than a sector's
            // worth of difference in one step.
            assert!(worst < 0.05, "largest step {worst}");
        }
        // Either side of a border is nearly the same place.
        for id in [(3, 4), (-6, 2), (9, -9)] {
            let edge = (id.0 as f32 + 0.5) * SECTOR_SIZE;
            let y = id.1 as f32 * SECTOR_SIZE;
            let a = backdrop_at(SEED, Vec2::new(edge - 0.5, y));
            let b = backdrop_at(SEED, Vec2::new(edge + 0.5, y));
            assert!(dist(&a, &b) < 0.01);
        }
    }

    #[test]
    fn places_differ_and_the_palette_follows_the_map() {
        let mut seen = std::collections::HashSet::new();
        let mut far_apart = 0;
        let points: Vec<Vec2> = (-8..=8)
            .flat_map(|x| (-8..=8).map(move |y| Vec2::new(x as f32, y as f32) * SECTOR_SIZE))
            .collect();
        let looks: Vec<Backdrop> = points.iter().map(|p| backdrop_at(SEED, *p)).collect();
        for (i, a) in looks.iter().enumerate() {
            seen.insert(
                (a.tint[0] * 8.0) as u32 * 64
                    + (a.tint[1] * 8.0) as u32 * 8
                    + (a.tint[2] * 8.0) as u32,
            );
            if dist(a, &looks[(i + 37) % looks.len()]) > 0.3 {
                far_apart += 1;
            }
        }
        assert!(seen.len() > 12, "only {} tints", seen.len());
        assert!(far_apart > looks.len() / 4, "{far_apart} distinct pairs");
    }

    #[test]
    fn layers_stay_faint() {
        for x in -12..=12 {
            for y in -12..=12 {
                let p = Vec2::new(x as f32 * 2_300.0, y as f32 * 2_900.0);
                let l = backdrop_at(SEED, p).layers();
                for c in [l.far_wisp, l.mid_wisp, l.near_grit] {
                    assert!(c[3] >= 0.0 && c[3] <= MAX_LAYER_ALPHA, "{c:?}");
                    assert!(c[..3].iter().all(|v| (0.0..=1.0).contains(v)), "{c:?}");
                }
            }
        }
    }

    #[test]
    fn textures_are_deterministic_seamless_and_not_flat() {
        for grain in [Grain::Wisp, Grain::Streak, Grain::Filament, Grain::Grit] {
            let a = texture(grain, 64);
            assert_eq!(a, texture(grain, 64));
            let mean = a.iter().map(|v| *v as f32).sum::<f32>() / a.len() as f32;
            let max = *a.iter().max().unwrap();
            assert!(max > 120, "{grain:?} peaks at {max}");
            assert!(mean > 2.0 && mean < 160.0, "{grain:?} mean {mean}");
        }
        // Seamless: values across the wrap are as close as neighbours inside.
        let n = 128;
        let t = texture(Grain::Wisp, n);
        let step = |a: u8, b: u8| (a as i32 - b as i32).abs();
        let inner: i32 = (0..n).map(|y| step(t[y * n + 63], t[y * n + 64])).sum();
        let wrap: i32 = (0..n).map(|y| step(t[y * n + n - 1], t[y * n])).sum();
        assert!(wrap < inner * 3 + 40, "wrap {wrap} vs inner {inner}");
    }

    #[test]
    fn hsl_matches_known_colours() {
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-5);
        assert!(near(hsl(0.0, 1.0, 0.5), [1.0, 0.0, 0.0]));
        assert!(near(hsl(120.0, 1.0, 0.5), [0.0, 1.0, 0.0]));
        assert!(near(hsl(240.0, 1.0, 0.5), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn the_biome_hues_match_the_map_page() {
        let page = include_str!("sectormap/template.html");
        let list = BIOME_HUES.map(|h| format!("{h}")).join(",");
        assert!(page.contains(&format!("const BIOHUE=[{list}]")), "{list}");
    }
}
