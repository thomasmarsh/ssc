//! Economy and skill tuning in one place: how tough rocks are, how fast the beam works,
//! what the rig upgrades cost and give. Change numbers here, not at the use sites.
//!
//! The intent for rocks: the mining beam is the way to turn rock into material, and shooting
//! one is a slow, wasteful way to lose it (see `mining` tests that pin the ratio).

use super::Material;

// ---- rocks ---------------------------------------------------------------------------------

/// How much tougher free rocks are against the ship's own weapons (its shots, blasts and
/// ram): damage to them is divided by this. Rocks still break from collisions and enemy fire
/// as before, so the ecology that feeds on shards is untouched. Planetoids and fortress walls
/// have their own rules.
pub const ROCK_HULL_FACTOR: f32 = 80.0;
/// Share of a shot rock's remaining ore that survives in its fragments (the rest is dust).
pub const SHOT_ORE_KEEP: f32 = 0.5;
/// Chance a shot rock surfaces something, per kind (the roll is drawn either way).
pub const SALVAGE_CHANCE: f32 = 0.02;
pub const ICE_CHANCE: f32 = 0.06;
pub const ORE_CHANCE: f32 = 0.05;
pub const CRYSTAL_CHANCE: f32 = 0.12;
/// Metal in an ore rock's scrap drop, per unit of grade (it used to be 45).
pub const ORE_SCRAP: f32 = 15.0;
pub const ICE_VOLATILES: f32 = 3.0;

// ---- the mining beam -----------------------------------------------------------------------

/// How much of each material the hold carries before cargo upgrades.
pub const CAP: f32 = 200.0;
/// Beam reach, from the ship's center to the rock's surface, before range upgrades.
pub const BEAM_RANGE: f32 = 260.0;
/// Shield the beam draws per second, and the shield below which it will not fire.
pub const BEAM_DRAIN: f32 = 4.0;
pub const SHIELD_FLOOR: f32 = 6.0;
/// A rock never shrinks below this radius: it crumbles there.
pub const CRUMBLE_RADIUS: f32 = 14.0;
/// Crystal is harvested in cycles of this many seconds, each worth `CYCLE_YIELD`; held past
/// `BURST_AFTER` seconds it bursts.
pub const CYCLE: f32 = 0.5;
pub const CYCLE_YIELD: f32 = 4.0;
pub const BURST_AFTER: f32 = 3.0 * CYCLE + 0.1;
/// Ore a planetoid will give before it is spent (it never shrinks).
pub const PLANETOID_BUDGET: f32 = 400.0;
/// Ore per second of beam, by kind of rock (crystal uses its cycles instead).
pub const RATE_ORE: f32 = 7.0;
pub const RATE_PLAIN: f32 = 4.5;
pub const RATE_ICE: f32 = 6.0;
pub const RATE_HUSK: f32 = 2.5;
pub const RATE_PLANETOID: f32 = 1.2;

// ---- rig upgrades --------------------------------------------------------------------------

/// Levels every mining upgrade can reach.
pub const SKILL_MAX: u8 = 4;
/// Per level: beam rate multiplier, extra reach, extra yield per ore, extra magnet radius and
/// extra hold space per material.
pub const POWER_STEP: f32 = 0.35;
pub const RANGE_STEP: f32 = 60.0;
pub const YIELD_STEP: f32 = 0.2;
pub const MAGNET_STEP: f32 = 70.0;
pub const CARGO_STEP: f32 = 50.0;
/// Each level costs this much more than the one before.
pub const PRICE_GROWTH: f32 = 1.8;
/// First-level prices: (material, amount).
pub const PRICE_POWER: [(Material, f32); 2] = [(Material::Metal, 30.0), (Material::Crystal, 4.0)];
pub const PRICE_RANGE: [(Material, f32); 2] = [(Material::Metal, 25.0), (Material::Crystal, 6.0)];
pub const PRICE_YIELD: [(Material, f32); 2] =
    [(Material::Volatiles, 20.0), (Material::Crystal, 8.0)];
pub const PRICE_MAGNET: [(Material, f32); 2] =
    [(Material::Volatiles, 20.0), (Material::Crystal, 4.0)];
pub const PRICE_CARGO: [(Material, f32); 2] =
    [(Material::Metal, 40.0), (Material::Volatiles, 10.0)];

// ---- parry (a locked upgrade: level 0 is not owned) ----------------------------------------

/// Chance a hostile shot inside the arc is stopped at level 1, and the gain per level after.
/// Some of a barrage always leaks through.
pub const PARRY_CHANCE: f32 = 0.70;
pub const PARRY_CHANCE_STEP: f32 = 0.06;
/// Seconds the shield stays up, and the first part of that which is a "perfect" window: a
/// shot stopped then is sent back harder and part of the cost is refunded.
pub const PARRY_WINDOW: f32 = 0.35;
pub const PARRY_PERFECT: f32 = 0.12;
/// Seconds from raising the shield until it can be raised again, less per level.
pub const PARRY_COOLDOWN: f32 = 1.6;
pub const PARRY_COOLDOWN_STEP: f32 = 0.15;
/// Shield energy a raise costs (it needs that much on hand), and what a perfect parry gives back.
pub const PARRY_COST: f32 = 15.0;
pub const PARRY_REFUND: f32 = 8.0;
/// The arc: half-angle either side of the nose (radians) and reach from the ship's center.
pub const PARRY_HALF_ARC: f32 = 1.0;
pub const PARRY_RADIUS: f32 = 90.0;
/// A reflected shot's damage as a multiple of what it carried, and the gain per level after
/// the first. The perfect window also widens a little per level.
pub const PARRY_REFLECT: f32 = 1.5;
pub const PARRY_REFLECT_STEP: f32 = 0.25;
pub const PARRY_PERFECT_STEP: f32 = 0.02;
/// What a perfect parry earns, once per raise however many shots it turns: seconds off the
/// cooldown, a freeze of the whole simulation (hit-stop), and how long the flash lasts. The
/// shield it gives back is capped per raise at what the raise cost, so a perfect parry is at
/// best free. A raise during the cooldown is refused and earns nothing.
pub const PARRY_PERFECT_COOLDOWN_REFUND: f32 = 0.6;
pub const PARRY_HITSTOP: f32 = 0.06;
pub const PARRY_FLASH: f32 = 0.3;
pub const PARRY_REFUND_CAP: f32 = PARRY_COST;
/// A reflected shot is re-aimed at the nearest creature or base within `PARRY_AIM_RANGE` that
/// lies within `PARRY_AIM_CONE` radians of the mirrored heading, and seeks at this level.
pub const PARRY_AIM_RANGE: f32 = 1100.0;
pub const PARRY_AIM_CONE: f32 = 1.1;
pub const PARRY_REFLECT_HOMING: u8 = 2;
/// Unlock and level prices (the first purchase unlocks it), and the part it needs on the
/// ship: a Rare or better plating, so it arrives mid-game.
pub const PRICE_PARRY: [(Material, f32); 3] = [
    (Material::Metal, 120.0),
    (Material::Crystal, 40.0),
    (Material::Volatiles, 40.0),
];

// ---- dash (a locked upgrade: level 0 is not owned) -----------------------------------------

/// How far a dash jumps at level 1, and the gain per level after.
pub const DASH_DISTANCE: f32 = 240.0;
pub const DASH_DISTANCE_STEP: f32 = 30.0;
/// A dash that could cover less than this (hard against something solid) is refused free.
pub const DASH_MIN: f32 = 24.0;
/// Seconds before the next dash, less per level, and the shield energy one costs.
pub const DASH_COOLDOWN: f32 = 1.2;
pub const DASH_COOLDOWN_STEP: f32 = 0.15;
pub const DASH_COST: f32 = 8.0;
/// Seconds of invulnerability a dash grants, and how long its trail stays drawn.
pub const DASH_INVULN: f32 = 0.3;
pub const DASH_TRAIL: f32 = 0.35;
/// Passing through a hostile shot or a flinger's touch during the invulnerable window is a
/// graze. The first graze of a dash adds one stack of damage (`DASH_BOOST_STEP` each, up to
/// `DASH_BOOST_STACKS`) for `DASH_BOOST_TIME` seconds (each new graze refreshes it) and gives
/// back some shield (less than a dash costs, so dashing is never free profit).
pub const DASH_BOOST_TIME: f32 = 4.0;
pub const DASH_BOOST_STEP: f32 = 0.25;
pub const DASH_BOOST_STACKS: u8 = 3;
pub const DASH_GRAZE_REFUND: f32 = 5.0;
/// A creature the ship dashes through staggers: its speed is damped, and it can neither
/// strike, fling nor shoot for this long.
pub const DASH_STAGGER: f32 = 0.8;
pub const DASH_STAGGER_DAMP: f32 = 0.3;
/// Reach beyond the ship's radius and the target's that counts as passing through.
pub const DASH_GRAZE_MARGIN: f32 = 6.0;
/// Unlock and level prices, and the part the ship needs first: a Rare or better engine.
pub const PRICE_DASH: [(Material, f32); 3] = [
    (Material::Metal, 100.0),
    (Material::Crystal, 30.0),
    (Material::Volatiles, 30.0),
];
