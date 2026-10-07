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
use crate::range::ecology;
use crate::region::{RegionKind, region};
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
pub const MAX_LAYER_ALPHA: f32 = 0.075;
/// Dark layers may be a little more opaque: they cut rather than glow.
pub const MAX_STREAK_ALPHA: f32 = 0.16;
/// The smooth colour wash and the vignette are capped too.
pub const MAX_WASH_ALPHA: f32 = 0.09;
pub const MAX_VIGNETTE_ALPHA: f32 = 0.3;
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
    /// How near a region border the place is, in [0, 1], and the colour of the seam (the
    /// mix of the regions that meet there, leaning to the neighbour).
    pub edge: f32,
    pub edge_tint: [f32; 3],
    /// Peaks on the boundary of a civilization's claim, in [0, 1].
    pub claim_edge: f32,
    /// The bright heart of a confluence, in [0, 1].
    pub core: f32,
    /// Nearness of an apex elder's sector, in [0, 1]: a dark and gold vignette.
    pub vignette: f32,
    /// The kind of region of the nearest sector.
    pub kind: RegionKind,
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
        edge: 0.0,
        edge_tint: [0.3, 0.4, 0.6],
        claim_edge: 0.0,
        core: 0.0,
        vignette: 0.0,
        kind: RegionKind::Home,
    };

    /// The renderer's recipe: a colour and an opacity (never above `MAX_LAYER_ALPHA`) for each
    /// layer, derived from the character. Rendering multiplies these by the grain textures.
    pub fn layers(&self) -> Layers {
        let cap = |a: f32| a.clamp(0.0, MAX_LAYER_ALPHA);
        let t = self.texture;
        let d = self.density;
        let mid = mix3(self.tint, self.secondary, 0.45);
        let warm = [1.0, 0.72, 0.4];
        let gold = [0.96, 0.77, 0.26];
        let lift = |c: [f32; 3], t: f32| mix3(c, [1.0, 1.0, 1.0], t);
        Layers {
            far_wisp: rgba(self.tint, cap(MAX_LAYER_ALPHA * d * (0.25 + 0.75 * t.wisp))),
            mid_wisp: rgba(mid, cap(MAX_LAYER_ALPHA * 0.8 * d * t.wisp)),
            mid_streak: rgba(
                [0.015, 0.008, 0.025],
                (1.5 * MAX_LAYER_ALPHA * d * t.streak).clamp(0.0, MAX_STREAK_ALPHA),
            ),
            mid_filament: rgba(
                lift(self.tint, 0.3),
                cap(MAX_LAYER_ALPHA * 1.1 * d * t.filament),
            ),
            near_grit: rgba(
                mix3([0.62, 0.52, 0.42], self.tint, 0.25),
                cap(MAX_LAYER_ALPHA * (0.35 * t.grit * d + 0.35 * self.dust)),
            ),
            wash: wash(&[
                (self.haze_tint, 0.03 * self.haze),
                (self.edge_tint, 0.07 * self.edge.powf(1.5)),
                (lift(self.haze_tint, 0.3), 0.08 * self.claim_edge),
                (warm, 0.08 * self.glow),
                (lift(mix3(gold, self.tint, 0.4), 0.35), 0.06 * self.core),
                (gold, 0.02 * self.vignette),
            ]),
            vignette: self.vignette,
        }
    }
}

/// Darkness of the apex vignette at normalized screen radius `r` (0 centre, 1 the middle of
/// an edge) for a strength in [0, 1].
pub fn vignette_alpha(strength: f32, r: f32) -> f32 {
    (MAX_VIGNETTE_ALPHA * strength * (r * r).min(1.6)).clamp(0.0, MAX_VIGNETTE_ALPHA)
}

/// Several translucent colours added as one (premultiplied sum, opacity capped).
fn wash(parts: &[([f32; 3], f32)]) -> Rgba {
    let total: f32 = parts.iter().map(|(_, a)| *a).sum();
    if total <= 1e-6 {
        return [0.0; 4];
    }
    let mut c = [0.0; 3];
    for (col, a) in parts {
        for (o, v) in c.iter_mut().zip(col) {
            *o += v * a / total;
        }
    }
    [c[0], c[1], c[2], total.min(MAX_WASH_ALPHA)]
}

/// The colour and opacity of each cloud layer at a point (back to front).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layers {
    pub far_wisp: Rgba,
    pub mid_wisp: Rgba,
    /// Dark streaks (a dark colour, so it may be a little more opaque).
    pub mid_streak: Rgba,
    pub mid_filament: Rgba,
    pub near_grit: Rgba,
    /// Smooth colour with no grain: haze, region seam, claim edge, glow, core, apex gold.
    pub wash: Rgba,
    /// Strength of the dark vignette; see `vignette_alpha`.
    pub vignette: f32,
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
    key: u64,
    kind: RegionKind,
    claim: f32,
    core: f32,
    apex: f32,
    /// An oasis' planetoid (world position, radius).
    pocket: Option<(Vec2, f32)>,
}

/// A civilization's pigment lifted so a dark one still reads against space.
fn lifted(c: [f32; 3]) -> [f32; 3] {
    let top = c[0].max(c[1]).max(c[2]).max(0.01);
    let k = (0.7 / top).max(1.0);
    [
        (c[0] * k).min(1.0),
        (c[1] * k).min(1.0),
        (c[2] * k).min(1.0),
    ]
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
        Some(h) => hsl(h, 0.25 + 0.4 * life, 0.5),
        None => hsl(220.0, 0.15, 0.45),
    };
    let secondary = hsl(BIOME_HUES[eco.biome.kind.index()], 0.6, 0.5);
    // Rock shades the cloud toward the map's brown; a belt is nearly all dust.
    let rock = ((eco.matter - 0.4) / 0.6).clamp(0.0, 1.0) * 0.5;
    tint = mix3(tint, ROCK, (rock + 0.4 * eco.belt).min(0.8));
    let mut look = Look {
        tint,
        secondary,
        density: (0.25 + 0.6 * life).clamp(0.0, 1.0),
        texture: biome_texture(eco.biome.kind),
        dust: (0.3 * rock + 0.8 * eco.belt).clamp(0.0, 1.0),
        haze: 0.0,
        haze_tint: tint,
        star_density: 0.8 + 0.5 * life,
        star_tint: STAR_BASE,
        key: reg.key,
        kind: reg.kind,
        claim: 0.0,
        core: 0.0,
        apex: match crate::apex::rank(seed, id) {
            Some(crate::apex::Rank::Major) => 1.0,
            Some(crate::apex::Rank::Lesser) => 0.5,
            None => 0.0,
        },
        pocket: None,
    };
    match reg.kind {
        RegionKind::Home => {
            look.density = 0.3;
            look.star_density = 1.0;
        }
        RegionKind::Wild => {}
        RegionKind::Belt => {
            // Dusty and dim: brown cloud, a grit layer, few stars.
            look.tint = mix3(look.tint, ROCK, 0.55);
            look.density = 0.3;
            look.texture = [0.3, 0.0, 0.0, 0.7];
            look.dust = (0.45 + 0.5 * eco.belt).clamp(0.0, 1.0);
            look.star_density = 0.45;
        }
        RegionKind::Gap => {
            // A cold, dark reach: little cloud, sparse stars.
            look.tint = hsl(215.0, 0.35, 0.42);
            look.secondary = hsl(235.0, 0.3, 0.4);
            look.density = 0.14;
            look.texture = [1.0, 0.0, 0.0, 0.0];
            look.dust = 0.0;
            look.star_density = 0.5;
        }
        RegionKind::Oasis => {
            look.tint = mix3(look.tint, [0.9, 0.62, 0.35], 0.25);
            look.density = 0.3;
            look.texture = [0.7, 0.0, 0.0, 0.3];
            look.star_density = 0.8;
            look.pocket = crate::world::planetoid_at(seed, id);
        }
        RegionKind::Confluence => {
            // Two hues and a bright heart.
            if let Some(second) = eco.presence.get(1) {
                let h = (hash2(second.species.lineage, 17, 29) % 360) as f32;
                look.secondary = hsl(h, 0.55 + 0.2 * life, 0.52);
            }
            look.tint = mix3(look.tint, look.secondary, 0.25);
            look.density = 0.7;
            look.texture = [0.5, 0.0, 0.5, 0.0];
            look.core = 1.0;
            look.star_density = 1.45;
        }
        RegionKind::Civ => {
            if let Some(t) = crate::territory::territory(seed, id) {
                look.density *= 0.65;
                look.haze = 1.0;
                look.haze_tint = lifted(t.color(seed));
                look.claim = 1.0;
            }
        }
    }
    look.star_tint = mix3(STAR_BASE, mix3(look.tint, [1.0, 1.0, 1.0], 0.5), 0.35);
    look
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
    let weights = |tx: f32, ty: f32| {
        [
            (1.0 - tx) * (1.0 - ty),
            tx * (1.0 - ty),
            (1.0 - tx) * ty,
            tx * ty,
        ]
    };
    // Eased weights set the colours (a place keeps its own until a border nears); the plain
    // bilinear ones measure nearness to borders and to the centre of a sector.
    let w = weights(ease(f.x), ease(f.y));
    let raw = weights(f.x, f.y);
    let sum =
        |get: &dyn Fn(&Look) -> f32| -> f32 { looks.iter().zip(w).map(|(l, w)| get(l) * w).sum() };
    let sum_raw = |get: &dyn Fn(&Look) -> f32| -> f32 {
        looks.iter().zip(raw).map(|(l, w)| get(l) * w).sum()
    };
    let sum3 = |get: &dyn Fn(&Look) -> [f32; 3]| -> [f32; 3] {
        let mut out = [0.0; 3];
        for (l, w) in looks.iter().zip(w) {
            for (o, v) in out.iter_mut().zip(get(l)) {
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
    // Region seam: how mixed the four corners' regions are (an impurity measure, symmetric,
    // zero inside one region, 1 on a border between two even halves).
    let share = |i: usize| -> f32 {
        (0..4)
            .filter(|j| looks[*j].key == looks[i].key)
            .map(|j| raw[j])
            .sum()
    };
    let purity: f32 = (0..4).map(|i| raw[i] * share(i)).sum();
    let edge = (2.0 * (1.0 - purity)).clamp(0.0, 1.0);
    // The seam leans to the neighbour: a corner counts for more the smaller its region's share.
    let mut edge_tint = [0.0; 3];
    let mut total = 0.0;
    for i in 0..4 {
        let k = raw[i] * (1.0 - share(i)).powi(2) + 1e-4 * raw[i];
        total += k;
        for (o, v) in edge_tint.iter_mut().zip(looks[i].tint) {
            *o += v * k;
        }
    }
    for o in &mut edge_tint {
        *o /= total;
    }
    let claim = sum_raw(&|l| l.claim);
    let nearest = (0..4)
        .max_by(|a, b| raw[*a].total_cmp(&raw[*b]))
        .unwrap_or(0);
    let glow = looks
        .iter()
        .filter_map(|l| l.pocket)
        .map(|(at, radius)| {
            let d = at.distance(pos) / (420.0 + 1.2 * radius);
            (-d * d).exp()
        })
        .fold(0.0, f32::max);
    Backdrop {
        tint: sum3(&|l| l.tint),
        secondary: sum3(&|l| l.secondary),
        density: sum(&|l| l.density),
        texture: Texture::from_array(tex),
        dust: sum(&|l| l.dust),
        glow,
        haze: sum(&|l| l.haze),
        haze_tint: sum3(&|l| l.haze_tint),
        star_density: sum(&|l| l.star_density),
        star_tint: sum3(&|l| l.star_tint),
        edge,
        edge_tint,
        claim_edge: (4.0 * claim * (1.0 - claim)).clamp(0.0, 1.0),
        core: sum(&|l| l.core),
        vignette: sum_raw(&|l| l.apex),
        kind: looks[nearest].kind,
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
                    smoothstep(0.4, 0.9, n)
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
    use crate::range::ring;

    const SEED: u64 = 0x53_5343;

    fn dist(a: &Backdrop, b: &Backdrop) -> f32 {
        let c = |p: [f32; 3], q: [f32; 3]| (0..3).map(|k| (p[k] - q[k]).abs()).sum::<f32>();
        c(a.tint, b.tint)
            + c(a.secondary, b.secondary)
            + (a.density - b.density).abs()
            + (a.dust - b.dust).abs()
            + (a.star_density - b.star_density).abs()
            + (a.edge - b.edge).abs()
            + (a.claim_edge - b.claim_edge).abs()
            + (a.core - b.core).abs()
            + (a.glow - b.glow).abs()
            + (a.haze - b.haze).abs()
            + (a.vignette - b.vignette).abs()
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

    /// The first sector (spiralling out from HOME) whose region has `kind` and whose
    /// neighbours all have it too, so its centre shows the kind undiluted.
    fn find(kind: RegionKind, biome: Option<BiomeKind>) -> SectorId {
        for r in 3..40 {
            for x in -r..=r {
                for y in -r..=r {
                    let id = SectorId { x, y };
                    if ring(id) != r as u32 || region(SEED, id).kind != kind {
                        continue;
                    }
                    if biome.is_some_and(|b| ecology(SEED, id).biome.kind != b) {
                        continue;
                    }
                    return id;
                }
            }
        }
        panic!("no {kind:?} sector found");
    }

    fn at(id: SectorId) -> Backdrop {
        backdrop_at(SEED, id.center())
    }

    #[test]
    fn belts_are_dusty_dim_and_sparse() {
        let belt = at(find(RegionKind::Belt, None));
        let wild = at(find(RegionKind::Wild, None));
        assert!(belt.dust > 0.5 && belt.dust > wild.dust + 0.2, "{belt:?}");
        assert!(belt.texture.grit > 0.5);
        assert!(belt.star_density < 0.6 && belt.star_density < wild.star_density);
        assert!(belt.layers().near_grit[3] > wild.layers().near_grit[3]);
    }

    #[test]
    fn reaches_are_dark_cold_and_sparse() {
        let gap = at(find(RegionKind::Gap, None));
        let wild = at(find(RegionKind::Wild, None));
        assert!(gap.density < 0.2 && gap.density < wild.density);
        assert!(gap.star_density < 0.6);
        assert!(gap.tint[2] > gap.tint[0] + 0.1, "cold: {:?}", gap.tint);
    }

    #[test]
    fn an_oasis_glows_warm_around_its_planetoid() {
        let id = find(RegionKind::Oasis, None);
        let (planet, _) = crate::world::planetoid_at(SEED, id).expect("oases have planetoids");
        let near = backdrop_at(SEED, planet);
        let far = backdrop_at(SEED, planet + Vec2::new(2_600.0, 0.0));
        assert!(near.glow > 0.9, "{}", near.glow);
        assert!(far.glow < 0.05, "{}", far.glow);
        let wash = near.layers().wash;
        assert!(wash[0] > wash[2], "warm: {wash:?}");
        assert!(far.layers().wash[3] < wash[3]);
    }

    #[test]
    fn a_confluence_has_two_hues_and_a_bright_core() {
        let conf = at(find(RegionKind::Confluence, None));
        let wild = at(find(RegionKind::Wild, None));
        assert!(conf.core > 0.95 && wild.core < 0.05);
        assert!(conf.star_density > 1.3);
        assert!(conf.texture.filament > 0.3);
        let apart: f32 = (0..3)
            .map(|k| (conf.tint[k] - conf.secondary[k]).abs())
            .sum();
        assert!(apart > 0.1, "{apart}");
        assert!(conf.layers().wash[3] > wild.layers().wash[3]);
    }

    #[test]
    fn biomes_have_their_own_grain() {
        let grain = |b| at(find(RegionKind::Wild, Some(b))).texture;
        assert_eq!(grain(BiomeKind::Plains).dominant(), Grain::Wisp);
        assert!(grain(BiomeKind::Plains).wisp > 0.9);
        assert_eq!(grain(BiomeKind::Predator).dominant(), Grain::Streak);
        assert_eq!(grain(BiomeKind::Strange).dominant(), Grain::Filament);
        // Every mix is a mix: the weights add up.
        for b in BiomeKind::ALL {
            let t = grain(b).array();
            assert!((t.iter().sum::<f32>() - 1.0).abs() < 1e-3, "{b:?} {t:?}");
        }
    }

    #[test]
    fn a_claim_has_haze_inside_and_a_stronger_edge_at_its_boundary() {
        let id = find(RegionKind::Civ, None);
        let inside = at(id);
        assert!(inside.haze > 0.9 && inside.claim_edge < 0.1, "{inside:?}");
        // Walk out of the territory along +x: the haze fades and the edge peaks on the way.
        let (mut peak, mut last) = (0.0f32, inside.haze);
        for step in 0..200 {
            let p = id.center() + Vec2::new(step as f32 * 150.0, 0.0);
            let b = backdrop_at(SEED, p);
            peak = peak.max(b.claim_edge);
            last = b.haze;
            if step > 40 && b.haze < 0.05 {
                break;
            }
        }
        assert!(peak > 0.9, "edge peaked at {peak}");
        assert!(last < 0.2, "haze never cleared: {last}");
        assert!(inside.layers().wash[3] > 0.0);
    }

    #[test]
    fn a_region_border_shows_a_seam_in_a_mix_of_both_colours() {
        // Two neighbouring sectors of different regions.
        let (a, b) = (-60..60)
            .flat_map(|x| (3..30).map(move |y| (x, y)))
            .find_map(|(x, y)| {
                let a = SectorId { x, y };
                let b = SectorId { x: x + 1, y };
                (ring(a) > 3 && region(SEED, a).key != region(SEED, b).key).then_some((a, b))
            })
            .expect("regions meet somewhere");
        let middle = (a.center() + b.center()) / 2.0;
        let on = backdrop_at(SEED, middle);
        let off = backdrop_at(SEED, a.center());
        assert!(on.edge > 0.9, "{}", on.edge);
        assert!(off.edge < 0.05, "{}", off.edge);
        // The seam is lit more on the border than in the middle of a region.
        assert!(on.layers().wash[3] > off.layers().wash[3]);
        // And on the way there the seam grows steadily.
        let mut prev = 0.0;
        for i in 0..=10 {
            let e = backdrop_at(SEED, a.center().lerp(middle, i as f32 / 10.0)).edge;
            assert!(e + 1e-4 >= prev, "edge fell from {prev} to {e}");
            prev = e;
        }
    }

    #[test]
    fn an_apex_sector_has_a_vignette() {
        let id = (3..40)
            .flat_map(|r| (-r..=r).flat_map(move |x| [(x, r), (x, -r), (r, x), (-r, x)]))
            .map(|(x, y)| SectorId { x, y })
            .find(|id| crate::apex::rank(SEED, *id) == Some(crate::apex::Rank::Major))
            .expect("an apex exists");
        let here = at(id);
        assert!(here.vignette > 0.95, "{}", here.vignette);
        assert!(here.layers().vignette > 0.95);
        assert!(vignette_alpha(1.0, 1.2) > vignette_alpha(1.0, 0.2));
        assert!(vignette_alpha(1.0, 3.0) <= MAX_VIGNETTE_ALPHA);
        assert_eq!(vignette_alpha(0.0, 1.0), 0.0);
        let away = backdrop_at(SEED, id.center() + Vec2::new(SECTOR_SIZE, 0.0));
        assert!(away.vignette < here.vignette);
    }

    #[test]
    fn the_wash_and_streaks_stay_capped_everywhere() {
        for x in -15..=15 {
            for y in -15..=15 {
                let p = Vec2::new(x as f32 * 3_100.0, y as f32 * 2_700.0);
                let l = backdrop_at(SEED, p).layers();
                assert!(l.wash[3] <= MAX_WASH_ALPHA);
                assert!(l.mid_streak[3] <= MAX_STREAK_ALPHA);
                assert!(l.mid_filament[3] <= MAX_LAYER_ALPHA);
                assert!((0.0..=1.0).contains(&l.vignette));
            }
        }
    }
}
