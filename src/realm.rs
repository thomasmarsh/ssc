//! Realms: the very large granularity layer over the map. Where a biome is a country of 8 to 20
//! sectors, a realm is a continent of roughly 40 to 120, cut by the same machinery (Voronoi
//! cells with additive weights and a noise-warped edge) at a much larger scale. Each realm has
//! a name, a colour and a **stress profile**: one or two primary axes of play it tests, a mild
//! secondary one, and a bundle of modifiers (`Effects`) on world rules and on enemy stats that
//! do not depend on depth. A build that carries you through one realm is weak in the next, so
//! no single way of getting powerful covers the whole map.
//!
//! The catalog is data (`CATALOG`): adding a realm kind is one table row. Every system reads a
//! realm through the one `Realm` view-model (`realm(seed, sector)`): species niche weights and
//! biome affinity (`species_weight`, `biome_weight`), apex archetype weights, enemy hull,
//! shield, speed, damage and armour (`Effects::foe`), the ship's weapon and sensor reach, wells,
//! jam time and mining richness. Nothing else decides what a realm does.
//!
//! Everything is a pure function of the master seed and a sector. HOME and everything near it
//! always lie in the starter realm (the Cradle), which applies nothing; a realm's effects also
//! fade in over the sectors next to a border (`EDGE_RAMP`) and past the starter (`FAR_RAMP`), so
//! crossing one is a slope and a modifier never steps. See `docs/UNIVERSE.md`, "Realms".

use crate::apex::Archetype;
use crate::biome::BiomeKind;
use crate::genome::{Diet, Genome, Social};
use crate::power::Power;
use crate::region::{harsh_name, soft_name};
use crate::world::{SectorId, hash2, value_noise};
use bevy::prelude::Vec2;
use std::cell::RefCell;
use std::collections::HashMap;

// ---- tuning: the layer ---------------------------------------------------------------------

/// Sectors on a side of the lattice that places one realm point each.
pub const REALM_CELL: f32 = 80.0;
/// The most a point's additive weight can add, in sectors: realms run about 40 to 120 across.
pub const REALM_WEIGHT: f32 = 22.0;
/// The warp of the realm edges: noise frequency per sector and amplitude in sectors.
pub const REALM_WARP_FREQUENCY: f32 = 0.03;
pub const REALM_WARP: f32 = 5.0;
/// The starter realm's point sits at HOME with this additive weight (a realm of its own).
pub const STARTER_WEIGHT: f32 = 50.0;
/// Rings inside which HOME's neighbourhood is the starter realm whatever the lattice says.
pub const STARTER_RINGS: f32 = 14.0;
/// Sectors of distance score over which a realm's effects rise from nothing at a border.
pub const EDGE_RAMP: f32 = 10.0;
/// A realm's effects also rise with depth: nothing up to `FAR_FROM` rings, full `FAR_RAMP` later.
pub const FAR_FROM: f32 = 16.0;
pub const FAR_RAMP: f32 = 12.0;
/// Bounds every multiplier stays inside, whatever a table row says.
pub const MIN_MULT: f32 = 0.1;
pub const MAX_MULT: f32 = 5.0;
/// The most any additive effect (flat armour, ability fizzle chance) can reach; a use site
/// clamps the fizzle chance to `MAX_FIZZLE` besides.
pub const MAX_PLATING: f32 = 12.0;
pub const MAX_FIZZLE: f32 = 0.6;
/// An effect smaller than this share does not make the details line.
pub const SHOWN_AT: f32 = 0.04;

const REALM_SALT: u64 = 0x5EA1_3000_0000_0061;
const NAME_SALT: u64 = 0x5EA1_4E41_0000_0063;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---- the axes ------------------------------------------------------------------------------

/// A dimension of play: what a ship can be built to be good at, and what a realm can test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    /// Per-hit and sustained damage.
    Damage,
    /// How far the ship can shoot and notice.
    Range,
    /// Shields, hull, armour and surviving a crowd.
    Defense,
    /// Speed, dash, handling.
    Mobility,
    /// Mining and rock work.
    Mining,
    /// Sensors, stealth and being unnoticed.
    Sensors,
    /// Organs, symbiotes and the powers they give.
    Symbiosis,
    /// Beacons, pads, and the energy abilities.
    Utility,
}

impl Axis {
    pub const ALL: [Axis; 8] = [
        Self::Damage,
        Self::Range,
        Self::Defense,
        Self::Mobility,
        Self::Mining,
        Self::Sensors,
        Self::Symbiosis,
        Self::Utility,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Damage => "DAMAGE",
            Self::Range => "RANGE",
            Self::Defense => "DEFENSE",
            Self::Mobility => "MOBILITY",
            Self::Mining => "MINING",
            Self::Sensors => "SENSORS",
            Self::Symbiosis => "SYMBIOSIS",
            Self::Utility => "UTILITY",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|a| *a == self).unwrap_or(0)
    }
}

// ---- effects ---------------------------------------------------------------------------------

/// What a realm does to the enemies born in it (carried by their `Phenotype`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foe {
    /// Multipliers on hull, shield, pace and the damage fired or rammed.
    pub hull: f32,
    pub shield: f32,
    pub speed: f32,
    pub damage: f32,
    /// A flat amount knocked off every one of the ship's hits (before depth): a light, fast gun
    /// is worth little against heavy plates, a big hit barely feels it.
    pub plating: f32,
}

impl Foe {
    pub const NEUTRAL: Self = Self {
        hull: 1.0,
        shield: 1.0,
        speed: 1.0,
        damage: 1.0,
        plating: 0.0,
    };
}

/// Every modifier a realm applies, at full strength for a table row and scaled by the realm's
/// intensity at a sector. All are multipliers (one is neutral) except `plating` and `fizzle`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    pub foe: Foe,
    /// Scales how far depth threat rises above one (below one lowers the threat).
    pub threat: f32,
    /// How many creatures a sector holds.
    pub life: f32,
    /// Niche weights: hunters and packs, schoolers and broods, jam carriers.
    pub predators: f32,
    pub swarms: f32,
    pub jammers: f32,
    /// Chance of an apex elder, and of gravity wells (count and odds).
    pub apex: f32,
    pub wells: f32,
    /// The ship's weapon reach and sensor reach.
    pub weapon_range: f32,
    pub sensor: f32,
    /// Pull of every gravity well.
    pub gravity: f32,
    /// What a mined rock yields.
    pub mining: f32,
    /// Chance (added) that a dash or parry fizzles when the key is pressed.
    pub fizzle: f32,
    /// How long a jam or confusion lasts.
    pub jam_time: f32,
}

impl Effects {
    pub const NEUTRAL: Self = Self {
        foe: Foe::NEUTRAL,
        threat: 1.0,
        life: 1.0,
        predators: 1.0,
        swarms: 1.0,
        jammers: 1.0,
        apex: 1.0,
        wells: 1.0,
        weapon_range: 1.0,
        sensor: 1.0,
        gravity: 1.0,
        mining: 1.0,
        fizzle: 0.0,
        jam_time: 1.0,
    };

    /// Each field, by name, as (label, value, neutral): for the lerp, the bounds and the details.
    fn fields(&self) -> [(&'static str, f32, f32); 18] {
        [
            ("ENEMY HULL", self.foe.hull, 1.0),
            ("ENEMY SHIELD", self.foe.shield, 1.0),
            ("ENEMY SPEED", self.foe.speed, 1.0),
            ("ENEMY DAMAGE", self.foe.damage, 1.0),
            ("ENEMY PLATING", self.foe.plating, 0.0),
            ("THREAT", self.threat, 1.0),
            ("CREATURES", self.life, 1.0),
            ("HUNTERS", self.predators, 1.0),
            ("SWARMS", self.swarms, 1.0),
            ("JAMMERS", self.jammers, 1.0),
            ("ELDERS", self.apex, 1.0),
            ("WELLS", self.wells, 1.0),
            ("WEAPON RANGE", self.weapon_range, 1.0),
            ("SENSORS", self.sensor, 1.0),
            ("GRAVITY", self.gravity, 1.0),
            ("MINING YIELD", self.mining, 1.0),
            ("ABILITY FIZZLE", self.fizzle, 0.0),
            ("JAM TIME", self.jam_time, 1.0),
        ]
    }

    fn map(&self, mut f: impl FnMut(f32, f32) -> f32) -> Self {
        let mut out = *self;
        let slots: [(&mut f32, f32); 18] = [
            (&mut out.foe.hull, 1.0),
            (&mut out.foe.shield, 1.0),
            (&mut out.foe.speed, 1.0),
            (&mut out.foe.damage, 1.0),
            (&mut out.foe.plating, 0.0),
            (&mut out.threat, 1.0),
            (&mut out.life, 1.0),
            (&mut out.predators, 1.0),
            (&mut out.swarms, 1.0),
            (&mut out.jammers, 1.0),
            (&mut out.apex, 1.0),
            (&mut out.wells, 1.0),
            (&mut out.weapon_range, 1.0),
            (&mut out.sensor, 1.0),
            (&mut out.gravity, 1.0),
            (&mut out.mining, 1.0),
            (&mut out.fizzle, 0.0),
            (&mut out.jam_time, 1.0),
        ];
        for (slot, neutral) in slots {
            *slot = f(*slot, neutral);
        }
        out
    }

    /// The effects at `intensity` (0 is neutral, 1 is the table row), held inside the bounds.
    pub fn scaled(&self, intensity: f32) -> Self {
        let t = intensity.clamp(0.0, 1.0);
        self.map(|v, neutral| {
            let v = if t >= 1.0 {
                v
            } else {
                neutral + (v - neutral) * t
            };
            if neutral == 0.0 {
                v.clamp(0.0, MAX_PLATING)
            } else {
                v.clamp(MIN_MULT, MAX_MULT)
            }
        })
    }

    /// Whether anything differs from neutral.
    pub fn is_neutral(&self) -> bool {
        *self == Self::NEUTRAL
    }
}

// ---- the catalog -----------------------------------------------------------------------------

/// One kind of realm: its identity and its stress profile. Add a kind by adding a row.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// Stable id (used by tests and the map).
    pub id: &'static str,
    /// The kind as the HUD and the map name it.
    pub title: &'static str,
    /// The word a realm's name ends in, and whether its phoneme name is harsh or soft.
    pub word: &'static str,
    pub harsh: bool,
    /// Colour identity: hue in [0, 1), saturation and lightness of the tint.
    pub hue: f32,
    pub sat: f32,
    pub light: f32,
    /// Weight in the lottery for a cell's kind (zero: never rolled).
    pub roll: f32,
    /// The axes this realm tests hardest, its mild secondary axis, and what it favours.
    pub primary: &'static [Axis],
    pub mild: Option<Axis>,
    pub favours: &'static [Axis],
    /// One line of what the realm is.
    pub blurb: &'static str,
    pub effects: Effects,
    /// Extra weight of a biome's kind in this realm.
    pub biomes: &'static [(BiomeKind, f32)],
    /// Extra weight of an apex archetype in this realm.
    pub archetypes: &'static [(Archetype, f32)],
    /// Powers an elder of this realm is likely to carry (see `apex`): the realm's signature.
    pub stamps: &'static [Power],
    /// Every elder of this realm wears a regenerating bubble (see `apexes::shield_factor`).
    pub bubbled: bool,
}

/// What a realm kind is, as an index into `CATALOG`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RealmKind(pub u8);

/// The realms. Order is stable (it keys the map and the hash lottery).
pub const CATALOG: [Spec; 10] = [
    Spec {
        id: "cradle",
        title: "THE CRADLE",
        word: "Cradle",
        harsh: false,
        hue: 0.55,
        sat: 0.35,
        light: 0.6,
        roll: 0.0,
        primary: &[],
        mild: None,
        favours: &[],
        blurb: "a gentle starter realm: no stress at all",
        effects: Effects::NEUTRAL,
        biomes: &[],
        archetypes: &[],
        stamps: &[],
        bubbled: false,
    },
    Spec {
        id: "veil",
        title: "THE VEIL",
        word: "Shroud",
        harsh: false,
        hue: 0.72,
        sat: 0.45,
        light: 0.55,
        roll: 1.0,
        primary: &[Axis::Range, Axis::Sensors],
        mild: Some(Axis::Defense),
        favours: &[Axis::Damage, Axis::Mobility],
        blurb: "dust cuts weapon range and sensors; enemies close in fast",
        effects: Effects {
            foe: Foe {
                speed: 1.2,
                damage: 1.1,
                ..Foe::NEUTRAL
            },
            weapon_range: 0.65,
            sensor: 0.55,
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Strange, 2.0), (BiomeKind::Keen, 1.5)],
        archetypes: &[(Archetype::Phantom, 3.0), (Archetype::Hunter, 2.0)],
        stamps: &[Power::Blink],
        bubbled: false,
    },
    Spec {
        id: "dead_reach",
        title: "DEAD REACH",
        word: "Stillness",
        harsh: true,
        hue: 0.08,
        sat: 0.5,
        light: 0.5,
        roll: 1.0,
        primary: &[Axis::Utility, Axis::Mobility],
        mild: Some(Axis::Sensors),
        favours: &[Axis::Damage, Axis::Mining],
        blurb: "jam carriers and elders everywhere; dash and parry fizzle; kinetic guns and mining tools carry you",
        effects: Effects {
            jammers: 4.0,
            fizzle: 0.25,
            jam_time: 1.4,
            mining: 1.25,
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Strange, 2.0), (BiomeKind::Hardy, 1.5)],
        archetypes: &[
            (Archetype::Maelstrom, 3.0),
            (Archetype::Warden, 2.5),
            (Archetype::Phantom, 2.0),
        ],
        stamps: &[Power::Emp, Power::Glare, Power::Confuse],
        bubbled: false,
    },
    Spec {
        id: "crush",
        title: "THE CRUSH",
        word: "Deep",
        harsh: true,
        hue: 0.8,
        sat: 0.5,
        light: 0.45,
        roll: 1.0,
        primary: &[Axis::Mobility],
        mild: Some(Axis::Range),
        favours: &[Axis::Mobility],
        blurb: "deep dynamic wells and heavy gravity punish slow ships",
        effects: Effects {
            gravity: 1.7,
            wells: 2.2,
            foe: Foe {
                speed: 0.95,
                ..Foe::NEUTRAL
            },
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Brutish, 2.0)],
        archetypes: &[(Archetype::Maelstrom, 3.0), (Archetype::Juggernaut, 1.5)],
        stamps: &[Power::Lens],
        bubbled: false,
    },
    Spec {
        id: "hive",
        title: "HIVE MARCHES",
        word: "Hive",
        harsh: false,
        hue: 0.14,
        sat: 0.6,
        light: 0.55,
        roll: 1.0,
        primary: &[Axis::Defense],
        mild: Some(Axis::Mobility),
        favours: &[Axis::Damage],
        blurb: "swarms, cords and queens: area weapons and tight defense shine",
        effects: Effects {
            swarms: 2.4,
            life: 1.4,
            predators: 0.7,
            foe: Foe {
                hull: 0.8,
                damage: 0.9,
                ..Foe::NEUTRAL
            },
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Plains, 3.0), (BiomeKind::Grazing, 2.0)],
        archetypes: &[(Archetype::Queen, 4.0), (Archetype::Lasher, 2.5)],
        stamps: &[Power::Split],
        bubbled: false,
    },
    Spec {
        id: "iron_tide",
        title: "IRON TIDE",
        word: "Bastion",
        harsh: true,
        hue: 0.6,
        sat: 0.12,
        light: 0.6,
        roll: 1.0,
        primary: &[Axis::Damage],
        mild: Some(Axis::Mining),
        favours: &[Axis::Defense, Axis::Mining],
        blurb: "armoured enemies behind strong shields: heavy hits or bypass, fast weak guns bounce",
        effects: Effects {
            foe: Foe {
                hull: 1.4,
                shield: 2.4,
                speed: 0.85,
                damage: 1.0,
                plating: 6.0,
            },
            mining: 1.3,
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Hardy, 3.0), (BiomeKind::Brutish, 2.0)],
        archetypes: &[(Archetype::Bulwark, 4.0), (Archetype::Juggernaut, 2.5)],
        stamps: &[Power::Bypass],
        bubbled: true,
    },
    Spec {
        id: "glass_seas",
        title: "GLASS SEAS",
        word: "Glass",
        harsh: false,
        hue: 0.5,
        sat: 0.6,
        light: 0.7,
        roll: 1.0,
        primary: &[Axis::Damage],
        mild: Some(Axis::Defense),
        favours: &[Axis::Mobility],
        blurb: "fragile, fast enemies in huge swarms: area and fast guns shine, heavy slow guns suffer",
        effects: Effects {
            foe: Foe {
                hull: 0.45,
                shield: 0.4,
                speed: 1.35,
                ..Foe::NEUTRAL
            },
            swarms: 1.8,
            life: 1.7,
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Plains, 2.0), (BiomeKind::Open, 1.5)],
        archetypes: &[(Archetype::Queen, 2.0), (Archetype::Phantom, 2.0)],
        stamps: &[Power::Phase],
        bubbled: false,
    },
    Spec {
        id: "quiet_gold",
        title: "QUIET GOLD",
        word: "Gold",
        harsh: false,
        hue: 0.12,
        sat: 0.7,
        light: 0.62,
        roll: 0.8,
        primary: &[],
        mild: None,
        favours: &[Axis::Mining, Axis::Utility],
        blurb: "rich and safe: the rest realm, lower threat and fat rocks",
        effects: Effects {
            threat: 0.6,
            mining: 1.6,
            life: 0.8,
            apex: 0.5,
            foe: Foe {
                damage: 0.85,
                ..Foe::NEUTRAL
            },
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Grazing, 2.0), (BiomeKind::Plains, 1.5)],
        archetypes: &[],
        stamps: &[],
        bubbled: false,
    },
    Spec {
        id: "hungry_deep",
        title: "HUNGRY DEEP",
        word: "Maw",
        harsh: true,
        hue: 0.0,
        sat: 0.6,
        light: 0.45,
        roll: 0.9,
        primary: &[Axis::Mobility, Axis::Defense],
        mild: Some(Axis::Sensors),
        favours: &[Axis::Damage, Axis::Range],
        blurb: "predators dominate and hunt in packs: you cannot be slow and cannot be soft",
        effects: Effects {
            predators: 3.2,
            swarms: 0.6,
            life: 0.85,
            foe: Foe {
                speed: 1.15,
                damage: 1.15,
                ..Foe::NEUTRAL
            },
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Predator, 4.0), (BiomeKind::Brutish, 1.5)],
        archetypes: &[
            (Archetype::Hunter, 4.0),
            (Archetype::Lasher, 2.0),
            (Archetype::Juggernaut, 1.5),
        ],
        stamps: &[Power::Blink],
        bubbled: false,
    },
    Spec {
        id: "bright_silence",
        title: "BRIGHT SILENCE",
        word: "Radiance",
        harsh: false,
        hue: 0.16,
        sat: 0.3,
        light: 0.85,
        roll: 0.8,
        primary: &[Axis::Damage, Axis::Defense],
        mild: Some(Axis::Utility),
        favours: &[Axis::Mobility, Axis::Sensors],
        blurb: "almost nothing lives here but the elders: a boss every few sectors",
        effects: Effects {
            life: 0.18,
            apex: 4.5,
            predators: 0.5,
            foe: Foe {
                hull: 1.15,
                ..Foe::NEUTRAL
            },
            ..Effects::NEUTRAL
        },
        biomes: &[(BiomeKind::Keen, 2.0), (BiomeKind::Hardy, 1.5)],
        archetypes: &[
            (Archetype::Warden, 3.0),
            (Archetype::Juggernaut, 2.0),
            (Archetype::Hunter, 1.5),
        ],
        stamps: &[Power::Blink, Power::Lens],
        bubbled: false,
    },
];

impl RealmKind {
    /// The starter realm's kind.
    pub const CRADLE: RealmKind = RealmKind(0);

    pub fn all() -> impl Iterator<Item = RealmKind> {
        (0..CATALOG.len() as u8).map(RealmKind)
    }

    pub fn spec(self) -> &'static Spec {
        &CATALOG[usize::from(self.0).min(CATALOG.len() - 1)]
    }

    pub fn by_id(id: &str) -> Option<RealmKind> {
        CATALOG
            .iter()
            .position(|s| s.id == id)
            .map(|i| RealmKind(i as u8))
    }
}

// ---- the layer --------------------------------------------------------------------------------

fn cell_point(seed: u64, cx: i32, cy: i32) -> (Vec2, f32, u64) {
    let h = hash2(seed ^ REALM_SALT, cx, cy);
    let unit = |shift: u32| ((h >> shift) & 0xFFFF) as f32 / 65_536.0;
    let at = Vec2::new(
        cx as f32 + unit(0) * 0.8 + 0.1,
        cy as f32 + unit(16) * 0.8 + 0.1,
    ) * REALM_CELL;
    (at, unit(32) * REALM_WEIGHT, h)
}

/// The kind a cell rolls, by the lottery weights of the catalog.
fn kind_of(hash: u64) -> RealmKind {
    let total: f32 = CATALOG.iter().map(|s| s.roll).sum();
    let mut roll = ((hash >> 40) & 0xFF_FFFF) as f32 / 16_777_216.0 * total;
    for (i, spec) in CATALOG.iter().enumerate() {
        if spec.roll <= 0.0 {
            continue;
        }
        roll -= spec.roll;
        if roll < 0.0 {
            return RealmKind(i as u8);
        }
    }
    RealmKind(1)
}

/// The identity of the starter realm's key (every sector of it shares it).
const STARTER_KEY: u64 = 1;

/// What the layer says about a sector, before names and effects are built.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Core {
    key: u64,
    kind: RealmKind,
    intensity: f32,
}

thread_local! {
    /// Memo of the layer: it is a pure function of seed and sector.
    static CORES: RefCell<HashMap<(u64, i32, i32), Core>> = RefCell::new(HashMap::new());
}

fn core(seed: u64, id: SectorId) -> Core {
    let memo = (seed, id.x, id.y);
    if let Some(hit) = CORES.with(|c| c.borrow().get(&memo).copied()) {
        return hit;
    }
    let made = compute(seed, id);
    CORES.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 60_000 {
            c.clear();
        }
        c.insert(memo, made);
    });
    made
}

fn compute(seed: u64, id: SectorId) -> Core {
    let raw = Vec2::new(id.x as f32, id.y as f32);
    let warp = Vec2::new(
        value_noise(seed ^ REALM_SALT, 1, raw * REALM_WARP_FREQUENCY) - 0.5,
        value_noise(seed ^ REALM_SALT, 2, raw * REALM_WARP_FREQUENCY) - 0.5,
    ) * (2.0 * REALM_WARP);
    let at = raw + warp;
    let (cx, cy) = (
        (at.x / REALM_CELL).floor() as i32,
        (at.y / REALM_CELL).floor() as i32,
    );
    let ring = crate::range::ring(id) as f32;
    // The starter realm is a point at HOME; it also holds the whole neighbourhood outright.
    let mut scores: Vec<(f32, u64)> = Vec::with_capacity(26);
    scores.push((at.length() - STARTER_WEIGHT, STARTER_KEY));
    for dx in -2..=2 {
        for dy in -2..=2 {
            let (point, weight, h) = cell_point(seed, cx + dx, cy + dy);
            scores.push((at.distance(point) - weight, h | 2));
        }
    }
    scores.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let (best, next) = (scores[0], scores[1]);
    if best.1 == STARTER_KEY || ring <= STARTER_RINGS {
        return Core {
            key: STARTER_KEY,
            kind: RealmKind::CRADLE,
            intensity: 0.0,
        };
    }
    let edge = smooth((next.0 - best.0) / EDGE_RAMP);
    let far = smooth((ring - FAR_FROM) / FAR_RAMP);
    Core {
        key: best.1,
        kind: kind_of(best.1),
        intensity: edge * far,
    }
}

/// A realm as the rest of the game sees it: the one view-model every system reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Realm {
    /// Stable identity, the same across the whole realm.
    pub key: u64,
    pub kind: RealmKind,
    pub name: String,
    /// How fully the realm's effects apply at this sector, in [0, 1] (zero in the starter realm
    /// and at a border, rising to one inside).
    pub intensity: f32,
    /// The effects at this sector (already scaled by `intensity`).
    pub effects: Effects,
}

impl Realm {
    fn of(c: Core) -> Self {
        let spec = c.kind.spec();
        let h = hash2(c.key ^ NAME_SALT, 5, 9);
        let base = if spec.harsh {
            harsh_name(h)
        } else {
            soft_name(h)
        };
        Self {
            key: c.key,
            kind: c.kind,
            name: format!("{base} {}", spec.word),
            intensity: c.intensity,
            effects: spec.effects.scaled(c.intensity),
        }
    }

    pub fn spec(&self) -> &'static Spec {
        self.kind.spec()
    }

    /// The kind's title (THE VEIL).
    pub fn title(&self) -> &'static str {
        self.spec().title
    }

    /// Whether this realm applies no stress here (the starter, a rest realm or a border).
    pub fn is_gentle(&self) -> bool {
        self.effects.is_neutral() || self.spec().primary.is_empty()
    }

    /// The axes this realm tests, primary first, as the HUD's stress line shows them.
    pub fn stress(&self) -> Vec<Axis> {
        let spec = self.spec();
        let mut axes: Vec<Axis> = spec.primary.to_vec();
        axes.extend(spec.mild);
        axes
    }

    /// The tint of the realm as sRGB in [0, 1]: its kind's colour with a small shift per realm.
    pub fn tint(&self) -> [f32; 3] {
        let spec = self.spec();
        let shift = ((self.key >> 20) & 0xFF) as f32 / 255.0 * 0.06 - 0.03;
        hsl(spec.hue + shift, spec.sat, spec.light)
    }

    /// The banner text for entering.
    pub fn banner(&self) -> String {
        format!("ENTERING THE REALM OF {}", self.name.to_uppercase())
    }

    /// What the realm tests, as a short line: the axes (the mild one in brackets).
    pub fn tests_line(&self) -> String {
        let spec = self.spec();
        if spec.primary.is_empty() {
            return "NO STRESS".to_string();
        }
        let mut axes: Vec<String> = spec.primary.iter().map(|a| a.label().to_string()).collect();
        axes.extend(spec.mild.map(|a| format!("({})", a.label())));
        format!("TESTS {}", axes.join(" "))
    }

    /// The stress as one line: the kind, then what it tests.
    pub fn stress_line(&self) -> String {
        format!("{}   {}", self.spec().title, self.tests_line())
    }

    /// What the realm changes here, as (label, change) pairs, biggest first: the details panel's
    /// lines. A multiplier reads as a signed percent, flat armour as points knocked off each
    /// hit, the fizzle chance as a percent chance. Empty where nothing differs by `SHOWN_AT`.
    pub fn changes(&self) -> Vec<(&'static str, String)> {
        let mut out: Vec<(&'static str, f32, String)> = self
            .effects
            .fields()
            .into_iter()
            .filter_map(|(label, v, neutral)| {
                let delta = v - neutral;
                if delta.abs() < SHOWN_AT {
                    return None;
                }
                let text = match label {
                    "ENEMY PLATING" => format!("-{v:.0} PER HIT"),
                    "ABILITY FIZZLE" => format!("{:.0}% OF PRESSES", v * 100.0),
                    _ => format!("{:+.0}%", delta * 100.0),
                };
                let weight = if neutral == 0.0 { delta / 6.0 } else { delta };
                Some((label, weight.abs(), text))
            })
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out.into_iter().map(|(l, _, t)| (l, t)).collect()
    }

    /// How much a species of this genome takes to the realm: hunters and packs, schoolers and
    /// broods, jam carriers each by their weight. One in the starter.
    pub fn species_weight(&self, g: &Genome) -> f32 {
        if self.intensity <= 0.0 {
            return 1.0;
        }
        let e = &self.effects;
        let mut w = 1.0;
        if g.diet == Diet::Hunt || g.social == Social::Pack {
            w *= e.predators;
        }
        if matches!(g.social, Social::School | Social::Brood) {
            w *= e.swarms;
        }
        if carries_jam(g) {
            w *= e.jammers;
        }
        w
    }

    /// The weight of the country the sector lies in (the realm's liking for a kind of biome).
    pub fn biome_weight(&self, kind: BiomeKind) -> f32 {
        let table = self.spec().biomes;
        let base = table
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(1.0, |(_, w)| *w);
        1.0 + (base - 1.0) * self.intensity
    }

    /// The weight of an elder archetype here.
    pub fn archetype_weight(&self, archetype: Archetype) -> f32 {
        let base = self
            .spec()
            .archetypes
            .iter()
            .find(|(a, _)| *a == archetype)
            .map_or(1.0, |(_, w)| *w);
        1.0 + (base - 1.0) * self.intensity
    }

    /// A power this realm's elders are stamped with, if any, from a roll in [0, 1). Zero
    /// intensity (the starter, a border) stamps nothing.
    pub fn stamp(&self, roll: f32) -> Option<Power> {
        let stamps = self.spec().stamps;
        if stamps.is_empty() || self.intensity < STAMP_FROM {
            return None;
        }
        Some(stamps[((roll * stamps.len() as f32) as usize).min(stamps.len() - 1)])
    }
}

/// How many of the eight axes a ship can be strong in at once (the design rule `no build covers every
/// realm` is checked against it).
pub const BUILD_AXES: usize = 3;

/// An elder only carries its realm's signature power where the realm is this strong.
pub const STAMP_FROM: f32 = 0.5;

/// Whether the genome carries a jam: an EMP, a glare or a confusion.
pub fn carries_jam(g: &Genome) -> bool {
    [Power::Emp, Power::Glare, Power::Confuse]
        .into_iter()
        .any(|p| p.active(g))
}

fn hsl(h: f32, s: f32, l: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
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

/// The realm of sector `id`. Pure.
pub fn realm(seed: u64, id: SectorId) -> Realm {
    Realm::of(core(seed, id))
}

/// The realm of sector `id` without its name spelled (the name is empty): for the generation
/// paths that only read weights and effects, which run for many sectors.
pub fn weighting(seed: u64, id: SectorId) -> Realm {
    let c = core(seed, id);
    Realm {
        key: c.key,
        kind: c.kind,
        name: String::new(),
        intensity: c.intensity,
        effects: c.kind.spec().effects.scaled(c.intensity),
    }
}

/// Only the effects at a sector (cheap: no name is spelled).
pub fn effects(seed: u64, id: SectorId) -> Effects {
    let c = core(seed, id);
    c.kind.spec().effects.scaled(c.intensity)
}

/// The enemy modifiers at a sector (what a creature born there wears).
pub fn foe(seed: u64, id: SectorId) -> Foe {
    effects(seed, id).foe
}

/// The sector's realm identity and kind without spelling a name: for maps and borders.
pub fn identity(seed: u64, id: SectorId) -> (u64, RealmKind, f32) {
    let c = core(seed, id);
    (c.key, c.kind, c.intensity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    const SEED: u64 = 0x535343;

    fn sectors(reach: i32, step: usize) -> impl Iterator<Item = SectorId> {
        (-reach..=reach).step_by(step).flat_map(move |x| {
            (-reach..=reach)
                .step_by(step)
                .map(move |y| SectorId { x, y })
        })
    }

    #[test]
    fn realms_are_pure_and_seeded() {
        for id in sectors(60, 7) {
            assert_eq!(realm(SEED, id), realm(SEED, id));
        }
        assert_ne!(
            realm(1, SectorId { x: 90, y: 40 }),
            realm(2, SectorId { x: 90, y: 40 })
        );
        // The memo changes nothing: a fresh computation agrees with it.
        for id in sectors(60, 9) {
            assert_eq!(core(SEED, id), compute(SEED, id));
        }
    }

    /// Realms run about 40 to 120 sectors across: the median realm side, and a spread of sizes.
    #[test]
    fn realms_are_continents_not_countries() {
        let mut area: HashMap<u64, u32> = HashMap::new();
        let step = 4;
        for id in sectors(520, step as usize) {
            *area.entry(realm(SEED, id).key).or_default() += 1;
        }
        let mut sides: Vec<f32> = area
            .values()
            .map(|a| (*a as f32).sqrt() * step as f32)
            .collect();
        sides.sort_by(f32::total_cmp);
        // Cells cut by the window's edge are small: judge the interior ones.
        let interior: Vec<f32> = sides.iter().copied().filter(|s| *s > 20.0).collect();
        let median = interior[interior.len() / 2];
        assert!((40.0..=120.0).contains(&median), "median side {median}");
        let (small, large) = (
            interior[interior.len() / 10],
            interior[interior.len() * 9 / 10],
        );
        assert!(large > 1.2 * small, "all realms alike: {small} to {large}");
        assert!(large < 170.0, "a realm {large} across");
        assert!(area.len() > 15, "{} realms", area.len());
    }

    #[test]
    fn home_is_in_the_gentle_starter_realm() {
        for seed in [SEED, 1, 7, 42, 99, 12345] {
            for id in sectors(14, 1) {
                let r = realm(seed, id);
                assert_eq!(r.kind, RealmKind::CRADLE, "{id:?} seed {seed}");
                assert!(r.effects.is_neutral() && r.is_gentle() && r.intensity == 0.0);
                assert_eq!(r.key, realm(seed, SectorId::ORIGIN).key);
                assert!(r.stress().is_empty());
            }
            // The starter is a real region of tens of sectors, not just the neighbourhood.
            let reach = sectors(30, 1)
                .filter(|id| realm(seed, *id).key == STARTER_KEY)
                .count();
            assert!(
                reach > 700,
                "seed {seed}: the starter holds {reach} sectors"
            );
        }
        let r = realm(SEED, SectorId::ORIGIN);
        assert!(r.name.ends_with(" Cradle"));
        assert_eq!(
            r.banner(),
            format!("ENTERING THE REALM OF {}", r.name.to_uppercase())
        );
    }

    #[test]
    fn every_catalog_kind_appears_over_a_large_area() {
        let mut seen: HashSet<RealmKind> = HashSet::new();
        for seed in [SEED, 1, 42] {
            for id in sectors(700, 7) {
                seen.insert(realm(seed, id).kind);
            }
        }
        for kind in RealmKind::all() {
            assert!(seen.contains(&kind), "{} never appears", kind.spec().id);
        }
        // Only the starter has no lottery weight, and a kind appears only if it has one.
        for (i, spec) in CATALOG.iter().enumerate() {
            assert_eq!(spec.roll > 0.0, i != 0, "{}", spec.id);
        }
    }

    #[test]
    fn the_catalog_is_well_formed() {
        let mut ids = HashSet::new();
        for spec in &CATALOG {
            assert!(ids.insert(spec.id), "duplicate id {}", spec.id);
            assert!(!spec.title.contains('\u{2014}') && !spec.blurb.contains('\u{2014}'));
            assert!(spec.primary.len() <= 2, "{}", spec.id);
            assert_eq!(
                spec.effects,
                spec.effects.scaled(1.0),
                "{} is out of bounds",
                spec.id
            );
            // A realm that stresses nothing changes little; one that stresses something does.
            assert_eq!(
                spec.primary.is_empty(),
                spec.id == "cradle" || spec.id == "quiet_gold"
            );
            for (_, w) in spec.biomes {
                assert!((1.0..=5.0).contains(w));
            }
        }
        assert_eq!(RealmKind::by_id("veil").unwrap().spec().title, "THE VEIL");
        // The headline realms exist with the effects the design asks for.
        let veil = RealmKind::by_id("veil").unwrap().spec().effects;
        assert!(veil.weapon_range < 0.8 && veil.sensor < 0.8);
        let dead = RealmKind::by_id("dead_reach").unwrap().spec().effects;
        assert!(dead.jammers > 2.0 && dead.fizzle > 0.0 && dead.fizzle <= MAX_FIZZLE);
        let crush = RealmKind::by_id("crush").unwrap().spec().effects;
        assert!(crush.gravity > 1.3 && crush.wells > 1.5);
        let iron = RealmKind::by_id("iron_tide").unwrap().spec().effects;
        assert!(iron.foe.shield > 2.0 && iron.foe.plating > 0.0);
        let glass = RealmKind::by_id("glass_seas").unwrap().spec().effects;
        assert!(glass.foe.hull < 0.6 && glass.foe.speed > 1.2 && glass.life > 1.4);
        let gold = RealmKind::by_id("quiet_gold").unwrap().spec().effects;
        assert!(gold.threat < 1.0 && gold.mining > 1.3);
        let deep = RealmKind::by_id("hungry_deep").unwrap().spec().effects;
        assert!(deep.predators > 2.0);
        let bright = RealmKind::by_id("bright_silence").unwrap().spec().effects;
        assert!(bright.life < 0.3 && bright.apex > 3.0);
    }

    /// Modifiers rise monotonically from neutral to the table row with intensity, and every one
    /// stays inside its bounds along the way and across the whole map.
    #[test]
    fn modifiers_are_monotonic_and_bounded() {
        for spec in &CATALOG {
            let mut last = Effects::NEUTRAL.scaled(0.0);
            assert!(last.is_neutral());
            for step in 1..=20 {
                let now = spec.effects.scaled(step as f32 / 20.0);
                for ((label, a, neutral), (_, b, _)) in last.fields().into_iter().zip(now.fields())
                {
                    assert!(
                        (b - neutral).abs() + 1e-6 >= (a - neutral).abs(),
                        "{} {label} not monotonic",
                        spec.id
                    );
                    if neutral == 0.0 {
                        assert!((0.0..=MAX_PLATING).contains(&b), "{} {label} {b}", spec.id);
                    } else {
                        assert!(
                            (MIN_MULT..=MAX_MULT).contains(&b),
                            "{} {label} {b}",
                            spec.id
                        );
                    }
                }
                last = now;
            }
            assert_eq!(last, spec.effects);
        }
        for id in sectors(500, 11) {
            let r = realm(SEED, id);
            assert!((0.0..=1.0).contains(&r.intensity));
            for (_, v, neutral) in r.effects.fields() {
                if neutral == 0.0 {
                    assert!(v >= 0.0);
                } else {
                    assert!((MIN_MULT..=MAX_MULT).contains(&v));
                }
            }
        }
    }

    /// No step across a border: effects change slowly from one sector to the next.
    #[test]
    fn a_border_is_a_slope() {
        let mut worst = 0.0_f32;
        let mut crossings = 0;
        for x in -300..300 {
            for y in (-300..300).step_by(13) {
                let (a, b) = (
                    realm(SEED, SectorId { x, y }),
                    realm(SEED, SectorId { x: x + 1, y }),
                );
                crossings += usize::from(a.key != b.key);
                worst = worst.max((a.intensity - b.intensity).abs());
            }
        }
        assert!(crossings > 20, "no borders crossed");
        assert!(worst < 0.5, "intensity steps by {worst}");
    }

    #[test]
    fn names_are_spelled_and_distinct() {
        let mut by_key: HashMap<u64, String> = HashMap::new();
        for id in sectors(500, 5) {
            let r = realm(SEED, id);
            assert!(r.name.is_ascii() && r.name.chars().next().unwrap().is_uppercase());
            assert!(r.name.ends_with(r.spec().word), "{}", r.name);
            assert_eq!(
                by_key.entry(r.key).or_insert_with(|| r.name.clone()),
                &r.name
            );
        }
        let distinct: HashSet<&String> = by_key.values().collect();
        assert!(
            distinct.len() * 10 >= by_key.len() * 9,
            "names repeat too much"
        );
    }

    #[test]
    fn species_biome_and_archetype_weights_follow_the_realm() {
        let at_full = |id: &str| -> Realm {
            let kind = RealmKind::by_id(id).unwrap();
            Realm {
                key: 9,
                kind,
                name: "Test".into(),
                intensity: 1.0,
                effects: kind.spec().effects,
            }
        };
        let dead = at_full("dead_reach");
        let mut jammer = Genome::default();
        Power::Emp.set(&mut jammer, 0.7);
        assert!(carries_jam(&jammer));
        assert!(dead.species_weight(&jammer) > 3.0);
        assert_eq!(dead.species_weight(&Genome::default()), 1.0);
        let hunter = Genome {
            diet: Diet::Hunt,
            ..Genome::default()
        };
        assert!(at_full("hungry_deep").species_weight(&hunter) > 3.0);
        assert!(at_full("hive").species_weight(&hunter) < 1.0);
        let school = Genome {
            social: Social::School,
            ..Genome::default()
        };
        assert!(at_full("hive").species_weight(&school) > 2.0);
        assert!(at_full("iron_tide").archetype_weight(Archetype::Bulwark) > 3.0);
        assert!(at_full("hive").archetype_weight(Archetype::Queen) > 3.0);
        assert!(at_full("hungry_deep").biome_weight(BiomeKind::Predator) > 3.0);
        // The starter never reweights anything.
        let home = realm(SEED, SectorId::ORIGIN);
        assert_eq!(home.species_weight(&jammer), 1.0);
        assert_eq!(home.archetype_weight(Archetype::Bulwark), 1.0);
        assert_eq!(home.biome_weight(BiomeKind::Hardy), 1.0);
        assert_eq!(home.stamp(0.3), None);
    }

    #[test]
    fn the_details_lines_name_what_changes() {
        let iron = RealmKind::by_id("iron_tide").unwrap();
        let r = Realm {
            key: 9,
            kind: iron,
            name: "Test Bastion".into(),
            intensity: 1.0,
            effects: iron.spec().effects,
        };
        let changes = r.changes();
        assert!(
            changes
                .iter()
                .any(|(l, d)| *l == "ENEMY SHIELD" && d == "+140%")
        );
        assert!(
            changes
                .iter()
                .any(|(l, d)| *l == "ENEMY PLATING" && d == "-6 PER HIT")
        );
        assert!(r.stress_line().contains("IRON TIDE") && r.stress_line().contains("DAMAGE"));
        assert!(realm(SEED, SectorId::ORIGIN).changes().is_empty());
        assert!(
            realm(SEED, SectorId::ORIGIN)
                .stress_line()
                .contains("NO STRESS")
        );
    }

    /// Sectors fully inside a realm of kind `id`, spread over the map (stepping out from HOME).
    fn inside(seed: u64, id: &str, want: usize) -> Vec<SectorId> {
        let kind = RealmKind::by_id(id).unwrap();
        let mut out = Vec::new();
        for x in (-500..=500).step_by(4) {
            for y in (-500..=500).step_by(4) {
                let s = SectorId { x, y };
                let (_, k, i) = identity(seed, s);
                if k == kind && i >= 0.95 {
                    out.push(s);
                }
            }
        }
        let stride = (out.len() / want).max(1);
        out.into_iter().step_by(stride).take(want).collect()
    }

    /// What a realm does to generation, measured: far fewer creatures in a bright silence and
    /// more elders, more jam carriers in the dead reach, more hunters in the hungry deep, more
    /// schoolers in the hive.
    #[test]
    fn realms_shape_who_lives_and_how_many_elders() {
        let creatures = |id: &str| -> f32 {
            let sectors = inside(SEED, id, 40);
            assert!(sectors.len() >= 20, "{id}: {} sectors", sectors.len());
            sectors
                .iter()
                .map(|s| {
                    crate::world::generate(SEED, *s)
                        .iter()
                        .filter(|sp| sp.species.is_some())
                        .count() as f32
                })
                .sum::<f32>()
                / sectors.len() as f32
        };
        let (bright, glass, hive, gold) = (
            creatures("bright_silence"),
            creatures("glass_seas"),
            creatures("hive"),
            creatures("quiet_gold"),
        );
        assert!(
            bright * 2.0 < glass.min(hive) && bright < gold,
            "{bright} {glass} {hive} {gold}"
        );

        let elders = |id: &str| -> usize {
            inside(SEED, id, 300)
                .iter()
                .filter(|s| crate::apex::rank(SEED, **s).is_some())
                .count()
        };
        assert!(
            elders("bright_silence") > 2 * elders("quiet_gold").max(2),
            "elders"
        );

        let share = |id: &str, f: &dyn Fn(&Genome) -> bool| -> f32 {
            let (mut hit, mut all) = (0.0, 0.0);
            for s in inside(SEED, id, 150) {
                for p in crate::range::ecology(SEED, s).presence {
                    all += p.weight;
                    if f(&p.species.genome) {
                        hit += p.weight;
                    }
                }
            }
            hit / all.max(1e-3)
        };
        let jam = |g: &Genome| carries_jam(g);
        let hunter = |g: &Genome| g.diet == Diet::Hunt || g.social == Social::Pack;
        let school = |g: &Genome| matches!(g.social, Social::School | Social::Brood);
        let base_jam = share("quiet_gold", &jam).max(0.005);
        assert!(share("dead_reach", &jam) > 2.0 * base_jam, "jam carriers");
        assert!(
            share("hungry_deep", &hunter) > 1.4 * share("hive", &hunter),
            "hunters"
        );
        assert!(
            share("hive", &school) > 1.3 * share("hungry_deep", &school),
            "schoolers"
        );
    }

    /// The archetype of an elder follows the realm: bulwarks in the iron tide, queens in the hive.
    #[test]
    fn elders_take_the_archetypes_of_their_realm() {
        let count = |id: &str, a: Archetype| -> f32 {
            let sectors = inside(SEED, id, 400);
            sectors
                .iter()
                .filter(|s| crate::apex::archetype(SEED, **s) == a)
                .count() as f32
                / sectors.len() as f32
        };
        assert!(count("iron_tide", Archetype::Bulwark) > 1.8 * count("hive", Archetype::Bulwark));
        assert!(count("hive", Archetype::Queen) > 1.8 * count("iron_tide", Archetype::Queen));
        assert!(count("hungry_deep", Archetype::Hunter) > 1.8 * count("hive", Archetype::Hunter));
    }

    /// The very large layer leaves HOME's neighbourhood exactly as it was: a rule of the start.
    #[test]
    fn the_starter_realm_changes_nothing_of_generation() {
        for id in sectors(14, 1) {
            let r = weighting(SEED, id);
            assert!(r.effects.is_neutral() && r.intensity == 0.0);
            assert_eq!(foe(SEED, id), Foe::NEUTRAL);
        }
    }

    #[test]
    fn colours_are_valid_and_kinds_differ() {
        let mut seen: Vec<[f32; 3]> = Vec::new();
        for kind in RealmKind::all() {
            let r = Realm {
                key: 77,
                kind,
                name: String::new(),
                intensity: 1.0,
                effects: Effects::NEUTRAL,
            };
            let c = r.tint();
            assert!(c.iter().all(|v| (0.0..=1.0).contains(v)), "{c:?}");
            seen.push(c);
        }
        for (i, a) in seen.iter().enumerate() {
            for b in &seen[i + 1..] {
                let d: f32 = a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum();
                assert!(d > 0.05, "two realm kinds share a colour");
            }
        }
    }

    /// The rule: no build covers every realm. A ship gets strong in about `BUILD_AXES` of the
    /// eight axes; whichever it picks, some realm stresses a primary axis it left out, and every
    /// combat axis is both tested in one realm and favoured in another.
    #[test]
    fn no_build_covers_every_realm() {
        let primary: Vec<Axis> = CATALOG
            .iter()
            .flat_map(|s| s.primary.iter().copied())
            .collect();
        let mut stressed: Vec<Axis> = primary.clone();
        stressed.sort();
        stressed.dedup();
        assert!(stressed.len() > BUILD_AXES, "{stressed:?}");
        // Every way of picking `BUILD_AXES` axes leaves some realm's primary stress uncovered.
        let n = Axis::ALL.len();
        for a in 0..n {
            for b in a + 1..n {
                for c in b + 1..n {
                    let build = [Axis::ALL[a], Axis::ALL[b], Axis::ALL[c]];
                    assert!(
                        CATALOG
                            .iter()
                            .any(|s| s.primary.iter().any(|p| !build.contains(p))),
                        "{build:?} covers every realm"
                    );
                    // And no build is safe everywhere: some realm has every primary axis outside it.
                    assert!(
                        CATALOG.iter().any(|s| !s.primary.is_empty()
                            && s.primary.iter().all(|p| !build.contains(p))),
                        "{build:?} is untouched by some realm"
                    );
                }
            }
        }
        // Every combat axis is tested somewhere, and each one is a strength elsewhere.
        for axis in [
            Axis::Damage,
            Axis::Range,
            Axis::Defense,
            Axis::Mobility,
            Axis::Sensors,
            Axis::Utility,
        ] {
            assert!(primary.contains(&axis), "{axis:?} is never primary");
            assert!(
                CATALOG.iter().any(|s| s.favours.contains(&axis)),
                "{axis:?} is never favoured"
            );
        }
    }
}
