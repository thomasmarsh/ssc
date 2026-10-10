//! Ship augmentation as a composable system. Nothing here is a fixed "upgrade": a ship's
//! abilities are the sum of the `Effect`s carried by the parts bolted to it (permanent,
//! limited by mounting slots), the weapon profile it has switched to and the boosts running
//! on its cargo (see `arsenal`). An effect either scales a `Stat` or grants levels of a
//! `Trait`; parts and surges are assembled from a blueprint, a rarity, a grade (how deep the
//! source was) and rolled affixes, so the same few ingredients yield a wide range of items.
//! `Stats::compute` folds them into what the simulation reads. Pure data and deterministic
//! rolls; no game state lives here.
//!
//! Weapon abilities are never lost: a part or surge that carries a weapon trait unlocks (or
//! levels up) the matching `Profile` in the arsenal instead of occupying a slot, and finding
//! gear can never lower the ship's firepower.

pub use super::arsenal::Gain;
use super::arsenal::{Arsenal, Boost, BoostGain, Need, Profile};
use super::{Material, Tunables};
use crate::genome::{Diet, Genome, Weapon};
use crate::world::{Rng, SectorParams};

/// Scalable ship statistics. An effect's amount is a fraction of the base value
/// (+0.25 is 25 percent more).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Stat {
    Thrust,
    TopSpeed,
    Handling,
    FireRate,
    Damage,
    ShotSpeed,
    Range,
    Hull,
    Shield,
    Recharge,
    Armor,
    Magnet,
}

impl Stat {
    pub const ALL: [Stat; 12] = [
        Self::Thrust,
        Self::TopSpeed,
        Self::Handling,
        Self::FireRate,
        Self::Damage,
        Self::ShotSpeed,
        Self::Range,
        Self::Hull,
        Self::Shield,
        Self::Recharge,
        Self::Armor,
        Self::Magnet,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Thrust => "thrust",
            Self::TopSpeed => "top speed",
            Self::Handling => "handling",
            Self::FireRate => "fire rate",
            Self::Damage => "damage",
            Self::ShotSpeed => "shot speed",
            Self::Range => "range",
            Self::Hull => "hull",
            Self::Shield => "shield",
            Self::Recharge => "recharge",
            Self::Armor => "armor",
            Self::Magnet => "magnet",
        }
    }

    fn adjective(self) -> &'static str {
        match self {
            Self::Thrust => "Surging",
            Self::TopSpeed => "Swift",
            Self::Handling => "Nimble",
            Self::FireRate => "Rapid",
            Self::Damage => "Brutal",
            Self::ShotSpeed => "Hot",
            Self::Range => "Far",
            Self::Hull => "Stout",
            Self::Shield => "Warded",
            Self::Recharge => "Lively",
            Self::Armor => "Plated",
            Self::Magnet => "Sticky",
        }
    }

    /// Fraction a grade-1 affix adds.
    fn affix_bonus(self) -> f32 {
        match self {
            Self::Thrust | Self::Handling | Self::Damage => 0.15,
            Self::TopSpeed => 0.1,
            Self::FireRate | Self::ShotSpeed | Self::Range => 0.14,
            Self::Hull => 0.2,
            Self::Shield => 0.25,
            Self::Recharge => 0.35,
            Self::Armor => 0.1,
            Self::Magnet => 0.4,
        }
    }

    /// How much a point of this stat is worth when ranking parts and rating a ship.
    fn worth(self) -> f32 {
        match self {
            Self::Thrust => 0.6,
            Self::TopSpeed => 0.5,
            Self::Handling => 0.4,
            Self::FireRate | Self::Damage => 1.0,
            Self::ShotSpeed | Self::Range => 0.3,
            Self::Hull => 0.8,
            Self::Shield => 0.7,
            Self::Recharge => 0.3,
            Self::Armor => 1.2,
            Self::Magnet => 0.15,
        }
    }
}

/// Abilities with levels. Levels from every part and surge add up, up to a cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Trait {
    /// Extra pairs of shots fanned around the nose.
    Spread,
    /// Shots pass through this many bodies.
    Pierce,
    /// Shots bend toward the nearest hostile.
    Homing,
    /// Guns on the flanks.
    Broadside,
    /// A gun at the stern.
    Tailgun,
    /// Shots burst on impact.
    Blast,
    /// Ramming hurts what you ram.
    Ram,
    /// Hostile cords that latch on are cut at once.
    Shears,
    /// Gravity wells barely tug and cannot hurt.
    Ballast,
    /// Kills mend the hull.
    Siphon,
    /// A lunatic field: whatever touches the ship is flung, and contact does no harm.
    Aura,
    /// Homing missile salvos while firing.
    Missiles,
    /// Lays mines behind the ship when hostiles are near.
    Mines,
    /// The main gun becomes a dense burst of thin particles.
    Needles,
    /// Pulses a ring of shots when hostiles are close.
    Nova,
}

impl Trait {
    pub const ALL: [Trait; 15] = [
        Self::Spread,
        Self::Pierce,
        Self::Homing,
        Self::Broadside,
        Self::Tailgun,
        Self::Blast,
        Self::Ram,
        Self::Shears,
        Self::Ballast,
        Self::Siphon,
        Self::Aura,
        Self::Missiles,
        Self::Mines,
        Self::Needles,
        Self::Nova,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Spread => "spread",
            Self::Pierce => "pierce",
            Self::Homing => "homing",
            Self::Broadside => "broadside",
            Self::Tailgun => "tail gun",
            Self::Blast => "blast",
            Self::Ram => "ram",
            Self::Shears => "cord shears",
            Self::Ballast => "ballast",
            Self::Siphon => "siphon",
            Self::Aura => "lunatic field",
            Self::Missiles => "missiles",
            Self::Mines => "mines",
            Self::Needles => "needles",
            Self::Nova => "nova",
        }
    }

    pub fn cap(self) -> u8 {
        match self {
            Self::Spread
            | Self::Homing
            | Self::Blast
            | Self::Ram
            | Self::Aura
            | Self::Missiles
            | Self::Mines
            | Self::Needles
            | Self::Nova => 3,
            Self::Pierce => 4,
            Self::Broadside => 2,
            Self::Tailgun | Self::Shears | Self::Ballast => 1,
            Self::Siphon => 4,
        }
    }

    fn worth(self) -> f32 {
        match self {
            Self::Spread => 0.45,
            Self::Pierce | Self::Broadside | Self::Blast | Self::Nova => 0.4,
            Self::Missiles | Self::Needles => 0.5,
            Self::Mines => 0.35,
            Self::Homing | Self::Siphon => 0.35,
            Self::Ram | Self::Shears | Self::Aura => 0.3,
            Self::Tailgun | Self::Ballast => 0.25,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Effect {
    Stat(Stat, f32),
    Trait(Trait, u8),
}

impl Effect {
    pub fn describe(&self) -> String {
        match *self {
            Self::Stat(stat, amount) => format!("{:+.0}% {}", amount * 100.0, stat.label()),
            Self::Trait(kind, 1) => kind.label().to_string(),
            Self::Trait(kind, level) => format!("{} {level}", kind.label()),
        }
    }

    pub(super) fn rating(&self) -> f32 {
        match *self {
            Self::Stat(stat, amount) => amount * stat.worth(),
            Self::Trait(kind, level) => f32::from(level) * kind.worth(),
        }
    }
}

/// Where a permanent part is bolted on. Each slot holds a few parts; a better part
/// displaces the weakest one when its slot is full.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Slot {
    Cannon,
    Engine,
    Plating,
    Core,
    Aux,
}

impl Slot {
    pub const ALL: [Slot; 5] = [
        Self::Cannon,
        Self::Engine,
        Self::Plating,
        Self::Core,
        Self::Aux,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    pub fn capacity(self) -> usize {
        match self {
            Self::Cannon => 3,
            _ => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cannon => "cannon",
            Self::Engine => "engine",
            Self::Plating => "plating",
            Self::Core => "core",
            Self::Aux => "aux",
        }
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
}

impl Rarity {
    pub const ALL: [Rarity; 4] = [Self::Common, Self::Uncommon, Self::Rare, Self::Epic];

    pub fn label(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
        }
    }

    pub fn color(self) -> [f32; 3] {
        match self {
            Self::Common => [0.72, 0.8, 0.88],
            Self::Uncommon => [0.35, 0.95, 0.5],
            Self::Rare => [0.35, 0.65, 1.0],
            Self::Epic => [1.0, 0.55, 0.95],
        }
    }

    /// Rolled affixes on top of the blueprint's own effects.
    fn affixes(self) -> usize {
        self as usize
    }

    fn strength(self) -> f32 {
        match self {
            Self::Common => 1.0,
            Self::Uncommon => 1.2,
            Self::Rare => 1.45,
            Self::Epic => 1.8,
        }
    }
}

/// What problem a blueprint is a good answer to: it drops more often where the matching
/// sector parameter runs high.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answers {
    Anything,
    Distortion,
    Tech,
    Swarm,
    Aggression,
    Danger,
}

struct Blueprint {
    name: &'static str,
    slot: Slot,
    effects: &'static [Effect],
    answers: Answers,
    /// What a boost burns, per second, and when. None for parts and for weapon charges.
    boost: Option<(Material, f32, Need)>,
}

const fn part(
    name: &'static str,
    slot: Slot,
    answers: Answers,
    effects: &'static [Effect],
) -> Blueprint {
    Blueprint {
        name,
        slot,
        effects,
        answers,
        boost: None,
    }
}

/// A surge that carries a weapon trait: a charge that unlocks or levels up that profile.
const fn charge(
    name: &'static str,
    slot: Slot,
    answers: Answers,
    effects: &'static [Effect],
) -> Blueprint {
    part(name, slot, answers, effects)
}

/// A surge that is a boost: it burns `drain` of `material` per second while `need` holds.
const fn boost(
    name: &'static str,
    slot: Slot,
    (material, drain, need): (Material, f32, Need),
    answers: Answers,
    effects: &'static [Effect],
) -> Blueprint {
    Blueprint {
        name,
        slot,
        effects,
        answers,
        boost: Some((material, drain, need)),
    }
}

use Effect::{Stat as S, Trait as T};
use Slot::{Aux, Cannon, Core, Engine, Plating};

const PARTS: &[Blueprint] = &[
    part(
        "Missile Pods",
        Cannon,
        Answers::Danger,
        &[T(Trait::Missiles, 1)],
    ),
    part(
        "Needler Rack",
        Cannon,
        Answers::Tech,
        &[T(Trait::Needles, 1), S(Stat::ShotSpeed, 0.1)],
    ),
    part(
        "Mine Layer",
        Aux,
        Answers::Aggression,
        &[T(Trait::Mines, 1)],
    ),
    part(
        "Twin Barrels",
        Cannon,
        Answers::Swarm,
        &[T(Trait::Spread, 1)],
    ),
    part(
        "Rapid Coil",
        Cannon,
        Answers::Anything,
        &[S(Stat::FireRate, 0.3)],
    ),
    part(
        "Heavy Bore",
        Cannon,
        Answers::Danger,
        &[S(Stat::Damage, 0.4), S(Stat::FireRate, -0.1)],
    ),
    part(
        "Lance",
        Cannon,
        Answers::Danger,
        &[T(Trait::Pierce, 1), S(Stat::ShotSpeed, 0.2)],
    ),
    part(
        "Seeker Rack",
        Cannon,
        Answers::Swarm,
        &[T(Trait::Homing, 1)],
    ),
    part(
        "Broadside Pods",
        Cannon,
        Answers::Swarm,
        &[T(Trait::Broadside, 1)],
    ),
    part(
        "Tail Gun",
        Cannon,
        Answers::Aggression,
        &[T(Trait::Tailgun, 1)],
    ),
    part(
        "Blast Charge",
        Cannon,
        Answers::Swarm,
        &[T(Trait::Blast, 1), S(Stat::Damage, -0.05)],
    ),
    part(
        "Long Barrel",
        Cannon,
        Answers::Anything,
        &[S(Stat::Range, 0.35), S(Stat::ShotSpeed, 0.25)],
    ),
    part(
        "Ion Thruster",
        Engine,
        Answers::Anything,
        &[S(Stat::Thrust, 0.35)],
    ),
    part(
        "Afterburner",
        Engine,
        Answers::Aggression,
        &[S(Stat::TopSpeed, 0.22), S(Stat::Thrust, 0.1)],
    ),
    part(
        "Gyro Vane",
        Engine,
        Answers::Anything,
        &[S(Stat::Handling, 0.35)],
    ),
    part(
        "Drive Spine",
        Engine,
        Answers::Anything,
        &[
            S(Stat::TopSpeed, 0.12),
            S(Stat::Thrust, 0.2),
            S(Stat::Handling, 0.15),
        ],
    ),
    part(
        "Hull Plating",
        Plating,
        Answers::Danger,
        &[S(Stat::Hull, 0.4)],
    ),
    part(
        "Reactive Armor",
        Plating,
        Answers::Aggression,
        &[S(Stat::Armor, 0.28)],
    ),
    part(
        "Ram Prow",
        Plating,
        Answers::Aggression,
        &[T(Trait::Ram, 1), S(Stat::Hull, 0.1)],
    ),
    part(
        "Shield Cell",
        Core,
        Answers::Anything,
        &[S(Stat::Shield, 0.45)],
    ),
    part(
        "Capacitor",
        Core,
        Answers::Anything,
        &[S(Stat::Recharge, 0.8), S(Stat::Shield, 0.1)],
    ),
    part("Siphon Core", Core, Answers::Danger, &[T(Trait::Siphon, 1)]),
    part(
        "Aegis Matrix",
        Core,
        Answers::Aggression,
        &[S(Stat::Shield, 0.25), S(Stat::Armor, 0.12)],
    ),
    part(
        "Tractor Dish",
        Aux,
        Answers::Anything,
        &[S(Stat::Magnet, 0.8)],
    ),
    part(
        "Ballast Keel",
        Aux,
        Answers::Distortion,
        &[T(Trait::Ballast, 1)],
    ),
    part("Cord Shears", Aux, Answers::Tech, &[T(Trait::Shears, 1)]),
    part(
        "Chaos Coil",
        Aux,
        Answers::Aggression,
        &[T(Trait::Aura, 1), S(Stat::Handling, -0.15)],
    ),
];

const SURGES: &[Blueprint] = &[
    // Weapon charges: each unlocks or levels up its profile and brings fuel for it.
    charge(
        "Missile Barrage",
        Cannon,
        Answers::Danger,
        &[T(Trait::Missiles, 2)],
    ),
    charge(
        "Needle Storm",
        Cannon,
        Answers::Tech,
        &[T(Trait::Needles, 2)],
    ),
    charge("Nova Pulse", Core, Answers::Swarm, &[T(Trait::Nova, 1)]),
    charge("Minefield", Aux, Answers::Aggression, &[T(Trait::Mines, 2)]),
    charge(
        "Scatter Burst",
        Cannon,
        Answers::Swarm,
        &[T(Trait::Spread, 2)],
    ),
    charge(
        "Needle Rounds",
        Cannon,
        Answers::Danger,
        &[T(Trait::Pierce, 2)],
    ),
    charge(
        "Hunter Swarm",
        Cannon,
        Answers::Swarm,
        &[T(Trait::Homing, 2)],
    ),
    // Boosts: owned for the run once found, they burn cargo while they are in use.
    boost(
        "Overdrive",
        Cannon,
        (Material::Metal, 1.2, Need::Firing),
        Answers::Swarm,
        &[S(Stat::FireRate, 0.7)],
    ),
    boost(
        "Amplifier",
        Cannon,
        (Material::Metal, 1.2, Need::Firing),
        Answers::Danger,
        &[S(Stat::Damage, 0.9)],
    ),
    boost(
        "Afterglow",
        Engine,
        (Material::Volatiles, 1.5, Need::Thrusting),
        Answers::Aggression,
        &[S(Stat::TopSpeed, 0.45), S(Stat::Thrust, 0.6)],
    ),
    boost(
        "Aegis Field",
        Plating,
        (Material::Crystal, 0.8, Need::Danger),
        Answers::Aggression,
        &[S(Stat::Armor, 1.0)],
    ),
    boost(
        "Overshield",
        Core,
        (Material::Crystal, 0.8, Need::Danger),
        Answers::Anything,
        &[S(Stat::Shield, 1.5), S(Stat::Recharge, 1.5)],
    ),
    boost(
        "Magnet Storm",
        Aux,
        (Material::Crystal, 0.6, Need::Loot),
        Answers::Anything,
        &[S(Stat::Magnet, 3.0)],
    ),
    boost(
        "Lunatic Field",
        Aux,
        (Material::Crystal, 1.0, Need::Danger),
        Answers::Aggression,
        &[T(Trait::Aura, 2), S(Stat::Armor, 0.3)],
    ),
    boost(
        "Gravity Boots",
        Aux,
        (Material::Volatiles, 0.5, Need::Wells),
        Answers::Distortion,
        &[T(Trait::Ballast, 1)],
    ),
    boost(
        "Shear Pulse",
        Aux,
        (Material::Metal, 0.5, Need::Cords),
        Answers::Tech,
        &[T(Trait::Shears, 1)],
    ),
];

/// A permanent part, bolted to one slot of the ship.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Part {
    pub name: String,
    pub slot: Slot,
    pub rarity: Rarity,
    /// The threat of the place it came from; deeper parts are stronger.
    pub grade: f32,
    pub effects: Vec<Effect>,
    /// The blueprint's name without rolled adjectives (empty: use `name`), and how many of
    /// the leading effects are the blueprint's own (the rest are rolled affixes the bench
    /// may reforge). `usize::MAX` means every effect is the blueprint's.
    pub stem: String,
    pub core: usize,
}

/// "pierce, +24% damage": what a set of effects does, for display.
pub fn summarize(effects: &[Effect]) -> String {
    effects
        .iter()
        .map(Effect::describe)
        .collect::<Vec<_>>()
        .join(", ")
}

impl Part {
    pub fn rating(&self) -> f32 {
        self.effects.iter().map(Effect::rating).sum()
    }

    pub fn summary(&self) -> String {
        summarize(&self.effects)
    }
}

impl Surge {
    pub fn summary(&self) -> String {
        summarize(&self.effects)
    }

    /// The weapon profiles this surge unlocks or levels up (empty for a boost).
    pub fn profiles(&self) -> Vec<(Profile, u8)> {
        self.effects
            .iter()
            .filter_map(|e| match *e {
                Effect::Trait(kind, level) => Some((Profile::from_trait(kind)?, level)),
                Effect::Stat(..) => None,
            })
            .collect()
    }
}

/// A pickup that charges the ship: weapon traits unlock or level up the matching profile,
/// anything else becomes a boost the ship owns from then on. Either way it brings fuel.
/// (Formerly a timed surge; nothing runs on a countdown any more.)
#[derive(Clone, Debug, PartialEq)]
pub struct Surge {
    pub name: String,
    pub slot: Slot,
    pub rarity: Rarity,
    pub effects: Vec<Effect>,
    /// The material it is fuel for, and how much comes aboard.
    pub material: Material,
    pub fuel: f32,
    /// For a boost: material per second while running, and when.
    pub drain: f32,
    pub need: Need,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Hull points restored at once.
    Repair(f32),
    /// Shield points restored at once.
    Recharge(f32),
    Life,
    /// Raw material for the ship's cargo hold (and a small score bonus of about its amount).
    Material(Material, f32),
    Part(Part),
    Surge(Surge),
    /// A specimen of another creature's organ (see `organs`).
    Specimen(super::organs::Strain),
    /// A plant seed (species and crop genes), carried by creatures that eat that plant.
    Seed(crate::flora::SeedKind),
}

impl Item {
    pub fn rarity(&self) -> Rarity {
        match self {
            Self::Part(p) => p.rarity,
            Self::Surge(s) => s.rarity,
            Self::Specimen(_) => Rarity::Epic,
            Self::Life => Rarity::Epic,
            _ => Rarity::Common,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Repair(_) => "Hull repair".into(),
            Self::Recharge(_) => "Shield charge".into(),
            Self::Life => "Extra life".into(),
            Self::Material(kind, amount) => format!("{} +{amount:.0}", kind.label()),
            Self::Part(p) => p.name.clone(),
            Self::Surge(s) => s.name.clone(),
            Self::Specimen(s) => format!("Specimen: {}", s.organ.label()),
            Self::Seed(_) => "Seed".into(),
        }
    }
}

/// What happened when a part was offered to the ship.
#[derive(Debug, PartialEq)]
pub enum Install {
    Added,
    Replaced(Part),
    /// The slot was full of better parts, or the part would have lowered the ship's
    /// firepower; the newcomer was broken down for scrap.
    Scrapped(Part),
}

/// What acquiring a part did: weapon profiles unlocked or leveled, and the slot outcome of
/// whatever was left of the part (None when it was all weapon).
#[derive(Debug, PartialEq)]
pub struct Acquired {
    pub profiles: Vec<(Profile, Gain)>,
    pub installed: Option<Install>,
}

/// What a charged pickup did.
#[derive(Debug, PartialEq)]
pub enum Charged {
    Profiles(Vec<(Profile, Gain)>),
    Boost(BoostGain),
}

/// Everything attached to the ship.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Loadout {
    #[serde(default)]
    pub research: super::research::Research,
    /// Supplier-commissioned baseline; bounded patterns and handling remain independent.
    #[serde(default)]
    pub equipment_grade: f32,
    pub parts: Vec<Part>,
    pub arsenal: Arsenal,
    /// Rig upgrades bought at the bench (mining, later parry and dash).
    pub skills: super::skills::Skills,
    /// Organ strains owned and fitted (see `organs`).
    pub organs: super::organs::Organs,
}

impl Loadout {
    pub fn in_slot(&self, slot: Slot) -> impl Iterator<Item = &Part> {
        self.parts.iter().filter(move |p| p.slot == slot)
    }

    /// Stats from the bolted-on parts alone (no weapon profile, no boosts).
    pub fn gear_stats(&self) -> Stats {
        self.apply_grade(Stats::compute(
            self.parts.iter().flat_map(|p| p.effects.iter().copied()),
        ))
    }

    /// Bolts a part on. A part that would lower the ship's firepower is scrapped instead,
    /// so no pickup can leave the ship weaker.
    pub fn install(&mut self, part: Part) -> Install {
        let before = self.gear_stats().firepower();
        let held = self.in_slot(part.slot).count();
        if held < part.slot.capacity() {
            self.parts.push(part);
            if self.gear_stats().firepower() + 1e-4 < before
                && let Some(part) = self.parts.pop()
            {
                return Install::Scrapped(part);
            }
            return Install::Added;
        }
        let weakest = self
            .parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.slot == part.slot)
            .min_by(|a, b| a.1.rating().total_cmp(&b.1.rating()))
            .map(|(i, _)| i);
        match weakest {
            Some(i) if self.parts[i].rating() < part.rating() => {
                let old = std::mem::replace(&mut self.parts[i], part);
                if self.gear_stats().firepower() + 1e-4 < before {
                    let part = std::mem::replace(&mut self.parts[i], old);
                    return Install::Scrapped(part);
                }
                Install::Replaced(old)
            }
            _ => Install::Scrapped(part),
        }
    }

    /// Takes a part aboard. Its weapon traits go to the arsenal (unlocking or leveling the
    /// profile, never losing anything) together with the penalties that came with them; what
    /// is left, if anything, is bolted on as an ordinary part.
    pub fn acquire(&mut self, mut part: Part) -> Acquired {
        let mut profiles = Vec::new();
        let mut stripped = false;
        part.effects.retain(|effect| match *effect {
            Effect::Trait(kind, level) => match Profile::from_trait(kind) {
                Some(profile) => {
                    profiles.push((profile, self.arsenal.acquire(profile, level)));
                    stripped = true;
                    false
                }
                None => true,
            },
            Effect::Stat(..) => true,
        });
        if stripped {
            part.effects
                .retain(|e| !matches!(e, Effect::Stat(_, amount) if *amount < 0.0));
        }
        let installed = (!part.effects.is_empty()).then(|| self.install(part));
        Acquired {
            profiles,
            installed,
        }
    }

    /// Takes a charged pickup aboard: weapon traits level up profiles, anything else becomes
    /// a boost (the stronger of two of the same name is kept). The fuel is the caller's.
    pub fn charge(&mut self, surge: &Surge) -> Charged {
        let profiles = surge.profiles();
        if !profiles.is_empty() {
            return Charged::Profiles(
                profiles
                    .into_iter()
                    .map(|(profile, level)| (profile, self.arsenal.acquire(profile, level)))
                    .collect(),
            );
        }
        Charged::Boost(self.arsenal.add_boost(Boost {
            name: surge.name.clone(),
            slot: surge.slot,
            rarity: surge.rarity,
            effects: surge.effects.clone(),
            material: Material::Fuel,
            drain: surge.drain,
            need: surge.need,
            running: false,
            dry: false,
        }))
    }

    /// The strongest part, the one a wrecked ship leaves behind.
    pub fn best_part(&self) -> Option<usize> {
        self.parts
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.rating().total_cmp(&b.1.rating()))
            .map(|(i, _)| i)
    }

    fn effects(&self) -> impl Iterator<Item = &Effect> {
        self.parts
            .iter()
            .flat_map(|p| &p.effects)
            .chain(self.arsenal.boost_effects())
    }

    /// The ship as it is flying now: parts, the active profile and the running boosts.
    pub fn stats(&self) -> Stats {
        self.apply_grade(Stats::compute(
            self.effects().copied().chain(self.arsenal.active_effect()),
        ))
    }

    fn apply_grade(&self, mut stats: Stats) -> Stats {
        let grade = if self.equipment_grade.is_finite() {
            self.equipment_grade.max(1.0)
        } else {
            1.0
        };
        stats.damage *= grade;
        stats.max_hull *= grade;
        stats.max_shield *= grade;
        stats.recharge *= grade;
        stats
    }

    /// Ship power for the HUD verdict: the stats' rating with the whole arsenal counted
    /// rather than only the profile that happens to be active.
    pub fn power(&self) -> f32 {
        self.stats()
            .power_with_volley(1.0 + self.arsenal.volley_bonus())
    }
}

/// The ship as the simulation sees it: base values with every effect folded in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub thrust: f32,
    pub top_speed: f32,
    /// Keyboard turn rate in radians per second.
    pub turn: f32,
    pub fire_period: f32,
    pub damage: f32,
    pub shot_speed: f32,
    pub shot_life: f32,
    pub max_hull: f32,
    pub max_shield: f32,
    /// Shield regained per second once the ship has been quiet.
    pub recharge: f32,
    /// Multiplier on damage taken; below one is armor.
    pub guard: f32,
    /// Distance at which pickups are drawn in.
    pub magnet: f32,
    pub spread: u8,
    pub pierce: u8,
    pub homing: u8,
    pub broadside: u8,
    pub tailgun: u8,
    pub blast: u8,
    pub ram: u8,
    pub siphon: u8,
    pub missiles: u8,
    pub mines: u8,
    pub needles: u8,
    pub nova: u8,
    pub shears: bool,
    pub ballast: bool,
    pub aura: u8,
}

impl Stats {
    pub const BASE: Self = Self {
        thrust: 430.0,
        top_speed: super::PLAYER_SPEED,
        turn: 3.8,
        fire_period: 0.16,
        damage: 26.0,
        shot_speed: 720.0,
        shot_life: 1.7,
        max_hull: 100.0,
        max_shield: 60.0,
        recharge: 6.0,
        guard: 1.0,
        magnet: 140.0,
        spread: 0,
        pierce: 0,
        homing: 0,
        broadside: 0,
        tailgun: 0,
        blast: 0,
        ram: 0,
        siphon: 0,
        missiles: 0,
        mines: 0,
        needles: 0,
        nova: 0,
        shears: false,
        ballast: false,
        aura: 0,
    };

    pub fn compute(effects: impl IntoIterator<Item = Effect>) -> Self {
        let mut bonus = [0.0_f32; Stat::ALL.len()];
        let mut levels = [0_u8; Trait::ALL.len()];
        for effect in effects {
            match effect {
                Effect::Stat(stat, amount) => bonus[stat.index()] += amount,
                Effect::Trait(kind, level) => {
                    let slot = &mut levels[kind.index()];
                    *slot = slot.saturating_add(level).min(kind.cap());
                }
            }
        }
        // Each stat scales its base by (1 + bonus), kept inside a sane band so no stack of
        // parts can break the simulation (or make the ship unplayably sluggish).
        let scale = |stat: Stat, ceiling: f32| (1.0 + bonus[stat.index()]).clamp(0.4, ceiling);
        let level = |kind: Trait| levels[kind.index()];
        let base = Self::BASE;
        Self {
            thrust: base.thrust * scale(Stat::Thrust, 3.0),
            top_speed: base.top_speed * scale(Stat::TopSpeed, 1.8),
            turn: base.turn * scale(Stat::Handling, 2.0),
            fire_period: (base.fire_period / scale(Stat::FireRate, 4.0)).max(0.04),
            damage: base.damage * scale(Stat::Damage, 8.0),
            shot_speed: base.shot_speed * scale(Stat::ShotSpeed, 2.0),
            shot_life: base.shot_life * scale(Stat::Range, 2.5),
            max_hull: base.max_hull * scale(Stat::Hull, 8.0),
            max_shield: base.max_shield * scale(Stat::Shield, 8.0),
            recharge: base.recharge * scale(Stat::Recharge, 6.0),
            guard: 1.0 / (1.0 + bonus[Stat::Armor.index()].clamp(-0.5, 4.0)),
            magnet: base.magnet * scale(Stat::Magnet, 6.0),
            spread: level(Trait::Spread),
            pierce: level(Trait::Pierce),
            homing: level(Trait::Homing),
            broadside: level(Trait::Broadside),
            tailgun: level(Trait::Tailgun),
            blast: level(Trait::Blast),
            ram: level(Trait::Ram),
            siphon: level(Trait::Siphon),
            missiles: level(Trait::Missiles),
            mines: level(Trait::Mines),
            needles: level(Trait::Needles),
            nova: level(Trait::Nova),
            shears: level(Trait::Shears) > 0,
            ballast: level(Trait::Ballast) > 0,
            aura: level(Trait::Aura),
        }
    }

    /// The level of a trait on this ship.
    pub fn level(&self, kind: Trait) -> u8 {
        match kind {
            Trait::Spread => self.spread,
            Trait::Pierce => self.pierce,
            Trait::Homing => self.homing,
            Trait::Broadside => self.broadside,
            Trait::Tailgun => self.tailgun,
            Trait::Blast => self.blast,
            Trait::Ram => self.ram,
            Trait::Shears => u8::from(self.shears),
            Trait::Ballast => u8::from(self.ballast),
            Trait::Siphon => self.siphon,
            Trait::Aura => self.aura,
            Trait::Missiles => self.missiles,
            Trait::Mines => self.mines,
            Trait::Needles => self.needles,
            Trait::Nova => self.nova,
        }
    }

    /// Volley rating of the traits this ship is firing with (1 for the stock gun).
    pub fn volley(&self) -> f32 {
        1.0 + Trait::ALL
            .iter()
            .map(|&t| t.volley_weight() * f32::from(self.level(t)))
            .sum::<f32>()
    }

    /// Raw gun output of the stock fire: damage times rate, 1 for the bare ship. Parts and
    /// pickups are never allowed to lower it.
    pub fn firepower(&self) -> f32 {
        let base = Self::BASE;
        self.damage / base.damage * base.fire_period / self.fire_period
    }

    /// One number for how much ship this is; the bare starting ship rates 1.
    pub fn power(&self) -> f32 {
        self.power_with_volley(self.volley())
    }

    /// Power with the volley rating given (so an arsenal can be counted as a whole). It is
    /// the geometric mean of firepower and staying power, nudged by agility, so it can be
    /// set against a sector's threat.
    pub fn power_with_volley(&self, volley: f32) -> f32 {
        let base = Self::BASE;
        let offense = self.firepower() * volley;
        let staying = (self.max_hull + 0.8 * self.max_shield)
            / (base.max_hull + 0.8 * base.max_shield)
            / self.guard
            * (1.0 + 0.04 * f32::from(self.siphon));
        let agility = (self.thrust / base.thrust * self.top_speed / base.top_speed * self.turn
            / base.turn)
            .cbrt();
        (offense * staying).sqrt() * agility.powf(0.3)
    }
}

/// Where a drop comes from: how deep, how lucky, what the source is like, and the
/// conditions around it. Everything `roll` needs.
#[derive(Clone, Copy, Debug)]
pub struct Source {
    /// The threat of the place; deeper sources roll stronger items.
    pub grade: f32,
    /// Extra luck: 0 for ordinary kills, more for bases and guardians.
    pub bias: f32,
    /// Leaning toward each slot's items, from what the creature is.
    pub affinity: [f32; Slot::ALL.len()],
    /// What the source shoots: gear that echoes it is likelier.
    pub weapon: Weapon,
    pub params: SectorParams,
    /// Items rolled from this source are at least this rare (bosses).
    pub min_rarity: Rarity,
}

impl Source {
    pub fn plain(grade: f32, params: SectorParams) -> Self {
        Self {
            grade: grade.max(1.0),
            bias: 0.0,
            affinity: [1.0; Slot::ALL.len()],
            weapon: Weapon::None,
            params,
            min_rarity: Rarity::Common,
        }
    }

    /// Creatures drop according to what they are: gunners shed weapons, heavies plating,
    /// fast things engines, shielded ones cores, and odd ones (cord throwers, flingers,
    /// negative mass) auxiliaries.
    pub fn of_creature(genome: &Genome, grade: f32, params: SectorParams) -> Self {
        let mut source = Self::plain(grade, params);
        source.weapon = genome.weapon;
        let a = &mut source.affinity;
        if genome.weapon == Weapon::Projectile {
            a[Slot::Cannon.index()] += 1.5;
        }
        a[Slot::Engine.index()] += (genome.speed / 200.0).min(2.0) + genome.lead;
        a[Slot::Plating.index()] +=
            (genome.hull / 100.0).min(2.5) + (genome.contact_damage / 24.0).min(1.0);
        a[Slot::Core.index()] += genome.shield / 30.0;
        if genome.diet == Diet::Siphon {
            a[Slot::Core.index()] += 1.5;
        }
        if genome.weapon == Weapon::Tether {
            a[Slot::Aux.index()] += 1.5;
        }
        if genome.fling_strength() > 0.4 {
            a[Slot::Aux.index()] += 1.0;
        }
        if genome.mass < 0.0 {
            a[Slot::Aux.index()] += 1.0;
            a[Slot::Engine.index()] += 0.5;
        }
        source
    }

    fn pull(&self, answers: Answers) -> f32 {
        let above = |p: f32| (p - 0.5).max(0.0) * 2.0;
        let p = &self.params;
        match answers {
            Answers::Anything => 1.0,
            Answers::Distortion => 1.0 + 4.0 * above(p.distortion),
            Answers::Tech => 1.0 + 4.0 * above(p.tech),
            Answers::Swarm => 1.0 + 3.0 * above(p.swarm),
            Answers::Aggression => 1.0 + 3.0 * above(p.aggression),
            Answers::Danger => 1.0 + 3.0 * p.danger,
        }
    }
}

fn pick(rng: &mut Rng, weights: &[f32]) -> usize {
    let total: f32 = weights.iter().sum();
    let mut roll = rng.f32() * total;
    for (i, w) in weights.iter().enumerate() {
        roll -= w;
        if roll < 0.0 {
            return i;
        }
    }
    weights.len() - 1
}

fn roll_rarity(rng: &mut Rng, source: &Source) -> Rarity {
    let depth = (source.grade - 1.0).max(0.0);
    let luck = 1.0 + 2.5 * source.bias;
    let weights = [
        60.0,
        28.0 * (1.0 + 0.15 * depth) * luck,
        10.0 * (1.0 + 0.35 * depth) * luck * luck,
        2.0 * (1.0 + 0.6 * depth) * luck * luck * luck,
    ];
    Rarity::ALL[pick(rng, &weights)].max(source.min_rarity)
}

/// Magnitude of a blueprint's effects for this rarity and grade. Deeper sources roll
/// stronger items; levels of a trait also climb with rarity and depth.
fn scaled(effects: &[Effect], rarity: Rarity, grade: f32) -> Vec<Effect> {
    let power = rarity.strength() * (1.0 + 0.35 * (grade - 1.0).max(0.0));
    let extra = u8::from(rarity >= Rarity::Rare) + u8::from(grade >= 4.0);
    effects
        .iter()
        .map(|&effect| match effect {
            Effect::Stat(stat, amount) => Effect::Stat(stat, amount * power),
            Effect::Trait(kind, level) => Effect::Trait(kind, (level + extra).min(kind.cap())),
        })
        .collect()
}

/// Stats a slot's gear naturally comes with as bonus affixes.
fn affix_pool(slot: Slot) -> &'static [Stat] {
    match slot {
        Slot::Cannon => &[Stat::FireRate, Stat::Damage, Stat::ShotSpeed, Stat::Range],
        Slot::Engine => &[Stat::Thrust, Stat::TopSpeed, Stat::Handling],
        Slot::Plating => &[Stat::Hull, Stat::Armor],
        Slot::Core => &[Stat::Shield, Stat::Recharge, Stat::Armor],
        Slot::Aux => &[Stat::Magnet, Stat::Handling, Stat::Thrust, Stat::Recharge],
    }
}

/// Gear that mirrors a weapon: what shoots missiles sheds missile pods, and so on.
fn echoes(weapon: Weapon, effects: &[Effect]) -> bool {
    let wanted: &[Trait] = match weapon {
        Weapon::Projectile => &[Trait::Spread],
        Weapon::Missile => &[Trait::Missiles, Trait::Homing],
        Weapon::Mine => &[Trait::Mines],
        Weapon::Needles => &[Trait::Needles, Trait::Pierce],
        Weapon::Nova => &[Trait::Nova, Trait::Spread],
        Weapon::Spiral => &[Trait::Broadside, Trait::Nova],
        Weapon::Tether => &[Trait::Shears],
        Weapon::None => &[],
    };
    effects
        .iter()
        .any(|e| matches!(e, Effect::Trait(t, _) if wanted.contains(t)))
}

fn choose<'a>(rng: &mut Rng, table: &'a [Blueprint], source: &Source) -> &'a Blueprint {
    let weights: Vec<f32> = table
        .iter()
        .map(|b| {
            let echo = if echoes(source.weapon, b.effects) {
                3.0
            } else {
                1.0
            };
            source.affinity[b.slot.index()] * source.pull(b.answers) * echo
        })
        .collect();
    &table[pick(rng, &weights)]
}

/// Rolls up to `count` bonus affixes onto `effects` (never repeating a stat already there)
/// and returns the adjectives of the ones that landed.
fn add_affixes(
    rng: &mut Rng,
    slot: Slot,
    effects: &mut Vec<Effect>,
    count: usize,
    scale: f32,
) -> Vec<&'static str> {
    let mut adjectives = Vec::new();
    for _ in 0..count {
        let pool = affix_pool(slot);
        // Mostly slot-themed, now and then anything at all.
        let stat = if rng.chance(0.25) {
            Stat::ALL[rng.int(0, Stat::ALL.len() as u32 - 1) as usize]
        } else {
            pool[rng.int(0, pool.len() as u32 - 1) as usize]
        };
        if effects
            .iter()
            .any(|e| matches!(e, Effect::Stat(s, _) if *s == stat))
        {
            continue;
        }
        effects.push(Effect::Stat(
            stat,
            stat.affix_bonus() * scale * rng.range(0.7, 1.2),
        ));
        adjectives.push(stat.adjective());
    }
    adjectives
}

/// "Stout Warded Hull Plating Mk3": up to two adjectives, the stem, a mark from the grade.
fn part_name(stem: &str, adjectives: &[&str], grade: f32) -> String {
    let mut name = adjectives
        .iter()
        .take(2)
        .map(|a| format!("{a} "))
        .collect::<String>();
    name.push_str(stem);
    if grade >= 2.0 {
        name.push_str(&format!(" Mk{}", grade.floor() as u32));
    }
    name
}

pub fn roll_part(rng: &mut Rng, source: &Source) -> Part {
    let blueprint = choose(rng, PARTS, source);
    let rarity = roll_rarity(rng, source);
    let mut effects = scaled(blueprint.effects, rarity, source.grade);
    let core = effects.len();
    let scale = rarity.strength() * (1.0 + 0.35 * (source.grade - 1.0).max(0.0));
    let adjectives = add_affixes(rng, blueprint.slot, &mut effects, rarity.affixes(), scale);
    Part {
        name: part_name(blueprint.name, &adjectives, source.grade),
        slot: blueprint.slot,
        rarity,
        grade: source.grade,
        core,
        effects,
        stem: blueprint.name.to_string(),
    }
}

impl Part {
    /// Strength multiplier of the part's rarity at its grade (what rolled affixes scale by).
    fn scale(&self) -> f32 {
        self.rarity.strength() * (1.0 + 0.35 * (self.grade - 1.0).max(0.0))
    }

    /// Adjectives of the rolled affixes, for the name.
    fn adjectives(&self) -> Vec<&'static str> {
        let core = self.core.min(self.effects.len());
        self.effects[core..]
            .iter()
            .filter_map(|e| match e {
                Effect::Stat(stat, _) => Some(stat.adjective()),
                Effect::Trait(..) => None,
            })
            .collect()
    }

    fn rename(&mut self) {
        if !self.stem.is_empty() {
            self.name = part_name(&self.stem, &self.adjectives(), self.grade);
        }
    }

    /// The next rarity step, or None at the cap.
    pub fn next_rarity(&self) -> Option<Rarity> {
        Rarity::ALL.get(self.rarity as usize + 1).copied()
    }

    /// Rerolls the rolled affixes: three fresh sets are drawn and the best of them and the
    /// current set is kept, so a reforge never makes a part worse. True if it changed.
    pub fn reforge(&mut self, rng: &mut Rng) -> bool {
        let core = self.core.min(self.effects.len());
        let scale = self.scale();
        let mut best: Option<Vec<Effect>> = None;
        let mut best_rating = self.rating();
        for _ in 0..3 {
            let mut candidate = self.effects[..core].to_vec();
            add_affixes(rng, self.slot, &mut candidate, self.rarity.affixes(), scale);
            let rating: f32 = candidate.iter().map(Effect::rating).sum();
            if rating > best_rating + 1e-4 {
                best_rating = rating;
                best = Some(candidate);
            }
        }
        match best {
            Some(effects) => {
                self.effects = effects;
                self.rename();
                true
            }
            None => false,
        }
    }

    /// Guaranteed positive-stat multiplier for the next rarity, before its random affix.
    pub fn next_rarity_scale(&self) -> Option<f32> {
        self.next_rarity()
            .map(|next| next.strength() / self.rarity.strength())
    }

    /// Raises the part one rarity step: gains of the new rarity scale every beneficial stat
    /// (penalties stay as they are), a Rare part gains a trait level, and one more affix is
    /// rolled. Nothing gets worse. False at the cap.
    pub fn upgrade(&mut self, rng: &mut Rng) -> bool {
        let Some(next) = self.next_rarity() else {
            return false;
        };
        let ratio = next.strength() / self.rarity.strength();
        let level_up = next >= Rarity::Rare && self.rarity < Rarity::Rare;
        for effect in &mut self.effects {
            *effect = match *effect {
                Effect::Stat(stat, amount) if amount > 0.0 => Effect::Stat(stat, amount * ratio),
                Effect::Trait(kind, level) if level_up => {
                    Effect::Trait(kind, (level + 1).min(kind.cap()))
                }
                other => other,
            };
        }
        self.core = self.core.min(self.effects.len());
        self.rarity = next;
        let scale = self.scale();
        add_affixes(rng, self.slot, &mut self.effects, 1, scale);
        self.rename();
        true
    }
}

pub fn roll_surge(rng: &mut Rng, source: &Source, tune: &Tunables) -> Surge {
    let blueprint = choose(rng, SURGES, source);
    let rarity = roll_rarity(rng, source);
    let richer = 1.0 + 0.25 * rarity as usize as f32;
    let effects = scaled(blueprint.effects, rarity, 1.0 + (source.grade - 1.0) * 0.5);
    let (material, drain, need, fuel) = match blueprint.boost {
        Some((material, drain, need)) => (material, drain, need, drain * tune.boost_fuel_seconds),
        None => {
            // A weapon charge is fuel for the profile it unlocks.
            let material = effects
                .iter()
                .find_map(|e| match *e {
                    Effect::Trait(kind, _) => Profile::from_trait(kind)?.material(),
                    Effect::Stat(..) => None,
                })
                .unwrap_or(Material::Metal);
            (material, 0.0, Need::Firing, tune.boost_charge_fuel)
        }
    };
    Surge {
        name: blueprint.name.to_string(),
        slot: blueprint.slot,
        rarity,
        effects,
        material,
        fuel: fuel * richer,
        drain,
        need,
    }
}

/// An ordinary kill's drop: mostly restoratives and surges, now and then a permanent part.
pub fn roll_item(rng: &mut Rng, source: &Source, tune: &Tunables) -> Item {
    let lucky = 1.0 + 2.0 * source.bias;
    let weights = [
        20.0,                // hull repair
        16.0,                // shield charge
        28.0 * lucky,        // surge
        22.0 * lucky,        // part
        8.0,                 // salvage
        0.4 * lucky * lucky, // extra life
    ];
    match pick(rng, &weights) {
        0 => Item::Repair(15.0 + 25.0 * source.grade.sqrt() * rng.range(0.8, 1.2)),
        1 => Item::Recharge(20.0 + 20.0 * source.grade.sqrt() * rng.range(0.8, 1.2)),
        2 => Item::Surge(roll_surge(rng, source, tune)),
        3 => Item::Part(roll_part(rng, source)),
        4 => {
            // The same single draw picks the amount and, through its low digits, the kind of
            // material, so existing loot streams keep their shape.
            let roll = rng.range(0.8, 1.4);
            let amount = ((15.0 * source.grade * roll) as u32 / 5 * 5).max(5) as f32;
            let kind = match (roll * 97.0).fract() {
                f if f < 0.5 => Material::Metal,
                f if f < 0.75 => Material::Volatiles,
                _ => Material::Crystal,
            };
            Item::Material(kind, amount)
        }
        _ => Item::Life,
    }
}

/// What a drifting rock sometimes gives up.
pub fn roll_salvage(rng: &mut Rng, source: &Source) -> Item {
    match pick(rng, &[1.0, 1.0, 1.0]) {
        0 => Item::Repair(10.0 + 8.0 * source.grade),
        1 => Item::Recharge(14.0 + 8.0 * source.grade),
        _ => Item::Material(
            Material::Metal,
            ((10.0 * source.grade) as u32 / 5 * 5).max(10) as f32,
        ),
    }
}

/// A plain surge for tests: one effect, 100 fuel, a drain of 1 a second while firing.
#[cfg(test)]
pub(crate) fn test_surge(effect: Effect) -> Surge {
    Surge {
        name: "Test Surge".into(),
        slot: Slot::Cannon,
        rarity: Rarity::Common,
        effects: vec![effect],
        material: Material::Fuel,
        fuel: 100.0,
        drain: 1.0,
        need: Need::Firing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(grade: f32) -> Source {
        Source::plain(grade, SectorParams::HOME)
    }

    #[test]
    fn the_bare_ship_has_the_base_stats_and_unit_power() {
        let stats = Loadout::default().stats();
        assert_eq!(stats, Stats::BASE);
        assert!((stats.power() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn effects_compose_and_clamp() {
        let stats = Stats::compute([
            Effect::Stat(Stat::Damage, 0.4),
            Effect::Stat(Stat::Damage, 0.6),
            Effect::Stat(Stat::FireRate, 1.0),
            Effect::Trait(Trait::Spread, 2),
            Effect::Trait(Trait::Spread, 2),
            Effect::Stat(Stat::Armor, 1.0),
        ]);
        assert!((stats.damage - 52.0).abs() < 1e-3);
        assert!((stats.fire_period - 0.08).abs() < 1e-4);
        assert_eq!(stats.spread, Trait::Spread.cap());
        assert!((stats.guard - 0.5).abs() < 1e-4);
        // Absurd stacks stay inside their band.
        let wild = Stats::compute((0..50).map(|_| Effect::Stat(Stat::TopSpeed, 5.0)));
        assert!(wild.top_speed <= Stats::BASE.top_speed * 1.8 + 1e-3);
        let crippled = Stats::compute((0..50).map(|_| Effect::Stat(Stat::Thrust, -5.0)));
        assert!(crippled.thrust >= Stats::BASE.thrust * 0.4 - 1e-3);
    }

    #[test]
    fn slots_fill_then_displace_the_weakest_and_scrap_the_rest() {
        let part = |name: &str, bonus: f32| Part {
            name: name.into(),
            slot: Slot::Plating,
            rarity: Rarity::Common,
            grade: 1.0,
            stem: String::new(),
            core: usize::MAX,
            effects: vec![Effect::Stat(Stat::Hull, bonus)],
        };
        let mut loadout = Loadout::default();
        assert_eq!(loadout.install(part("a", 0.2)), Install::Added);
        assert_eq!(loadout.install(part("b", 0.4)), Install::Added);
        assert!(matches!(loadout.install(part("c", 0.6)), Install::Replaced(p) if p.name == "a"));
        assert!(matches!(loadout.install(part("d", 0.1)), Install::Scrapped(p) if p.name == "d"));
        assert_eq!(loadout.parts.len(), 2);
        assert_eq!(
            loadout.best_part().map(|i| loadout.parts[i].name.as_str()),
            Some("c")
        );
    }

    #[test]
    fn rolls_are_deterministic_and_deeper_sources_roll_stronger_parts() {
        let rolled = |seed, grade| roll_part(&mut Rng::new(seed), &source(grade));
        assert_eq!(rolled(5, 2.0), rolled(5, 2.0));
        let mean = |grade: f32| (0..400).map(|s| rolled(s, grade).rating()).sum::<f32>() / 400.0;
        assert!(mean(1.0) < mean(3.0) && mean(3.0) < mean(6.0));
        // Rarity climbs with depth and with luck.
        let epic = |grade: f32, bias: f32| {
            let mut rng = Rng::new(9);
            let mut src = source(grade);
            src.bias = bias;
            (0..2000)
                .filter(|_| roll_rarity(&mut rng, &src) == Rarity::Epic)
                .count()
        };
        assert!(epic(1.0, 0.0) < epic(5.0, 0.0) && epic(1.0, 0.0) < epic(1.0, 1.0));
    }

    #[test]
    fn counters_drop_where_the_problem_is() {
        let count = |params: SectorParams, wanted: Trait| {
            let mut rng = Rng::new(3);
            let src = Source::plain(1.0, params);
            (0..3000)
                .filter(|_| {
                    roll_part(&mut rng, &src)
                        .effects
                        .iter()
                        .any(|e| matches!(e, Effect::Trait(t, _) if *t == wanted))
                })
                .count()
        };
        let calm = SectorParams::HOME;
        let warped = SectorParams {
            distortion: 1.0,
            ..calm
        };
        let wired = SectorParams { tech: 1.0, ..calm };
        assert!(count(warped, Trait::Ballast) > 2 * count(calm, Trait::Ballast));
        assert!(count(wired, Trait::Shears) > 2 * count(calm, Trait::Shears));
    }

    #[test]
    fn creatures_drop_what_they_are() {
        let params = SectorParams::HOME;
        let gunner = Source::of_creature(&Genome::bogey(), 1.0, params);
        let heavy = Source::of_creature(&Genome::fatso(), 1.0, params);
        let leech = Source::of_creature(&Genome::leech(), 1.0, params);
        assert!(gunner.affinity[Slot::Cannon.index()] > heavy.affinity[Slot::Cannon.index()]);
        assert!(heavy.affinity[Slot::Plating.index()] > gunner.affinity[Slot::Plating.index()]);
        assert!(leech.affinity[Slot::Aux.index()] > gunner.affinity[Slot::Aux.index()]);
    }

    #[test]
    fn a_ship_built_from_deep_drops_outclasses_one_built_from_shallow_drops() {
        // Best-in-slot out of a few hundred rolls, at two depths.
        let build = |grade: f32| {
            let mut rng = Rng::new(77);
            let mut loadout = Loadout::default();
            for _ in 0..300 {
                loadout.acquire(roll_part(&mut rng, &source(grade)));
            }
            loadout.power()
        };
        let (shallow, deep) = (build(1.0), build(5.0));
        assert!(shallow > 1.5, "{shallow}");
        assert!(deep > shallow * 1.8, "{shallow} vs {deep}");
    }

    #[test]
    fn every_blueprint_is_usable() {
        for blueprint in PARTS.iter().chain(SURGES) {
            assert!(!blueprint.effects.is_empty() && !blueprint.name.is_empty());
            assert!(
                !Stats::compute(blueprint.effects.iter().copied())
                    .power()
                    .is_nan()
            );
        }
        assert!(PARTS.iter().all(|b| b.boost.is_none()));
        // Every surge is either a weapon charge (it names a profile) or a boost with a
        // positive drain, never both and never neither.
        for b in SURGES {
            let weapon = b
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Trait(t, _) if Profile::from_trait(*t).is_some()));
            assert_eq!(weapon, b.boost.is_none(), "{}", b.name);
            if let Some((_, drain, _)) = b.boost {
                assert!(drain > 0.0);
            }
        }
        for slot in Slot::ALL {
            assert!(PARTS.iter().any(|b| b.slot == slot));
        }
    }
}
