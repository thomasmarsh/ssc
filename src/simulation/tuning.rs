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
/// repeated after `TITHE_COOLDOWN` seconds. A friend gives something back instead: a repair if
/// the hull is below `TRADE_REPAIR_BELOW` of full, else `TRADE_RATE` of the amount in the material
/// the hold has least of.
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
