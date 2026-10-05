//! Ship augmentation as a composable system. Nothing here is a fixed "upgrade": a ship's
//! abilities are the sum of the `Effect`s carried by the parts bolted to it (permanent,
//! limited by mounting slots) and the surges running on it (temporary). An effect either
//! scales a `Stat` or grants levels of a `Trait`; parts and surges are assembled from a
//! blueprint, a rarity, a grade (how deep the source was) and rolled affixes, so the same
//! few ingredients yield a wide range of items. `Stats::compute` folds them into what the
//! simulation reads. Pure data and deterministic rolls; no game state lives here.

use crate::genome::{Diet, Genome, Weapon};
use crate::world::{QuadrantParams, Rng};

/// Scalable ship statistics. An effect's amount is a fraction of the base value
/// (+0.25 is 25 percent more).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
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

    fn rating(&self) -> f32 {
        match *self {
            Self::Stat(stat, amount) => amount * stat.worth(),
            Self::Trait(kind, level) => f32::from(level) * kind.worth(),
        }
    }
}

/// Where a permanent part is bolted on. Each slot holds a few parts; a better part
/// displaces the weakest one when its slot is full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
/// quadrant parameter runs high.
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
    /// Seconds a surge runs; zero for permanent parts.
    seconds: f32,
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
        seconds: 0.0,
    }
}

const fn surge(
    name: &'static str,
    slot: Slot,
    seconds: f32,
    answers: Answers,
    effects: &'static [Effect],
) -> Blueprint {
    Blueprint {
        name,
        slot,
        effects,
        answers,
        seconds,
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
    surge(
        "Missile Barrage",
        Cannon,
        25.0,
        Answers::Danger,
        &[T(Trait::Missiles, 2)],
    ),
    surge(
        "Needle Storm",
        Cannon,
        22.0,
        Answers::Tech,
        &[T(Trait::Needles, 2)],
    ),
    surge(
        "Nova Pulse",
        Core,
        20.0,
        Answers::Swarm,
        &[T(Trait::Nova, 1)],
    ),
    surge(
        "Minefield",
        Aux,
        30.0,
        Answers::Aggression,
        &[T(Trait::Mines, 2)],
    ),
    surge(
        "Overdrive",
        Cannon,
        20.0,
        Answers::Swarm,
        &[S(Stat::FireRate, 0.7)],
    ),
    surge(
        "Amplifier",
        Cannon,
        20.0,
        Answers::Danger,
        &[S(Stat::Damage, 0.9)],
    ),
    surge(
        "Scatter Burst",
        Cannon,
        25.0,
        Answers::Swarm,
        &[T(Trait::Spread, 2)],
    ),
    surge(
        "Needle Rounds",
        Cannon,
        25.0,
        Answers::Danger,
        &[T(Trait::Pierce, 2), S(Stat::ShotSpeed, 0.3)],
    ),
    surge(
        "Hunter Swarm",
        Cannon,
        25.0,
        Answers::Swarm,
        &[T(Trait::Homing, 2)],
    ),
    surge(
        "Afterglow",
        Engine,
        18.0,
        Answers::Aggression,
        &[S(Stat::TopSpeed, 0.45), S(Stat::Thrust, 0.6)],
    ),
    surge(
        "Aegis Field",
        Plating,
        15.0,
        Answers::Aggression,
        &[S(Stat::Armor, 1.0)],
    ),
    surge(
        "Overshield",
        Core,
        25.0,
        Answers::Anything,
        &[S(Stat::Shield, 1.5), S(Stat::Recharge, 1.5)],
    ),
    surge(
        "Magnet Storm",
        Aux,
        30.0,
        Answers::Anything,
        &[S(Stat::Magnet, 3.0)],
    ),
    surge(
        "Lunatic Field",
        Aux,
        15.0,
        Answers::Aggression,
        &[T(Trait::Aura, 2), S(Stat::Armor, 0.3)],
    ),
    surge(
        "Gravity Boots",
        Aux,
        40.0,
        Answers::Distortion,
        &[T(Trait::Ballast, 1)],
    ),
    surge(
        "Shear Pulse",
        Aux,
        40.0,
        Answers::Tech,
        &[T(Trait::Shears, 1)],
    ),
];

/// A permanent part, bolted to one slot of the ship.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub slot: Slot,
    pub rarity: Rarity,
    /// The threat of the place it came from; deeper parts are stronger.
    pub grade: f32,
    pub effects: Vec<Effect>,
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
}

/// A temporary boost that runs down.
#[derive(Clone, Debug, PartialEq)]
pub struct Surge {
    pub name: String,
    pub slot: Slot,
    pub rarity: Rarity,
    pub effects: Vec<Effect>,
    pub duration: f32,
    pub remaining: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Hull points restored at once.
    Repair(f32),
    /// Shield points restored at once.
    Recharge(f32),
    Life,
    Scrap(u32),
    Part(Part),
    Surge(Surge),
}

impl Item {
    pub fn rarity(&self) -> Rarity {
        match self {
            Self::Part(p) => p.rarity,
            Self::Surge(s) => s.rarity,
            Self::Life => Rarity::Epic,
            _ => Rarity::Common,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Repair(_) => "Hull repair".into(),
            Self::Recharge(_) => "Shield charge".into(),
            Self::Life => "Extra life".into(),
            Self::Scrap(points) => format!("Salvage +{points}"),
            Self::Part(p) => p.name.clone(),
            Self::Surge(s) => s.name.clone(),
        }
    }
}

/// What happened when a part was offered to the ship.
#[derive(Debug, PartialEq)]
pub enum Install {
    Added,
    Replaced(Part),
    /// The slot was full of better parts; the newcomer was broken down for scrap.
    Scrapped(Part),
}

/// Everything attached to the ship.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loadout {
    pub parts: Vec<Part>,
    pub surges: Vec<Surge>,
}

/// Surges that can run at once.
const MAX_SURGES: usize = 5;

impl Loadout {
    pub fn in_slot(&self, slot: Slot) -> impl Iterator<Item = &Part> {
        self.parts.iter().filter(move |p| p.slot == slot)
    }

    pub fn install(&mut self, part: Part) -> Install {
        let held = self.in_slot(part.slot).count();
        if held < part.slot.capacity() {
            self.parts.push(part);
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
                Install::Replaced(std::mem::replace(&mut self.parts[i], part))
            }
            _ => Install::Scrapped(part),
        }
    }

    /// Starts a surge, or tops up a running one of the same kind.
    pub fn start(&mut self, surge: Surge) {
        if let Some(running) = self.surges.iter_mut().find(|s| s.name == surge.name) {
            running.remaining = running.remaining.max(surge.duration);
            running.duration = running.duration.max(surge.duration);
            return;
        }
        if self.surges.len() >= MAX_SURGES
            && let Some(oldest) = self
                .surges
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.remaining.total_cmp(&b.1.remaining))
                .map(|(i, _)| i)
        {
            self.surges.remove(oldest);
        }
        self.surges.push(surge);
    }

    /// Runs surges down; true when one has just ended.
    pub fn tick(&mut self, dt: f32) -> bool {
        let before = self.surges.len();
        for surge in &mut self.surges {
            surge.remaining -= dt;
        }
        self.surges.retain(|s| s.remaining > 0.0);
        self.surges.len() != before
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
            .chain(self.surges.iter().flat_map(|s| &s.effects))
    }

    pub fn stats(&self) -> Stats {
        Stats::compute(self.effects().copied())
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

    /// One number for how much ship this is; the bare starting ship rates 1. It is the
    /// geometric mean of firepower and staying power, nudged by agility, so it can be set
    /// against a quadrant's threat.
    pub fn power(&self) -> f32 {
        let base = Self::BASE;
        let volley = 1.0
            + 0.5 * f32::from(self.spread)
            + 0.4 * f32::from(self.pierce)
            + 0.3 * f32::from(self.homing)
            + 0.5 * f32::from(self.broadside)
            + 0.25 * f32::from(self.tailgun)
            + 0.3 * f32::from(self.blast)
            + 0.4 * f32::from(self.missiles)
            + 0.25 * f32::from(self.mines)
            + 0.5 * f32::from(self.needles)
            + 0.35 * f32::from(self.nova);
        let offense = self.damage / base.damage * base.fire_period / self.fire_period * volley;
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
    pub params: QuadrantParams,
}

impl Source {
    pub fn plain(grade: f32, params: QuadrantParams) -> Self {
        Self {
            grade: grade.max(1.0),
            bias: 0.0,
            affinity: [1.0; Slot::ALL.len()],
            weapon: Weapon::None,
            params,
        }
    }

    /// Creatures drop according to what they are: gunners shed weapons, heavies plating,
    /// fast things engines, shielded ones cores, and odd ones (cord throwers, flingers,
    /// negative mass) auxiliaries.
    pub fn of_creature(genome: &Genome, grade: f32, params: QuadrantParams) -> Self {
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
    Rarity::ALL[pick(rng, &weights)]
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

pub fn roll_part(rng: &mut Rng, source: &Source) -> Part {
    let blueprint = choose(rng, PARTS, source);
    let rarity = roll_rarity(rng, source);
    let mut effects = scaled(blueprint.effects, rarity, source.grade);
    let scale = rarity.strength() * (1.0 + 0.35 * (source.grade - 1.0).max(0.0));
    let mut adjectives: Vec<&str> = Vec::new();
    for _ in 0..rarity.affixes() {
        let pool = affix_pool(blueprint.slot);
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
    let mut name = adjectives
        .iter()
        .take(2)
        .map(|a| format!("{a} "))
        .collect::<String>();
    name.push_str(blueprint.name);
    if source.grade >= 2.0 {
        name.push_str(&format!(" Mk{}", source.grade.floor() as u32));
    }
    Part {
        name,
        slot: blueprint.slot,
        rarity,
        grade: source.grade,
        effects,
    }
}

pub fn roll_surge(rng: &mut Rng, source: &Source) -> Surge {
    let blueprint = choose(rng, SURGES, source);
    let rarity = roll_rarity(rng, source);
    let duration = blueprint.seconds * (1.0 + 0.25 * rarity as usize as f32);
    Surge {
        name: blueprint.name.to_string(),
        slot: blueprint.slot,
        rarity,
        effects: scaled(blueprint.effects, rarity, 1.0 + (source.grade - 1.0) * 0.5),
        duration,
        remaining: duration,
    }
}

/// An ordinary kill's drop: mostly restoratives and surges, now and then a permanent part.
pub fn roll_item(rng: &mut Rng, source: &Source) -> Item {
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
        2 => Item::Surge(roll_surge(rng, source)),
        3 => Item::Part(roll_part(rng, source)),
        4 => Item::Scrap((40.0 * source.grade * rng.range(0.8, 1.4)) as u32 / 5 * 5),
        _ => Item::Life,
    }
}

/// What a drifting rock sometimes gives up.
pub fn roll_salvage(rng: &mut Rng, source: &Source) -> Item {
    match pick(rng, &[1.0, 1.0, 1.0]) {
        0 => Item::Repair(10.0 + 8.0 * source.grade),
        1 => Item::Recharge(14.0 + 8.0 * source.grade),
        _ => Item::Scrap(25),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(grade: f32) -> Source {
        Source::plain(grade, QuadrantParams::HOME)
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
    fn surges_expire_and_refresh_instead_of_stacking() {
        let surge = |seconds| Surge {
            name: "Overdrive".into(),
            slot: Slot::Cannon,
            rarity: Rarity::Common,
            effects: vec![Effect::Stat(Stat::FireRate, 0.7)],
            duration: seconds,
            remaining: seconds,
        };
        let mut loadout = Loadout::default();
        loadout.start(surge(10.0));
        assert!(!loadout.tick(4.0));
        loadout.start(surge(10.0));
        assert_eq!(loadout.surges.len(), 1);
        assert!((loadout.surges[0].remaining - 10.0).abs() < 1e-3);
        assert!(loadout.stats().fire_period < Stats::BASE.fire_period);
        assert!(loadout.tick(11.0));
        assert_eq!(loadout.stats(), Stats::BASE);
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
        let count = |params: QuadrantParams, wanted: Trait| {
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
        let calm = QuadrantParams::HOME;
        let warped = QuadrantParams {
            distortion: 1.0,
            ..calm
        };
        let wired = QuadrantParams { tech: 1.0, ..calm };
        assert!(count(warped, Trait::Ballast) > 2 * count(calm, Trait::Ballast));
        assert!(count(wired, Trait::Shears) > 2 * count(calm, Trait::Shears));
    }

    #[test]
    fn creatures_drop_what_they_are() {
        let params = QuadrantParams::HOME;
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
                loadout.install(roll_part(&mut rng, &source(grade)));
            }
            loadout.stats().power()
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
        assert!(PARTS.iter().all(|b| b.seconds == 0.0));
        assert!(SURGES.iter().all(|b| b.seconds > 0.0));
        for slot in Slot::ALL {
            assert!(PARTS.iter().any(|b| b.slot == slot));
        }
    }
}
