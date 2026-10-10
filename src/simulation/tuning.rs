//! Economy and skill tuning in one place: how tough rocks are, how fast the beam works,
//! what the rig upgrades cost and give. Change numbers here, not at the use sites.
//!
//! The intent for rocks: the mining beam is the way to turn rock into material, and shooting
//! one is a slow, wasteful way to lose it (see `mining` tests that pin the ratio).

use super::Material;

// ---- per-tick body physics and timers (see `phases`) ---------------------------------------------

/// Seconds a body must go unhit before its shield starts to recharge.
pub const SHIELD_RECHARGE_DELAY: f32 = 2.0;
/// Shield points per second every non-player body recharges (the ship uses its `recharge` stat).
pub const NPC_SHIELD_RATE: f32 = 6.0;
/// Health per second a dust-grazing creature recovers.
pub const DUST_HEAL: f32 = 1.5;
/// A free rock faster than this slowly sheds its excess speed (units per second).
pub const ASTEROID_SPEED_FLOOR: f32 = 120.0;
/// Rate (per second) at which a free rock's excess speed decays toward the floor.
pub const ASTEROID_DRAG: f32 = 0.5;
/// Radians per second a free rock turns.
pub const ASTEROID_SPIN: f32 = 0.3;
/// Radians per second a planetoid turns.
pub const PLANETOID_SPIN: f32 = 0.02;

// ---- realms (see `realm` and `realms`) ----------------------------------------------------------

/// Seconds the ship must hold a new realm before it is announced, and the least gap between two
/// realm banners (the same hysteresis as regions, a little longer since realms are huge).
pub const REALM_HOLD: f32 = 4.0;
pub const REALM_COOLDOWN: f32 = 30.0;
/// A hit never loses more than this share of its damage to a realm's flat plating: a light gun
/// is turned away mostly, never entirely.
pub const PLATING_FLOOR: f32 = 0.25;
/// Past this many sectors of reach, hostile arrows beyond the sensor's range are dropped when a
/// realm cuts the sensors (the base reach of the threat arrows).
pub const THREAT_SENSE_RANGE: f32 = 14_000.0;
/// Stream salt of the roll that decides whether an ability fizzles (see `realm::Effects`).
pub const FIZZLE_SALT: u64 = 0xF122_1E00_0000_0071;
/// A fizzled dash or parry costs nothing but locks the ability for this long.
pub const FIZZLE_LOCK: f32 = 0.6;

// ---- sniping counters (see `adapt`, `apexes` and `Profile::reach`) ---------------------------

/// Distance over which a shot's damage slides from full (at its profile's sweet spot) to the
/// profile's floor (see `arsenal::Reach`).
pub const FALLOFF_SPAN: f32 = 1400.0;
/// A heavy gun kicks the ship: an impulse per trigger pull of `RECOIL_PER_DAMAGE` units of
/// speed for each point of damage a shot has above the stock gun's, times the profile's weight,
/// at most `RECOIL_CAP`. Heavy shots are also slower: their speed is `1 / (1 + HEAVY_SLOW * (d /
/// stock - 1))` of the stat, never under `HEAVY_SLOW_FLOOR`, so a big gun's shot can be led and
/// dodged and its reach shrinks a little.
pub const RECOIL_PER_DAMAGE: f32 = 0.2;
pub const RECOIL_CAP: f32 = 28.0;
pub const HEAVY_SLOW: f32 = 0.06;
pub const HEAVY_SLOW_FLOOR: f32 = 0.6;
/// Adaptive resistance. A creature adapts when its hull plus shield is at least
/// `ADAPT_MIN_POOL` (every apex does). Each hit adds `ADAPT_GAIN * dealt / pool` to the meter of
/// its damage family (so a third of the pool dealt by one family fills it), up to one. The
/// resistance is `ADAPT_MAX * meter` of the damage: it never goes beyond `ADAPT_MAX`, so a hit
/// always lands for `1 - ADAPT_MAX`. A meter bleeds `ADAPT_DECAY` a second, and `ADAPT_IDLE_DECAY`
/// more once its family has not hit for `ADAPT_IDLE` seconds, so switching guns relieves it.
pub const ADAPT_MIN_POOL: f32 = 420.0;
pub const ADAPT_GAIN: f32 = 3.0;
pub const ADAPT_MAX: f32 = 0.55;
pub const ADAPT_DECAY: f32 = 0.02;
pub const ADAPT_IDLE: f32 = 1.5;
pub const ADAPT_IDLE_DECAY: f32 = 0.09;
/// A hull bar shows a family's pip from this meter up.
pub const ADAPT_SHOWN: f32 = 0.08;
/// Apex range closers. An apex counts as sniped once the ship has hurt it from beyond
/// `SNIPE_RANGE` (it was hit within `SNIPE_WINDOW` seconds) for `SNIPE_AFTER` seconds in all
/// (it bleeds off at half speed otherwise); it then lunges: a planted `LUNGE_WINDUP`, then
/// `LUNGE_TIME` seconds at `LUNGE_SPEED` along the ship.
pub const SNIPE_RANGE: f32 = 1000.0;
pub const SNIPE_WINDOW: f32 = 1.2;
pub const SNIPE_AFTER: f32 = 5.0;
pub const LUNGE_WINDUP: f32 = 0.7;
pub const LUNGE_TIME: f32 = 1.0;
pub const LUNGE_SPEED: f32 = 720.0;
/// Apex barrage: a telegraphed fan of slow shots at a ship beyond `BARRAGE_RANGE`, every
/// (calm, enraged) seconds after `BARRAGE_WINDUP` of warning; `BARRAGE_SHOTS` (more when
/// enraged), each `BARRAGE_SPEED` fast and of `BARRAGE_SHARE` of a pellet, reaching
/// `BARRAGE_REACH`. The fan aims where the ship will be (a lead of `BARRAGE_LEAD` seconds).
pub const BARRAGE_RANGE: f32 = 1100.0;
pub const BARRAGE_EVERY: (f32, f32) = (8.0, 5.0);
pub const BARRAGE_WINDUP: f32 = 1.2;
pub const BARRAGE_SHOTS: (u8, u8) = (9, 13);
pub const BARRAGE_SPEED: f32 = 360.0;
pub const BARRAGE_SHARE: f32 = 1.2;
pub const BARRAGE_REACH: f32 = 1800.0;
pub const BARRAGE_LEAD: f32 = 0.5;
/// A bubble (on a Warden, and on every elder of a realm that shields them): shots from beyond
/// `BUBBLE_RANGE` of the elder leak `BUBBLE_LEAK` of their damage; a lance shot passes whole. A
/// shot from inside the range hurts the bubble as well: `BUBBLE_BREAK` of the elder's pool of it
/// in close damage breaks the bubble for `BUBBLE_DOWN` seconds, then it re-forms whole.
pub const BUBBLE_RANGE: f32 = 520.0;
pub const BUBBLE_LEAK: f32 = 0.15;
pub const BUBBLE_BREAK: f32 = 0.1;
pub const BUBBLE_DOWN: f32 = 9.0;
/// A bubble that has not been hit up close for `BUBBLE_REST` seconds mends this much a second.
pub const BUBBLE_REST: f32 = 3.0;
pub const BUBBLE_MEND: f32 = 0.05;

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

// ---- kinetic impacts -----------------------------------------------------------------------

/// Bodies striking each other hurt both by their reduced mass and the speed they close at (see
/// `impact`). Below `IMPACT_MIN_SPEED` (normal flight, schooling, resting contact) nothing is
/// dealt, which also keeps separation jitter harmless; speeds are clamped at
/// `IMPACT_SPEED_CAP`. Damage is `IMPACT_SCALE * 1/2 * reduced mass * (speed - min)^2`, capped
/// at `IMPACT_CAP`. The ship takes `IMPACT_PLAYER_SHARE` of it. A pair that has just struck
/// cannot strike again for `IMPACT_PAIR_COOLDOWN` seconds.
pub const IMPACT_MIN_SPEED: f32 = 300.0;
pub const IMPACT_SPEED_CAP: f32 = 1400.0;
pub const IMPACT_SCALE: f32 = 3.0e-5;
pub const IMPACT_CAP: f32 = 160.0;
pub const IMPACT_PLAYER_SHARE: f32 = 0.5;
pub const IMPACT_PAIR_COOLDOWN: f32 = 0.35;

// ---- shoving rocks (the SHOVE and PLATING skills; see `shove`) --------------------------------

/// A free body (a rock, a husk, a drifting creature) the ship rams is shoved by the contact
/// solver as usual (momentum by mass ratio). The SHOVE skill adds an extra push on the body
/// only: the imparted momentum is multiplied by `1 + SHOVE_MULT_STEP * level`, along a blend
/// of the contact normal and the ship's travel (`SHOVE_AIM`), needing a real closing speed of
/// `SHOVE_MIN_CLOSING`. The extra speed is capped (`SHOVE_BONUS_DV` plus a step a level) and a
/// body can take one extra push per `SHOVE_COOLDOWN` seconds, so the most a body can gain
/// from repeated rams is bounded per second. A body the ship shoved (or that a shoved body
/// struck) stays tagged for `SHOVE_TAG` seconds and is held to a speed cap
/// (`SHOVE_SPEED_CAP` plus a step a level) so it cannot tunnel through a wall.
pub const SHOVE_MULT_STEP: f32 = 0.25;
pub const SHOVE_AIM: f32 = 0.5;
pub const SHOVE_MIN_CLOSING: f32 = 120.0;
pub const SHOVE_BONUS_DV: f32 = 240.0;
pub const SHOVE_BONUS_DV_STEP: f32 = 40.0;
pub const SHOVE_COOLDOWN: f32 = 0.6;
pub const SHOVE_TAG: f32 = 5.0;
pub const SHOVE_SPEED_CAP: f32 = 700.0;
pub const SHOVE_SPEED_CAP_STEP: f32 = 60.0;
/// The beam's grip on the rock it works: a one-sided soft spring on the rock (it never pushes
/// back, so the ship can still ram it). Past `GRIP_SLACK` of open space between the hulls the
/// rock is pulled in at `GRIP_PULL` a second per unit of stretch, at most `GRIP_ACCEL` (a
/// step more a level, scaled down for heavy rocks against `GRIP_REF_MASS`) and its sideways drift
/// relative to the ship is damped at `GRIP_DAMP` a second. It comes in no faster than the
/// ship's pace plus `GRIP_APPROACH` a second per unit of stretch, so it settles at the slack. It never makes the rock faster than
/// the fastest of itself, the ship and `GRIP_FLOOR`, so it cannot add energy. Beyond `GRIP_REACH` (plus a step
/// a level) it breaks away and stays off for `GRIP_RETRY` seconds; a ram lets go for
/// `GRIP_RELEASE` seconds so the shoved rock is not yanked back.
pub const GRIP_SLACK: f32 = 70.0;
pub const GRIP_PULL: f32 = 3.0;
pub const GRIP_ACCEL: f32 = 380.0;
pub const GRIP_ACCEL_STEP: f32 = 90.0;
pub const GRIP_REF_MASS: f32 = 18.0;
pub const GRIP_DAMP: f32 = 2.0;
pub const GRIP_APPROACH: f32 = 3.0;
pub const GRIP_FLOOR: f32 = 90.0;
pub const GRIP_REACH: f32 = 190.0;
pub const GRIP_REACH_STEP: f32 = 25.0;
pub const GRIP_RETRY: f32 = 1.0;
pub const GRIP_RELEASE: f32 = 1.2;
/// A dash that ends next to a free rock (within `WHIP_REACH` of its face, inside a cone of
/// `WHIP_CONE` as a cosine) cracks it like a whip: an impulse of `WHIP_IMPULSE` (more by
/// `WHIP_STEP` a level) along the dash, so a light rock flies and a heavy one budges, at most
/// `WHIP_DV` of speed. The dash's invulnerability keeps the ship safe.
pub const WHIP_REACH: f32 = 140.0;
pub const WHIP_CONE: f32 = 0.75;
pub const WHIP_IMPULSE: f32 = 4500.0;
pub const WHIP_STEP: f32 = 0.3;
pub const WHIP_DV: f32 = 520.0;
/// PLATING takes down the share of an impact the ship itself takes. Impacts it caused (it was
/// the faster closer, or the other body is a shoved one) are cut by `PLATING_CAUSED_STEP` of the
/// share a level; from `PLATING_ALL_FROM` every other collision is cut by `PLATING_ALL_STEP` a
/// level from there.
pub const PLATING_CAUSED_STEP: f32 = 0.2;
pub const PLATING_ALL_FROM: u8 = 3;
pub const PLATING_ALL_STEP: f32 = 0.15;
/// First-level prices: metal builds, crystal tunes.
pub const PRICE_SHOVE: [(Material, f32); 2] = [(Material::Metal, 35.0), (Material::Crystal, 8.0)];
pub const PRICE_SHOVE_PLATING: [(Material, f32); 2] =
    [(Material::Metal, 45.0), (Material::Crystal, 10.0)];

// ---- organs and symbiosis (see `organs` and `parasite`) ---------------------------------------

/// An organ is an owned strain, level 1 to `ORGAN_LEVELS`; a level multiplies the perk by
/// `ORGAN_LEVEL_GAIN`. SYMBIOSIS (a skill, one slot a level, needs a Rare core) opens slots;
/// the first graft of a strain costs `GRAFT_CRYSTAL` crystal and `GRAFT_FUEL` volatiles
/// times its level (swapping an owned, paid strain is free), and each fitted organ draws
/// `ORGAN_UPKEEP` volatiles a minute: at an empty hold it sleeps (dormant, never lost). A lesser
/// find (a sample that cannot raise a strain) pays `LESSER_BIOMASS` per level instead.
pub const ORGAN_LEVELS: u8 = 3;
pub const ORGAN_LEVEL_GAIN: [f32; 3] = [1.0, 1.5, 2.0];
pub const SYMBIOSIS_SLOTS: usize = 3;
pub const GRAFT_CRYSTAL: f32 = 8.0;
pub const GRAFT_FUEL: f32 = 20.0;
pub const ORGAN_UPKEEP: f32 = 0.4;
pub const LESSER_BIOMASS: f32 = 12.0;
/// A bond works at once for `BOND_LOAN` seconds without a slot (and settles into a free slot
/// without a graft cost). A special carrier's first kill leaves a specimen with chance
/// `HARVEST_CHANCE`; one sector in `RELIC_ONE_IN` (from depth `RELIC_FROM`) holds a sealed relic.
pub const BOND_LOAN: f32 = 300.0;
pub const HARVEST_CHANCE: f32 = 0.25;
pub const RELIC_ONE_IN: u64 = 14;
pub const RELIC_FROM: f32 = 2.0;
/// The perks at level 1, times the strain's magnitude (0.6 to 1.6, from the donor's genes):
/// Remora mends this much hull a second after `REMORA_QUIET` quiet seconds; Faraday cuts the
/// length of every jam and glitch by this share (immune when it reaches one); Veil leaves the
/// ship intangible this long after a dash; Skipjack lets a dash hop an obstacle thinner than
/// this (and lands clear of every other).
pub const REMORA_REGEN: f32 = 0.8;
pub const REMORA_QUIET: f32 = 2.0;
pub const FARADAY_CUT: f32 = 0.3;
pub const VEIL_TIME: f32 = 0.35;
pub const SKIP_THICK: f32 = 80.0;
/// Price of the first SYMBIOSIS level (growing with `PRICE_GROWTH`): volatiles and crystal.
pub const PRICE_SYMBIOSIS: [(Material, f32); 2] =
    [(Material::Volatiles, 40.0), (Material::Crystal, 20.0)];

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
/// Ore a planetoid will give before it is spent (it never shrinks).
pub const PLANETOID_BUDGET: f32 = 400.0;
/// Substrate worked per second by the beam.
pub const RATE_PLAIN: f32 = 4.5;
pub const RATE_HUSK: f32 = 2.5;
pub const RATE_PLANETOID: f32 = 1.2;

/// Onboard electrolysis: slow emergency refueling, powered by the ship's shield.
pub const ELECTROLYSIS_WATER_RATE: f32 = 0.5;
pub const ELECTROLYSIS_FUEL_PER_WATER: f32 = 2.0;
pub const ELECTROLYSIS_SHIELD_PER_WATER: f32 = 12.0;
pub const ELECTROLYSIS_MAX_SPEED: f32 = 8.0;

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

// ---- sonar upgrades (bench SONAR tab; the base ping is unchanged at level 0) -----------------

/// Per level: extra reach (units), extra ring speed, seconds off the cooldown and extra echoes
/// of every kind. The cooldown never drops below `PING_COOLDOWN_FLOOR`.
pub const PING_REACH_STEP: f32 = 4_000.0;
pub const PING_SPEED_STEP: f32 = 1_500.0;
pub const PING_COOLDOWN_STEP: f32 = 0.8;
pub const PING_COOLDOWN_FLOOR: f32 = 1.5;
pub const PING_TARGETS_STEP: usize = 1;
/// Reveal tiers (bought once each, level 0 shows nothing of that kind): how many echoes of
/// each new kind one ping may return before the targets upgrade.
pub const CAP_PAD_ALERT: usize = 3;
pub const CAP_LODE: usize = 3;
pub const CAP_NEST: usize = 2;
pub const CAP_EGGS: usize = 2;
pub const CAP_PREDATORS: usize = 3;
/// A free rock holding at least this much ore counts as a rich lode to the sonar.
pub const LODE_MIN_ORE: f32 = 60.0;
pub const PRICE_PING_REACH: [(Material, f32); 2] =
    [(Material::Metal, 20.0), (Material::Crystal, 6.0)];
pub const PRICE_PING_SPEED: [(Material, f32); 2] =
    [(Material::Volatiles, 15.0), (Material::Crystal, 4.0)];
pub const PRICE_PING_COOLDOWN: [(Material, f32); 2] =
    [(Material::Volatiles, 20.0), (Material::Crystal, 6.0)];
pub const PRICE_PING_TARGETS: [(Material, f32); 2] =
    [(Material::Metal, 20.0), (Material::Volatiles, 15.0)];
pub const PRICE_ECHO_PADS: [(Material, f32); 2] =
    [(Material::Metal, 25.0), (Material::Crystal, 5.0)];
pub const PRICE_ECHO_LODES: [(Material, f32); 2] =
    [(Material::Metal, 30.0), (Material::Crystal, 10.0)];
pub const PRICE_ECHO_NESTS: [(Material, f32); 2] =
    [(Material::Volatiles, 40.0), (Material::Crystal, 15.0)];
pub const PRICE_ECHO_PREDATORS: [(Material, f32); 3] = [
    (Material::Metal, 40.0),
    (Material::Volatiles, 40.0),
    (Material::Crystal, 20.0),
];

// ---- renewable planetoids ------------------------------------------------------------------

/// Share of planetoids that regrow what the beam takes (chosen by a hash of the spawn key),
/// and how fast: ore per second, so a spent one is whole again after
/// `PLANETOID_BUDGET / REGROW_RATE` seconds.
pub const RENEWABLE_SHARE: f32 = 0.34;
pub const REGROW_RATE: f32 = 0.5;

// ---- chart, beacons and fast travel --------------------------------------------------------

/// Most pins the player may keep on the chart.
pub const MAX_PINS: usize = 24;
/// Beacons the rig may have standing at once: one per upgrade level.
pub const BEACONS_PER_LEVEL: usize = 1;
/// A beacon's price at its first level (the unlock), growing with `PRICE_GROWTH`.
pub const PRICE_BEACON: [(Material, f32); 3] = [
    (Material::Metal, 60.0),
    (Material::Crystal, 20.0),
    (Material::Volatiles, 20.0),
];
/// Fast travel to a beacon. The jump costs volatiles and crystal that grow with the distance in
/// sectors (rounded up), is refused beyond `TRAVEL_MAX_SECTORS`, and takes a charge-up that
/// grows with distance and shortens by `TRAVEL_CHARGE_LEVEL_CUT` of itself for each beacon
/// level after the first. Any damage to the ship during the charge breaks it: half the cost is
/// refunded and a short cooldown follows. A completed jump starts the long cooldown and leaves
/// the ship exposed on arrival: shield held at zero and no protection for a few seconds.
pub const TRAVEL_VOLATILES_BASE: f32 = 8.0;
pub const TRAVEL_VOLATILES_PER_SECTOR: f32 = 3.0;
pub const TRAVEL_CRYSTAL_BASE: f32 = 2.0;
pub const TRAVEL_CRYSTAL_PER_SECTOR: f32 = 0.75;
pub const TRAVEL_MAX_SECTORS: f32 = 40.0;
pub const TRAVEL_CHARGE_BASE: f32 = 4.0;
pub const TRAVEL_CHARGE_PER_SECTOR: f32 = 0.4;
pub const TRAVEL_CHARGE_MAX: f32 = 14.0;
pub const TRAVEL_CHARGE_LEVEL_CUT: f32 = 0.08;
pub const TRAVEL_COOLDOWN: f32 = 180.0;
pub const TRAVEL_CANCEL_COOLDOWN: f32 = 8.0;
pub const TRAVEL_CANCEL_REFUND: f32 = 0.5;
pub const TRAVEL_EXPOSED: f32 = 3.0;

// ---- legacy and wrecks ---------------------------------------------------------------------

/// When the run ends, this share of what it mined (per material) is carried into the next, up
/// to a cap per material. With insurance on the share and cap are larger and one weapon
/// profile (the highest level owned) is carried too, at most `LEGACY_WEAPON_LEVEL_CAP`.
pub const LEGACY_FRACTION: f32 = 0.25;
pub const LEGACY_CAP: f32 = 120.0;
pub const LEGACY_FRACTION_BARE: f32 = 0.10;
pub const LEGACY_CAP_BARE: f32 = 40.0;
pub const LEGACY_WEAPON_LEVEL_CAP: u8 = 2;
/// Seconds of a new run the HUD shows what the legacy brought.
pub const LEGACY_HUD_SECONDS: f32 = 25.0;
/// The wreck a lost ship leaves: what it held of each material (up to this), and the best part.
/// Flying within `WRECK_RADIUS` recovers it. At most `MAX_WRECKS` wait; the oldest is lost.
pub const WRECK_CAP: f32 = 150.0;
pub const WRECK_RADIUS: f32 = 140.0;
pub const MAX_WRECKS: usize = 3;
/// A wreck in a living civilization's territory is looted after this many seconds of play, plus
/// a deterministic share of the jitter (chosen by a hash of the seed, sector and wreck).
pub const LOOT_AFTER: f32 = 240.0;
pub const LOOT_JITTER: f32 = 240.0;

// ---- regions -------------------------------------------------------------------------------

/// A new region must hold the ship this many seconds before it is announced, and at least
/// `REGION_COOLDOWN` seconds must have passed since the last banner, so flying along a border
/// does not flicker the ENTERING notice.
pub const REGION_HOLD: f32 = 3.0;
/// Auto repair: the ship mends itself after this many quiet seconds (no damage, thrust, fire or
/// beam). Hull comes from metal; the shield only mends this way when it is below the share
/// given, and never takes the last of the volatiles (they are fuel).
pub const AUTO_REPAIR_DELAY: f32 = 3.0;
pub const AUTO_SHIELD_BELOW: f32 = 0.5;
pub const AUTO_FUEL_RESERVE: f32 = 25.0;
pub const REGION_COOLDOWN: f32 = 12.0;

// ---- diplomacy -----------------------------------------------------------------------------

/// What a civilization thinks of the ship is one number, `REGARD_MIN` to `REGARD_MAX`, read as a
/// tier (see `diplomacy`): hostile at or below `HOSTILE_AT`, wary at or below `WARY_AT`, friendly
/// at or above `FRIENDLY_AT`, otherwise it ignores the ship. Rising across a line needs
/// `TIER_HYSTERESIS` more, so a border does not flicker its banner.
pub const REGARD_MIN: f32 = -100.0;
pub const REGARD_MAX: f32 = 100.0;
pub const HOSTILE_AT: f32 = -40.0;
pub const WARY_AT: f32 = -12.0;
pub const FRIENDLY_AT: f32 = 40.0;
pub const TIER_HYSTERESIS: f32 = 3.0;
/// Where a civilization starts (the early outpost starts kinder), and how far leaving it alone
/// can raise it (only gifts take an ordinary civilization to friendly; the outpost's settlers
/// warm to a quiet neighbour on their own).
pub const REGARD_START: f32 = 0.0;
pub const REGARD_START_OUTPOST: f32 = 15.0;
pub const REST_CAP: f32 = 25.0;
pub const REST_CAP_OUTPOST: f32 = 60.0;
/// Regard recovered per second while the ship lingers inside a claim, once `REST_DELAY` seconds
/// have passed since the last offence. Away from the claim old grudges fade at `AWAY_RATE`, never
/// past where the civilization started.
pub const REST_RATE: f32 = 0.1;
pub const REST_DELAY: f32 = 20.0;
pub const AWAY_RATE: f32 = 0.01;
/// Regard lost per point of damage the ship deals to a member, and to a structure (station,
/// turret or wall piece); and for a kill, by role. A wall piece barely counts.
pub const HURT_MEMBER: f32 = 0.08;
pub const HURT_STRUCTURE: f32 = 0.02;
pub const KILL_MEMBER: f32 = 5.0;
pub const KILL_WARRIOR: f32 = 7.0;
pub const KILL_ELDER: f32 = 30.0;
pub const KILL_TURRET: f32 = 8.0;
pub const KILL_WALL: f32 = 0.4;
pub const KILL_OUTPOST_BASE: f32 = 30.0;
pub const KILL_CAPITAL: f32 = 45.0;
/// A kill counts against the ship if it struck the victim within this many seconds.
pub const KILL_WINDOW: f32 = 8.0;
/// Regard lost per unit of ore the beam takes inside a claim (a friend minds a quarter as much),
/// and the seconds between the warnings the beam earns.
pub const MINE_COST: f32 = 0.12;
pub const MINE_COST_FRIEND: f32 = 0.25;
pub const MINE_WARN_EVERY: f32 = 15.0;
/// The tithe: fly within `TITHE_RANGE` of a seat (past its hull) and press the key. It takes
/// `TITHE_AMOUNT` of the material the hold has most of, raises regard by `TITHE_GAIN` and may be
/// repeated after `TITHE_COOLDOWN` seconds. A friend ranks available repair and finite granary trade
/// through culture (critical hull requires repair); the legacy fallback gives `TRADE_RATE`
/// of the amount in the material the hold has least of.
pub const TITHE_RANGE: f32 = 420.0;
pub const TITHE_AMOUNT: f32 = 20.0;
pub const TITHE_GAIN: f32 = 9.0;
pub const TITHE_COOLDOWN: f32 = 2.0;
pub const TRADE_GAIN: f32 = 1.5;
pub const TRADE_RATE: f32 = 0.75;
pub const TRADE_REPAIR_BELOW: f32 = 0.75;
/// How far a friendly civilization's shared charts reach past its claim, in sectors.
pub const SHARE_MARGIN: i32 = 1;
/// Doctrine tables learn from members at this multiple of `TABLE_PULL`, by tier (hostile,
/// wary, ignores, friendly): a civilization at war drills faster than one at peace.
pub const DOCTRINE_PULL: [f32; 4] = [1.5, 1.0, 0.5, 0.25];

// ---- apex elders ---------------------------------------------------------------------------

/// The banner "APEX: NAME stirs" posts once, when an apex first comes within this of the ship.
pub const APEX_NOTICE_RANGE: f32 = 3600.0;
/// The HUD line and edge arrow follow the nearest apex this far.
pub const APEX_HUD_RANGE: f32 = 9000.0;
/// Score for slaying one (times the place's threat), halved for a lesser one.
pub const APEX_SCORE: f32 = 2500.0;
/// What a slain apex drops: this much of each material (halved for a lesser one), three parts
/// (one epic in the slot of a still-locked ability, two rare) and two lucky rolls.
pub const APEX_MATERIAL: f32 = 90.0;

// ---- run summary titles --------------------------------------------------------------------

/// Thresholds for the epithet on the summary panel (see `titles`). They are checked in the
/// order the titles are listed there; the first that holds wins.
pub const TITLE_SCOURGE_EXTIRPATIONS: usize = 2;
pub const TITLE_PROSPECT_MINED: f32 = 150.0;
pub const TITLE_TITHES: u32 = 5;
pub const TITLE_PARRIES: u32 = 10;
pub const TITLE_DASHES: u32 = 40;
pub const TITLE_CARTOGRAPHER_SECTORS: usize = 12;
pub const TITLE_CARTOGRAPHER_REGIONS: usize = 3;
/// A gentle cartographer destroyed at most one creature per this many sectors explored.
pub const TITLE_GENTLE_KILLS_PER_SECTOR: u32 = 3;
pub const TITLE_RECKLESS_DEATHS: u32 = 3;
pub const TITLE_RECKLESS_SECONDS: f32 = 360.0;
pub const TITLE_HERMIT_MINED: f32 = 300.0;
pub const TITLE_HERMIT_KILLS: u32 = 10;

// ---- wildlife and civilizations ------------------------------------------------------------

/// How each species regards each civilization is `affinity` (pure, -1 hostile to 1 friendly; its
/// own numbers live in `src/affinity.rs`). The simulation re-reads who is near whom every
/// `FAUNA_PERIOD` seconds. Wildlife nearer than `HOSTILE_REACH` to a civil body it is hostile
/// to sets upon it; a friendly species within `FRIEND_RANGE` drifts toward the settlement and
/// holds at `HERD_RING` (pull as a multiple of its cruise speed).
pub const FAUNA_PERIOD: f32 = 0.25;
pub const HOSTILE_REACH: f32 = 1100.0;
pub const FRIEND_RANGE: f32 = 1600.0;
pub const HERD_RING: f32 = 480.0;
pub const HERD_PULL: f32 = 1.8;
/// Pressure limits: at most `MAX_ATTACKERS` wildlife on one civil body and `MAX_ASSAULT` on one
/// civilization at once (`MAX_ASSAULT_PEACEFUL` on a settlement), whatever is hostile nearby.
pub const MAX_ATTACKERS: usize = 3;
pub const MAX_ASSAULT: usize = 8;
pub const MAX_ASSAULT_PEACEFUL: usize = 3;
/// A bite: `FAUNA_BITE` plus `FAUNA_BITE_PER_CONTACT` per point of the genome's contact damage
/// (times the attacker's threat), every `FAUNA_BITE_PERIOD` seconds, within `FAUNA_BITE_REACH`
/// of the other's rim. A structure takes `FAUNA_STRUCTURE_SCALE` of it, a settlement's people
/// and works only `FAUNA_PEACEFUL_SCALE` more. Wildlife never destroys a structure or an elder
/// (their fall is the ship's doing): they bottom out at `FAUNA_STRUCTURE_FLOOR` of their hull.
pub const FAUNA_BITE: f32 = 5.0;
pub const FAUNA_BITE_PER_CONTACT: f32 = 0.5;
pub const FAUNA_BITE_PERIOD: f32 = 0.9;
pub const FAUNA_BITE_REACH: f32 = 45.0;
pub const FAUNA_STRUCTURE_SCALE: f32 = 0.35;
pub const FAUNA_PEACEFUL_SCALE: f32 = 0.4;
pub const FAUNA_STRUCTURE_FLOOR: f32 = 0.4;
/// Pace of a hostile on the attack (of its full speed) and of a defender on the hunt.
pub const ATTACK_PACE: f32 = 0.9;
pub const DEFEND_PACE: f32 = 0.8;
/// A civilization's idle people hunt hostile wildlife within `DEFEND_RANGE` of themselves and
/// `DEFEND_LEASH` of their post. An armed one strikes from `DEFEND_REACH_SHARE` of its weapon
/// range (an unarmed one at the rim), for `STRIKE_DAMAGE` (times its threat) at most every
/// fire period and `STRIKE_PERIOD_MIN`. A fortress turret reaches `TURRET_DEFEND_RANGE`.
pub const DEFEND_RANGE: f32 = 1000.0;
pub const DEFEND_LEASH: f32 = 1500.0;
pub const DEFEND_REACH_SHARE: f32 = 0.6;
pub const STRIKE_DAMAGE: f32 = 9.0;
pub const STRIKE_PERIOD_MIN: f32 = 0.8;
pub const TURRET_DEFEND_RANGE: f32 = 950.0;
pub const TURRET_STRIKE: f32 = 16.0;

/// Killing wildlife near a civilization's people (see `wildlife::wildlife_killed`). A kill of a
/// species the civilization is friendly to costs `FRIEND_KILL_COST` regard times its affinity
/// (0.3 to 1). A kill of a hostile one earns `HOSTILE_KILL_GAIN` times how much it is hated,
/// times `GAIN_DIMINISH` for each earlier kill in the window, at most `GAIN_CAP` per civilization
/// per `GAIN_WINDOW` seconds, so it cannot be farmed. A banner at most every `KILL_BANNER_EVERY`.
pub const FRIEND_KILL_COST: f32 = 3.0;
pub const HOSTILE_KILL_GAIN: f32 = 1.5;
pub const GAIN_DIMINISH: f32 = 0.7;
pub const GAIN_CAP: f32 = 6.0;
pub const GAIN_WINDOW: f32 = 300.0;
pub const KILL_BANNER_EVERY: f32 = 4.0;
/// The HUD names up to `TAG_COUNT` species within `TAG_RANGE` of the ship, hostile or friendly to
/// the territory it is in.
pub const TAG_COUNT: usize = 3;
pub const TAG_RANGE: f32 = 2600.0;

// ---- drops ---------------------------------------------------------------------------------

/// Where the parts come from. Wild stations are gone (they were cheap farms: a few hits for a
/// part and a handful of rolls), so permanent gear comes from what is hard: civilizations,
/// apex elders and tough creatures.
///
/// A creature's drop chance is multiplied by its hardness,
/// `HARD_FLOOR + HARD_SLOPE * (hull + shield) / HARD_REF`, at most `HARD_CAP`: a frail thing
/// (hull 20) drops a little less than it did, a Bogey a quarter more, a juggernaut three
/// and a half times as much.
pub const HARD_FLOOR: f32 = 0.7;
pub const HARD_SLOPE: f32 = 1.0;
pub const HARD_REF: f32 = 100.0;
pub const HARD_CAP: f32 = 3.5;
/// No creature drops more often than this.
pub const DROP_CHANCE_CAP: f32 = 0.6;
/// A civilization's capital pays `CAPITAL_PARTS` parts plus one per fortress tier (0 to 3) and
/// its kind's rolls plus one per tier; its first part is at least Rare from tier 1 and Epic at
/// tier `CAPITAL_EPIC_TIER`. An outpost seat pays one part and `OUTPOST_ROLLS` rolls, one more
/// of each at fortress tier `OUTPOST_BONUS_TIER` or more. Wall turrets pay scrap only.
pub const CAPITAL_PARTS: u32 = 2;
pub const CAPITAL_EPIC_TIER: u8 = 3;
pub const OUTPOST_ROLLS: u32 = 1;
pub const OUTPOST_BONUS_TIER: u8 = 2;
/// An elder (a civilization's boss) pays one more part at fortress tier `ELDER_BONUS_TIER`.
pub const ELDER_BONUS_TIER: u8 = 2;

/// Whole-creature health for jointed bodies (see `simulation/breakup.rs`): a chain born where
/// the place's threat is at most `POOL_FULL_THREAT` (about ring 3) has one health pool, and
/// from `POOL_NONE_THREAT` (about ring 9) each part is its own life as before; in between the
/// pool shrinks to the plain sum of parts. At full pool each part beyond the head counts only
/// `POOL_FLOOR` of its hull toward the pool, so a ten-part body takes about 2.4 heads of
/// damage, not ten. Pieces shed as it weakens drift for `POOL_DRIFT` s at up to `POOL_FLING`
/// speed, then vanish; the kill pays the head's bounty times the pool's size in heads.
pub const POOL_FULL_THREAT: f32 = 1.9;
pub const POOL_NONE_THREAT: f32 = 3.7;
pub const POOL_FLOOR: f32 = 0.15;
pub const POOL_DRIFT: f32 = 2.5;
pub const POOL_FLING: f32 = 70.0;

/// Sustained hostile creature overlap applies one ordinary sting per this many seconds.
/// Drones follow their work ledger, so contact causes damage without displacement.
pub const DRONE_CONTACT_SECONDS: f32 = 0.65;
