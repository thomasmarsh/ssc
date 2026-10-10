//! Economy and skill tuning in one place: how tough rocks are, how fast the beam works,
//! what the rig upgrades cost and give. Change numbers here, not at the use sites.
//!
//! The intent for rocks: the mining beam is the way to turn rock into material, and shooting
//! one is a slow, wasteful way to lose it (see `mining` tests that pin the ratio).
//!
//! Every gameplay number below is an entry of the typed tunables registry (see `tunables` and
//! docs/DEVTOOLS.md): declared once with its default, range, unit, group, doc and effect, read
//! at the use site as a plain field of `Game::tune` (`self.tune.adapt_max`), and changeable live
//! through `Game::tune_set` (the console, a panel or an overrides file) within the range and the
//! cross-field rules of `validate`. The defaults are the shipped values.

use super::tuning_life;

// The core groups are listed here; the groups of the creature, civilization and play modules
// (`tuning_life`, `tuning_civ`, `tuning_play`) follow through a chain of macros that ends in the
// one `tunables!` invocation, so there is one struct, one table and one `validate`.
tuning_life::groups! {
    // ---- per-tick body physics and timers (see `phases`) ----------------------------------------
    group "physics" {
        /// Seconds a body must go unhit before its shield starts to recharge.
        shield_recharge_delay: f32 = 2.0, 0.0, 60.0, Seconds;
        /// Shield points per second every non-player body recharges (the ship uses its `recharge` stat).
        npc_shield_rate: f32 = 6.0, 0.0, 200.0, Rate;
        /// Health per second a dust-grazing creature recovers.
        dust_heal: f32 = 1.5, 0.0, 100.0, Rate;
        /// A free rock faster than this slowly sheds its excess speed.
        asteroid_speed_floor: f32 = 120.0, 0.0, 5000.0, Speed;
        /// Rate at which a free rock's excess speed decays toward the floor.
        asteroid_drag: f32 = 0.5, 0.0, 20.0, Rate;
        /// Radians per second a free rock turns.
        asteroid_spin: f32 = 0.3, 0.0, 10.0, Radians;
        /// Radians per second a planetoid turns.
        planetoid_spin: f32 = 0.02, 0.0, 10.0, Radians;
    }

    // ---- realms (see `realm` and `realms`) -------------------------------------------------------
    group "realms" {
        /// Seconds the ship must hold a new realm before it is announced (realms are huge, so a little longer than regions).
        realm_hold: f32 = 4.0, 0.0, 120.0, Seconds;
        /// Least gap between two realm banners.
        realm_cooldown: f32 = 30.0, 0.0, 600.0, Seconds;
        /// Least share of a hit's damage a realm's flat plating lets through: a light gun is turned away mostly, never entirely.
        plating_floor: f32 = 0.25, 0.0, 1.0, Ratio;
        /// Base reach of the threat arrows: hostile arrows beyond it are dropped when a realm cuts the sensors.
        threat_sense_range: f32 = 14_000.0, 0.0, 500_000.0, Distance;
        /// A fizzled dash or parry costs nothing but locks the ability for this long.
        fizzle_lock: f32 = 0.6, 0.0, 30.0, Seconds;
    }

    // ---- sniping counters (see `adapt`, `apexes` and `Profile::reach`) ---------------------------
    group "sniping" {
        /// Distance over which a shot's damage slides from full (at its profile's sweet spot) to the profile's floor.
        falloff_span: f32 = 1400.0, 1.0, 100_000.0, Distance;
        /// Impulse per trigger pull, in units of speed per point of damage a shot has above the stock gun's.
        recoil_per_damage: f32 = 0.2, 0.0, 10.0, Speed;
        /// Most a heavy gun's recoil kicks the ship per trigger pull.
        recoil_cap: f32 = 28.0, 0.0, 2000.0, Speed;
        /// How much slower a heavy shot is per stock-damage multiple above the stock gun (speed is 1 / (1 + this * (d / stock - 1))).
        heavy_slow: f32 = 0.06, 0.0, 5.0, Multiplier;
        /// A heavy shot is never slower than this share of the stat.
        heavy_slow_floor: f32 = 0.6, 0.05, 1.0, Ratio;
        /// A creature adapts when its hull plus shield is at least this much (every apex does).
        adapt_min_pool: f32 = 420.0, 0.0, 100_000.0, Amount;
        /// Each hit adds this times dealt / pool to the meter of its damage family (a third of the pool fills it).
        adapt_gain: f32 = 3.0, 0.0, 100.0, Multiplier;
        /// Resistance at a full meter: a hit always lands for one minus this.
        adapt_max: f32 = 0.55, 0.0, 0.95, Ratio;
        /// How much of a family's meter bleeds off a second.
        adapt_decay: f32 = 0.02, 0.0, 1.0, Rate;
        /// Seconds a family may go without hitting before its meter bleeds faster (switching guns relieves it).
        adapt_idle: f32 = 1.5, 0.0, 60.0, Seconds;
        /// Extra meter that bleeds off a second once its family has been idle.
        adapt_idle_decay: f32 = 0.09, 0.0, 1.0, Rate;
        /// A hull bar shows a family's pip from this meter up.
        adapt_shown: f32 = 0.08, 0.0, 1.0, Ratio;
        /// An apex counts as sniped when the ship hurts it from beyond this distance.
        snipe_range: f32 = 1000.0, 0.0, 100_000.0, Distance;
        /// A hit this recent still counts toward the apex's sniped clock.
        snipe_window: f32 = 1.2, 0.0, 60.0, Seconds;
        /// Seconds of sniping that make an apex lunge (it bleeds off at half speed otherwise).
        snipe_after: f32 = 5.0, 0.0, 300.0, Seconds;
        /// Seconds a lunging apex stays planted before it charges.
        lunge_windup: f32 = 0.7, 0.0, 10.0, Seconds;
        /// Seconds a lunge lasts.
        lunge_time: f32 = 1.0, 0.1, 10.0, Seconds;
        /// Speed of a lunge along the ship.
        lunge_speed: f32 = 720.0, 0.0, 10_000.0, Speed;
        /// An apex barrages a ship beyond this distance.
        barrage_range: f32 = 1100.0, 0.0, 100_000.0, Distance;
        /// Seconds between barrages while calm.
        barrage_every_calm: f32 = 8.0, 0.5, 300.0, Seconds;
        /// Seconds between barrages while enraged.
        barrage_every_enraged: f32 = 5.0, 0.5, 300.0, Seconds;
        /// Seconds of telegraphed warning before a barrage fires.
        barrage_windup: f32 = 1.2, 0.0, 30.0, Seconds;
        /// Shots in a calm barrage.
        barrage_shots_calm: u8 = 9, 0.0, 60.0, Count;
        /// Shots in an enraged barrage.
        barrage_shots_enraged: u8 = 13, 0.0, 60.0, Count;
        /// Speed of a barrage shot.
        barrage_speed: f32 = 360.0, 0.0, 10_000.0, Speed;
        /// Damage of a barrage shot as a multiple of a pellet.
        barrage_share: f32 = 1.2, 0.0, 20.0, Multiplier;
        /// How far a barrage shot reaches.
        barrage_reach: f32 = 1800.0, 0.0, 100_000.0, Distance;
        /// Seconds of lead: the fan aims where the ship will be.
        barrage_lead: f32 = 0.5, 0.0, 10.0, Seconds;
        /// Shots from beyond this distance of a bubbled elder leak only part of their damage.
        bubble_range: f32 = 520.0, 0.0, 100_000.0, Distance;
        /// Share of a far shot's damage that gets through a bubble (a lance passes whole).
        bubble_leak: f32 = 0.15, 0.0, 1.0, Ratio;
        /// Share of the elder's pool in close damage that breaks the bubble.
        bubble_break: f32 = 0.1, 0.0, 1.0, Ratio;
        /// Seconds a broken bubble stays down before it re-forms whole.
        bubble_down: f32 = 9.0, 0.0, 300.0, Seconds;
        /// Seconds without a close hit before a bubble starts to mend.
        bubble_rest: f32 = 3.0, 0.0, 300.0, Seconds;
        /// How much of a bubble mends a second once it rests.
        bubble_mend: f32 = 0.05, 0.0, 5.0, Rate;
    }

    // ---- rocks -----------------------------------------------------------------------------------
    group "rocks" {
        /// How much tougher free rocks are against the ship's own weapons: damage to them is divided by this.
        rock_hull_factor: f32 = 80.0, 1.0, 100_000.0, Multiplier;
        /// Share of a shot rock's remaining ore that survives in its fragments (the rest is dust).
        shot_ore_keep: f32 = 0.5, 0.0, 1.0, Ratio;
    }

    // ---- kinetic impacts -------------------------------------------------------------------------
    group "impacts" {
        /// Closing speed below which a collision deals nothing (normal flight, schooling, resting contact).
        impact_min_speed: f32 = 300.0, 0.0, 10_000.0, Speed;
        /// Closing speed is clamped here.
        impact_speed_cap: f32 = 1400.0, 1.0, 20_000.0, Speed;
        /// Damage is this times half the reduced mass times (speed - min) squared.
        impact_scale: f32 = 3.0e-5, 0.0, 0.01, Multiplier;
        /// Most damage one impact deals.
        impact_cap: f32 = 160.0, 0.0, 10_000.0, Amount;
        /// Share of an impact's damage the ship takes.
        impact_player_share: f32 = 0.5, 0.0, 1.0, Ratio;
        /// A pair that has just struck cannot strike again for this long.
        impact_pair_cooldown: f32 = 0.35, 0.0, 30.0, Seconds;
    }

    // ---- shoving rocks (the SHOVE and PLATING skills; see `shove`) -------------------------------
    group "shoving" {
        /// Extra momentum on a rammed body per SHOVE level (multiplied by 1 + this * level).
        shove_mult_step: f32 = 0.25, 0.0, 5.0, Multiplier;
        /// Blend of the contact normal (0) and the ship's travel (1) the extra push follows.
        shove_aim: f32 = 0.5, 0.0, 1.0, Ratio;
        /// Real closing speed a ram needs to earn the extra push.
        shove_min_closing: f32 = 120.0, 0.0, 5000.0, Speed;
        /// Most extra speed one shove adds at level 1.
        shove_bonus_dv: f32 = 240.0, 0.0, 5000.0, Speed;
        /// Extra shove speed per level after the first.
        shove_bonus_dv_step: f32 = 40.0, 0.0, 2000.0, Speed;
        /// A body takes one extra push per this many seconds.
        shove_cooldown: f32 = 0.6, 0.0, 60.0, Seconds;
        /// A shoved body stays tagged (speed-capped) for this long.
        shove_tag: f32 = 5.0, 0.0, 60.0, Seconds;
        /// Speed cap of a shoved body at level 1.
        shove_speed_cap: f32 = 700.0, 0.0, 20_000.0, Speed;
        /// Extra speed cap per level.
        shove_speed_cap_step: f32 = 60.0, 0.0, 5000.0, Speed;
        /// Open space between the hulls past which the beam's grip pulls the rock in.
        grip_slack: f32 = 70.0, 0.0, 5000.0, Distance;
        /// Pull per second per unit of stretch.
        grip_pull: f32 = 3.0, 0.0, 100.0, Rate;
        /// Most the grip accelerates a rock at level 1.
        grip_accel: f32 = 380.0, 0.0, 10_000.0, Speed;
        /// Extra grip acceleration per SHOVE level.
        grip_accel_step: f32 = 90.0, 0.0, 5000.0, Speed;
        /// Rock mass the grip is scaled against (heavier rocks are pulled less).
        grip_ref_mass: f32 = 18.0, 0.1, 10_000.0, Amount;
        /// Rate at which the rock's sideways drift relative to the ship is damped.
        grip_damp: f32 = 2.0, 0.0, 100.0, Rate;
        /// Per second per unit of stretch the rock may close faster than the ship's pace.
        grip_approach: f32 = 3.0, 0.0, 100.0, Rate;
        /// The grip never makes a rock faster than the fastest of itself, the ship and this.
        grip_floor: f32 = 90.0, 0.0, 5000.0, Speed;
        /// Beyond this reach the grip breaks away (at level 1).
        grip_reach: f32 = 190.0, 0.0, 5000.0, Distance;
        /// Extra grip reach per SHOVE level.
        grip_reach_step: f32 = 25.0, 0.0, 2000.0, Distance;
        /// Seconds the grip stays off after breaking away.
        grip_retry: f32 = 1.0, 0.0, 60.0, Seconds;
        /// Seconds the grip lets go after a ram so the shoved rock is not yanked back.
        grip_release: f32 = 1.2, 0.0, 60.0, Seconds;
        /// A dash ending within this distance of a rock's face cracks it like a whip.
        whip_reach: f32 = 140.0, 0.0, 5000.0, Distance;
        /// Cosine of the whip's cone around the dash.
        whip_cone: f32 = 0.75, -1.0, 1.0, Ratio;
        /// Impulse of a whip at level 1.
        whip_impulse: f32 = 4500.0, 0.0, 200_000.0, Amount;
        /// Extra whip impulse per level, as a share of the base.
        whip_step: f32 = 0.3, 0.0, 10.0, Multiplier;
        /// Most speed a whip gives a rock.
        whip_dv: f32 = 520.0, 0.0, 20_000.0, Speed;
        /// Share of an impact the ship caused that PLATING cuts per level.
        plating_caused_step: f32 = 0.2, 0.0, 1.0, Ratio;
        /// PLATING level from which every other collision is cut too.
        plating_all_from: u8 = 3, 1.0, 20.0, Count;
        /// Share of every other collision PLATING cuts per level from there.
        plating_all_step: f32 = 0.15, 0.0, 1.0, Ratio;
        /// SHOVE first-level price in metal.
        price_shove_metal: f32 = 35.0, 0.0, 100_000.0, Amount;
        /// SHOVE first-level price in crystal.
        price_shove_crystal: f32 = 8.0, 0.0, 100_000.0, Amount;
        /// SHOVE PLATING first-level price in metal.
        price_shove_plating_metal: f32 = 45.0, 0.0, 100_000.0, Amount;
        /// SHOVE PLATING first-level price in crystal.
        price_shove_plating_crystal: f32 = 10.0, 0.0, 100_000.0, Amount;
    }

    // ---- organs and symbiosis (see `organs` and `parasite`) --------------------------------------
    group "organs" {
        /// Perk multiplier of an organ at level 1.
        organ_level_gain_1: f32 = 1.0, 0.0, 10.0, Multiplier;
        /// Perk multiplier of an organ at level 2.
        organ_level_gain_2: f32 = 1.5, 0.0, 10.0, Multiplier;
        /// Perk multiplier of an organ at level 3.
        organ_level_gain_3: f32 = 2.0, 0.0, 10.0, Multiplier;
        /// Crystal the first graft of a strain costs per level.
        graft_crystal: f32 = 8.0, 0.0, 1000.0, Amount;
        /// Volatiles the first graft of a strain costs per level.
        graft_fuel: f32 = 20.0, 0.0, 1000.0, Amount;
        /// Volatiles a minute each fitted organ draws (an empty hold puts it to sleep).
        organ_upkeep: f32 = 0.4, 0.0, 1000.0, Amount;
        /// Biomass per level a lesser find (a sample that cannot raise a strain) pays instead.
        lesser_biomass: f32 = 12.0, 0.0, 1000.0, Amount;
        /// Seconds a bond works at once without a slot.
        bond_loan: f32 = 300.0, 0.0, 7200.0, Seconds;
        /// Chance a special carrier's first kill leaves a specimen.
        harvest_chance: f32 = 0.25, 0.0, 1.0, Ratio;
        /// One sector in this many (from `relic_from` depth) holds a sealed relic.
        relic_one_in: u64 = 14, 1.0, 100_000.0, Count, Regen;
        /// Depth from which a sector may hold a sealed relic.
        relic_from: f32 = 2.0, 0.0, 100.0, Amount, Regen;
        /// Remora mends this much hull a second after its quiet seconds (times the strain's magnitude).
        remora_regen: f32 = 0.8, 0.0, 100.0, Rate;
        /// Quiet seconds before a Remora mends.
        remora_quiet: f32 = 2.0, 0.0, 60.0, Seconds;
        /// Share by which a Faraday organ cuts every jam and glitch (immune when it reaches one).
        faraday_cut: f32 = 0.3, 0.0, 1.0, Ratio;
        /// Seconds a Veil organ leaves the ship intangible after a dash.
        veil_time: f32 = 0.35, 0.0, 30.0, Seconds;
        /// A Skipjack dash hops an obstacle thinner than this.
        skip_thick: f32 = 80.0, 0.0, 5000.0, Distance;
        /// SYMBIOSIS first-level price in volatiles.
        price_symbiosis_volatiles: f32 = 40.0, 0.0, 100_000.0, Amount;
        /// SYMBIOSIS first-level price in crystal.
        price_symbiosis_crystal: f32 = 20.0, 0.0, 100_000.0, Amount;
    }

    // ---- the mining beam -------------------------------------------------------------------------
    group "mining" {
        /// Beam reach, from the ship's center to the rock's surface, before range upgrades.
        beam_range: f32 = 260.0, 0.0, 20_000.0, Distance;
        /// Shield the beam draws per second.
        beam_drain: f32 = 4.0, 0.0, 1000.0, Rate;
        /// The beam will not fire below this much shield.
        shield_floor: f32 = 6.0, 0.0, 1000.0, Amount;
        /// A rock never shrinks below this radius: it crumbles there.
        crumble_radius: f32 = 14.0, 0.0, 500.0, Distance;
        /// Substrate worked per second by the beam on a plain rock.
        rate_plain: f32 = 4.5, 0.0, 1000.0, Rate;
        /// Substrate worked per second on a husk.
        rate_husk: f32 = 2.5, 0.0, 1000.0, Rate;
        /// Substrate worked per second on a planetoid.
        rate_planetoid: f32 = 1.2, 0.0, 1000.0, Rate;
        /// Water per second onboard electrolysis converts.
        electrolysis_water_rate: f32 = 0.5, 0.0, 100.0, Rate;
        /// Fuel gained per unit of water electrolysed.
        electrolysis_fuel_per_water: f32 = 2.0, 0.01, 100.0, Multiplier;
        /// Shield drawn per unit of water electrolysed.
        electrolysis_shield_per_water: f32 = 12.0, 0.0, 1000.0, Amount;
        /// Electrolysis only runs below this ship speed.
        electrolysis_max_speed: f32 = 8.0, 0.0, 1000.0, Speed;
    }

    // ---- rig upgrades ----------------------------------------------------------------------------
    group "rig" {
        /// Beam rate multiplier gained per POWER level.
        power_step: f32 = 0.35, 0.0, 5.0, Multiplier;
        /// Beam reach gained per RANGE level.
        range_step: f32 = 60.0, 0.0, 2000.0, Distance;
        /// Extra yield per ore gained per YIELD level.
        yield_step: f32 = 0.2, 0.0, 5.0, Multiplier;
        /// Magnet radius gained per MAGNET level.
        magnet_step: f32 = 70.0, 0.0, 5000.0, Distance;
        /// Hold space per material gained per CARGO level.
        cargo_step: f32 = 50.0, 0.0, 5000.0, Amount;
        /// Each level costs this much more than the one before.
        price_growth: f32 = 1.8, 1.0, 10.0, Multiplier;
        /// POWER first-level price in metal.
        price_power_metal: f32 = 30.0, 0.0, 100_000.0, Amount;
        /// POWER first-level price in crystal.
        price_power_crystal: f32 = 4.0, 0.0, 100_000.0, Amount;
        /// RANGE first-level price in metal.
        price_range_metal: f32 = 25.0, 0.0, 100_000.0, Amount;
        /// RANGE first-level price in crystal.
        price_range_crystal: f32 = 6.0, 0.0, 100_000.0, Amount;
        /// YIELD first-level price in volatiles.
        price_yield_volatiles: f32 = 20.0, 0.0, 100_000.0, Amount;
        /// YIELD first-level price in crystal.
        price_yield_crystal: f32 = 8.0, 0.0, 100_000.0, Amount;
        /// MAGNET first-level price in volatiles.
        price_magnet_volatiles: f32 = 20.0, 0.0, 100_000.0, Amount;
        /// MAGNET first-level price in crystal.
        price_magnet_crystal: f32 = 4.0, 0.0, 100_000.0, Amount;
        /// CARGO first-level price in metal.
        price_cargo_metal: f32 = 40.0, 0.0, 100_000.0, Amount;
        /// CARGO first-level price in volatiles.
        price_cargo_volatiles: f32 = 10.0, 0.0, 100_000.0, Amount;
    }

    // ---- parry (a locked upgrade: level 0 is not owned) ------------------------------------------
    group "parry" {
        /// Chance a hostile shot inside the arc is stopped at level 1 (some of a barrage always leaks).
        parry_chance: f32 = 0.70, 0.0, 1.0, Ratio;
        /// Chance gained per level after the first.
        parry_chance_step: f32 = 0.06, 0.0, 1.0, Ratio;
        /// Seconds the shield stays up.
        parry_window: f32 = 0.35, 0.0, 10.0, Seconds;
        /// The first part of the window that is a perfect parry.
        parry_perfect: f32 = 0.12, 0.0, 10.0, Seconds;
        /// Seconds from raising the shield until it can be raised again, at level 1.
        parry_cooldown: f32 = 1.6, 0.0, 60.0, Seconds;
        /// Seconds off the cooldown per level after the first.
        parry_cooldown_step: f32 = 0.15, 0.0, 10.0, Seconds;
        /// Shield energy a raise costs (it needs that much on hand).
        parry_cost: f32 = 15.0, 0.0, 1000.0, Amount;
        /// Shield energy a perfect parry gives back (capped per raise at what the raise cost).
        parry_refund: f32 = 8.0, 0.0, 1000.0, Amount;
        /// Half-angle of the arc either side of the nose.
        parry_half_arc: f32 = 1.0, 0.0, std::f32::consts::PI, Radians;
        /// Reach of the arc from the ship's center.
        parry_radius: f32 = 90.0, 0.0, 2000.0, Distance;
        /// A reflected shot's damage as a multiple of what it carried, at level 1.
        parry_reflect: f32 = 1.5, 0.0, 20.0, Multiplier;
        /// Reflect multiple gained per level after the first.
        parry_reflect_step: f32 = 0.25, 0.0, 10.0, Multiplier;
        /// Seconds the perfect window widens per level.
        parry_perfect_step: f32 = 0.02, 0.0, 1.0, Seconds;
        /// Seconds a perfect parry takes off the cooldown.
        parry_perfect_cooldown_refund: f32 = 0.6, 0.0, 60.0, Seconds;
        /// Seconds the whole simulation freezes on a perfect parry (hit-stop).
        parry_hitstop: f32 = 0.06, 0.0, 1.0, Seconds;
        /// Seconds the perfect-parry flash lasts.
        parry_flash: f32 = 0.3, 0.0, 5.0, Seconds;
        /// A reflected shot is re-aimed at a target within this distance.
        parry_aim_range: f32 = 1100.0, 0.0, 100_000.0, Distance;
        /// The re-aimed target must lie within this many radians of the mirrored heading.
        parry_aim_cone: f32 = 1.1, 0.0, std::f32::consts::PI, Radians;
        /// Level at which a reflected shot seeks.
        parry_reflect_homing: u8 = 2, 0.0, 10.0, Count;
        /// PARRY unlock price in metal.
        price_parry_metal: f32 = 120.0, 0.0, 100_000.0, Amount;
        /// PARRY unlock price in crystal.
        price_parry_crystal: f32 = 40.0, 0.0, 100_000.0, Amount;
        /// PARRY unlock price in volatiles.
        price_parry_volatiles: f32 = 40.0, 0.0, 100_000.0, Amount;
    }

    // ---- dash (a locked upgrade: level 0 is not owned) -------------------------------------------
    group "dash" {
        /// How far a dash jumps at level 1.
        dash_distance: f32 = 240.0, 0.0, 5000.0, Distance;
        /// Distance gained per level after the first.
        dash_distance_step: f32 = 30.0, 0.0, 2000.0, Distance;
        /// A dash that could cover less than this (hard against something solid) is refused free.
        dash_min: f32 = 24.0, 0.0, 2000.0, Distance;
        /// Seconds before the next dash, at level 1.
        dash_cooldown: f32 = 1.2, 0.0, 60.0, Seconds;
        /// Seconds off the cooldown per level after the first.
        dash_cooldown_step: f32 = 0.15, 0.0, 10.0, Seconds;
        /// Shield energy one dash costs.
        dash_cost: f32 = 8.0, 0.0, 1000.0, Amount;
        /// Seconds of invulnerability a dash grants.
        dash_invuln: f32 = 0.3, 0.0, 10.0, Seconds;
        /// Seconds a dash trail stays drawn.
        dash_trail: f32 = 0.35, 0.0, 10.0, Seconds;
        /// Seconds a graze's damage boost lasts (each new graze refreshes it).
        dash_boost_time: f32 = 4.0, 0.0, 60.0, Seconds;
        /// Damage gained per stack of the graze boost.
        dash_boost_step: f32 = 0.25, 0.0, 5.0, Multiplier;
        /// Most stacks of the graze boost.
        dash_boost_stacks: u8 = 3, 0.0, 20.0, Count;
        /// Shield a graze gives back (less than a dash costs, so dashing is never free profit).
        dash_graze_refund: f32 = 5.0, 0.0, 1000.0, Amount;
        /// Seconds a creature the ship dashes through can neither strike, fling nor shoot.
        dash_stagger: f32 = 0.8, 0.0, 30.0, Seconds;
        /// Share of its speed a staggered creature keeps.
        dash_stagger_damp: f32 = 0.3, 0.0, 1.0, Ratio;
        /// Reach beyond the two radii that counts as passing through.
        dash_graze_margin: f32 = 6.0, 0.0, 500.0, Distance;
        /// DASH unlock price in metal.
        price_dash_metal: f32 = 100.0, 0.0, 100_000.0, Amount;
        /// DASH unlock price in crystal.
        price_dash_crystal: f32 = 30.0, 0.0, 100_000.0, Amount;
        /// DASH unlock price in volatiles.
        price_dash_volatiles: f32 = 30.0, 0.0, 100_000.0, Amount;
    }

    // ---- sonar upgrades (bench SONAR tab; the base ping is unchanged at level 0) -----------------
    group "sonar" {
        /// Extra ping reach per level.
        ping_reach_step: f32 = 4_000.0, 0.0, 200_000.0, Distance;
        /// Extra ping ring speed per level.
        ping_speed_step: f32 = 1_500.0, 0.0, 100_000.0, Speed;
        /// Seconds off the ping cooldown per level.
        ping_cooldown_step: f32 = 0.8, 0.0, 30.0, Seconds;
        /// The ping cooldown never drops below this.
        ping_cooldown_floor: f32 = 1.5, 0.05, 60.0, Seconds;
        /// Extra echoes of every kind per level.
        ping_targets_step: usize = 1, 0.0, 20.0, Count;
        /// Pad-alert echoes one ping may return once bought.
        cap_pad_alert: usize = 3, 0.0, 50.0, Count;
        /// Lode echoes one ping may return once bought.
        cap_lode: usize = 3, 0.0, 50.0, Count;
        /// Nest echoes one ping may return once bought.
        cap_nest: usize = 2, 0.0, 50.0, Count;
        /// Egg echoes one ping may return once bought.
        cap_eggs: usize = 2, 0.0, 50.0, Count;
        /// Predator echoes one ping may return once bought.
        cap_predators: usize = 3, 0.0, 50.0, Count;
        /// A free rock holding at least this much ore counts as a rich lode to the sonar.
        lode_min_ore: f32 = 60.0, 0.0, 10_000.0, Amount;
        /// Ping reach first-level price in metal.
        price_ping_reach_metal: f32 = 20.0, 0.0, 100_000.0, Amount;
        /// Ping reach first-level price in crystal.
        price_ping_reach_crystal: f32 = 6.0, 0.0, 100_000.0, Amount;
        /// Ping speed first-level price in volatiles.
        price_ping_speed_volatiles: f32 = 15.0, 0.0, 100_000.0, Amount;
        /// Ping speed first-level price in crystal.
        price_ping_speed_crystal: f32 = 4.0, 0.0, 100_000.0, Amount;
        /// Ping cooldown first-level price in volatiles.
        price_ping_cooldown_volatiles: f32 = 20.0, 0.0, 100_000.0, Amount;
        /// Ping cooldown first-level price in crystal.
        price_ping_cooldown_crystal: f32 = 6.0, 0.0, 100_000.0, Amount;
        /// Ping targets first-level price in metal.
        price_ping_targets_metal: f32 = 20.0, 0.0, 100_000.0, Amount;
        /// Ping targets first-level price in volatiles.
        price_ping_targets_volatiles: f32 = 15.0, 0.0, 100_000.0, Amount;
        /// Pad echo tier price in metal.
        price_echo_pads_metal: f32 = 25.0, 0.0, 100_000.0, Amount;
        /// Pad echo tier price in crystal.
        price_echo_pads_crystal: f32 = 5.0, 0.0, 100_000.0, Amount;
        /// Lode echo tier price in metal.
        price_echo_lodes_metal: f32 = 30.0, 0.0, 100_000.0, Amount;
        /// Lode echo tier price in crystal.
        price_echo_lodes_crystal: f32 = 10.0, 0.0, 100_000.0, Amount;
        /// Nest echo tier price in volatiles.
        price_echo_nests_volatiles: f32 = 40.0, 0.0, 100_000.0, Amount;
        /// Nest echo tier price in crystal.
        price_echo_nests_crystal: f32 = 15.0, 0.0, 100_000.0, Amount;
        /// Predator echo tier price in metal.
        price_echo_predators_metal: f32 = 40.0, 0.0, 100_000.0, Amount;
        /// Predator echo tier price in volatiles.
        price_echo_predators_volatiles: f32 = 40.0, 0.0, 100_000.0, Amount;
        /// Predator echo tier price in crystal.
        price_echo_predators_crystal: f32 = 20.0, 0.0, 100_000.0, Amount;
    }

    // ---- renewable planetoids --------------------------------------------------------------------
    group "planetoids" {
        /// Share of planetoids that regrow what the beam takes (chosen by a hash of the spawn key).
        renewable_share: f32 = 0.34, 0.0, 1.0, Ratio;
        /// Ore per second a renewable planetoid regrows (a spent one is whole after budget / rate seconds).
        regrow_rate: f32 = 0.5, 0.0, 1000.0, Rate;
    }

    // ---- chart, beacons and fast travel ----------------------------------------------------------
    group "travel" {
        /// Most pins the player may keep on the chart.
        max_pins: usize = 24, 0.0, 200.0, Count;
        /// Beacons the rig may have standing at once per upgrade level.
        beacons_per_level: usize = 1, 0.0, 10.0, Count;
        /// Beacon first-level price in metal.
        price_beacon_metal: f32 = 60.0, 0.0, 100_000.0, Amount;
        /// Beacon first-level price in crystal.
        price_beacon_crystal: f32 = 20.0, 0.0, 100_000.0, Amount;
        /// Beacon first-level price in volatiles.
        price_beacon_volatiles: f32 = 20.0, 0.0, 100_000.0, Amount;
        /// Volatiles a jump costs before the distance term.
        travel_volatiles_base: f32 = 8.0, 0.0, 1000.0, Amount;
        /// Volatiles per sector of distance (rounded up).
        travel_volatiles_per_sector: f32 = 3.0, 0.0, 1000.0, Amount;
        /// Crystal a jump costs before the distance term.
        travel_crystal_base: f32 = 2.0, 0.0, 1000.0, Amount;
        /// Crystal per sector of distance (rounded up).
        travel_crystal_per_sector: f32 = 0.75, 0.0, 1000.0, Amount;
        /// A jump is refused beyond this many sectors.
        travel_max_sectors: f32 = 40.0, 0.0, 10_000.0, Count;
        /// Seconds of charge-up before the distance term.
        travel_charge_base: f32 = 4.0, 0.0, 300.0, Seconds;
        /// Seconds of charge-up per sector.
        travel_charge_per_sector: f32 = 0.4, 0.0, 100.0, Seconds;
        /// The charge-up never exceeds this.
        travel_charge_max: f32 = 14.0, 0.0, 600.0, Seconds;
        /// Share of the charge-up each beacon level after the first cuts.
        travel_charge_level_cut: f32 = 0.08, 0.0, 0.3, Ratio;
        /// Seconds of cooldown after a completed jump.
        travel_cooldown: f32 = 180.0, 0.0, 7200.0, Seconds;
        /// Seconds of cooldown after a broken charge.
        travel_cancel_cooldown: f32 = 8.0, 0.0, 600.0, Seconds;
        /// Share of the cost refunded when damage breaks the charge.
        travel_cancel_refund: f32 = 0.5, 0.0, 1.0, Ratio;
        /// Seconds the ship is exposed on arrival (shield held at zero, no protection).
        travel_exposed: f32 = 3.0, 0.0, 120.0, Seconds;
    }

    // ---- legacy and wrecks -----------------------------------------------------------------------
    group "legacy" {
        /// Share of each material mined that insurance carries into the next run.
        legacy_fraction: f32 = 0.25, 0.0, 1.0, Ratio;
        /// Most of one material insurance carries.
        legacy_cap: f32 = 120.0, 0.0, 10_000.0, Amount;
        /// Share carried without insurance.
        legacy_fraction_bare: f32 = 0.10, 0.0, 1.0, Ratio;
        /// Most of one material carried without insurance.
        legacy_cap_bare: f32 = 40.0, 0.0, 10_000.0, Amount;
        /// Highest weapon level insurance carries.
        legacy_weapon_level_cap: u8 = 2, 0.0, 10.0, Count;
        /// Seconds of a new run the HUD shows what the legacy brought.
        legacy_hud_seconds: f32 = 25.0, 0.0, 600.0, Seconds;
        /// Most of each material a lost ship's wreck holds.
        wreck_cap: f32 = 150.0, 0.0, 10_000.0, Amount;
        /// Flying within this distance recovers a wreck.
        wreck_radius: f32 = 140.0, 0.0, 5000.0, Distance;
        /// Wrecks that wait at once; the oldest is lost.
        max_wrecks: usize = 3, 1.0, 50.0, Count;
        /// Seconds of play before a wreck in a living civilization's territory is looted.
        loot_after: f32 = 240.0, 0.0, 36_000.0, Seconds;
        /// Extra seconds before looting, a deterministic share chosen by a hash.
        loot_jitter: f32 = 240.0, 0.0, 36_000.0, Seconds;
    }

    // ---- regions ---------------------------------------------------------------------------------
    group "regions" {
        /// Seconds a new region must hold the ship before it is announced.
        region_hold: f32 = 3.0, 0.0, 120.0, Seconds;
        /// Least seconds between two region banners.
        region_cooldown: f32 = 12.0, 0.0, 600.0, Seconds;
        /// Quiet seconds (no damage, thrust, fire or beam) before auto repair starts.
        auto_repair_delay: f32 = 3.0, 0.0, 120.0, Seconds;
        /// The shield only mends by auto repair when it is below this share.
        auto_shield_below: f32 = 0.5, 0.0, 1.0, Ratio;
        /// Auto repair never takes the last of the volatiles (they are fuel).
        auto_fuel_reserve: f32 = 25.0, 0.0, 1000.0, Amount;
    }

    // ---- diplomacy -------------------------------------------------------------------------------
    group "diplomacy" {
        /// Lowest regard a civilization can hold of the ship.
        regard_min: f32 = -100.0, -10_000.0, -1.0, Regard;
        /// Highest regard a civilization can hold of the ship.
        regard_max: f32 = 100.0, 1.0, 10_000.0, Regard;
        /// Hostile at or below this regard.
        hostile_at: f32 = -40.0, -10_000.0, 10_000.0, Regard;
        /// Wary at or below this regard.
        wary_at: f32 = -12.0, -10_000.0, 10_000.0, Regard;
        /// Friendly at or above this regard.
        friendly_at: f32 = 40.0, -10_000.0, 10_000.0, Regard;
        /// Rising across a tier line needs this much more, so a border does not flicker its banner.
        tier_hysteresis: f32 = 3.0, 0.0, 100.0, Regard;
        /// Where an ordinary civilization starts.
        regard_start: f32 = 0.0, -10_000.0, 10_000.0, Regard;
        /// Where the early outpost starts (kinder).
        regard_start_outpost: f32 = 15.0, -10_000.0, 10_000.0, Regard;
        /// How far leaving an ordinary civilization alone can raise it.
        rest_cap: f32 = 25.0, -10_000.0, 10_000.0, Regard;
        /// How far leaving the outpost alone can raise it.
        rest_cap_outpost: f32 = 60.0, -10_000.0, 10_000.0, Regard;
        /// Regard recovered per second while the ship lingers inside a claim.
        rest_rate: f32 = 0.1, 0.0, 100.0, Rate;
        /// Seconds since the last offence before resting recovers regard.
        rest_delay: f32 = 20.0, 0.0, 3600.0, Seconds;
        /// Old grudges fade at this regard per second away from the claim, never past the start.
        away_rate: f32 = 0.01, 0.0, 100.0, Rate;
        /// Regard lost per point of damage dealt to a member.
        hurt_member: f32 = 0.08, 0.0, 100.0, Multiplier;
        /// Regard lost per point of damage dealt to a structure.
        hurt_structure: f32 = 0.02, 0.0, 100.0, Multiplier;
        /// Regard lost for killing a member.
        kill_member: f32 = 5.0, 0.0, 1000.0, Regard;
        /// Regard lost for killing a warrior.
        kill_warrior: f32 = 7.0, 0.0, 1000.0, Regard;
        /// Regard lost for killing an elder.
        kill_elder: f32 = 30.0, 0.0, 1000.0, Regard;
        /// Regard lost for destroying a turret.
        kill_turret: f32 = 8.0, 0.0, 1000.0, Regard;
        /// Regard lost for destroying a wall piece (it barely counts).
        kill_wall: f32 = 0.4, 0.0, 1000.0, Regard;
        /// Regard lost for destroying an outpost base.
        kill_outpost_base: f32 = 30.0, 0.0, 1000.0, Regard;
        /// Regard lost for destroying a capital.
        kill_capital: f32 = 45.0, 0.0, 1000.0, Regard;
        /// A kill counts against the ship if it struck the victim within this many seconds.
        kill_window: f32 = 8.0, 0.0, 300.0, Seconds;
        /// Regard lost per unit of ore the beam takes inside a claim.
        mine_cost: f32 = 0.12, 0.0, 100.0, Multiplier;
        /// Share of `mine_cost` a friend minds.
        mine_cost_friend: f32 = 0.25, 0.0, 1.0, Ratio;
        /// Seconds between the warnings the beam earns.
        mine_warn_every: f32 = 15.0, 0.5, 600.0, Seconds;
        /// Fly within this distance of a seat (past its hull) to tithe.
        tithe_range: f32 = 420.0, 0.0, 10_000.0, Distance;
        /// Amount of the material the hold has most of that a tithe takes.
        tithe_amount: f32 = 20.0, 0.0, 1000.0, Amount;
        /// Regard a tithe raises.
        tithe_gain: f32 = 9.0, 0.0, 1000.0, Regard;
        /// Seconds before a tithe may be repeated.
        tithe_cooldown: f32 = 2.0, 0.0, 600.0, Seconds;
        /// Regard a friendly trade raises.
        trade_gain: f32 = 1.5, 0.0, 1000.0, Regard;
        /// Share of a tithe's amount a legacy-fallback trade gives back in the scarcest material.
        trade_rate: f32 = 0.75, 0.0, 10.0, Multiplier;
        /// Hull share below which a friend ranks repair first.
        trade_repair_below: f32 = 0.75, 0.0, 1.0, Ratio;
        /// How far a friendly civilization's shared charts reach past its claim, in sectors.
        share_margin: i32 = 1, 0.0, 20.0, Count;
        /// Doctrine learning pace multiple against a hostile civilization.
        doctrine_pull_hostile: f32 = 1.5, 0.0, 20.0, Multiplier;
        /// Doctrine learning pace multiple against a wary civilization.
        doctrine_pull_wary: f32 = 1.0, 0.0, 20.0, Multiplier;
        /// Doctrine learning pace multiple against a civilization that ignores the ship.
        doctrine_pull_ignores: f32 = 0.5, 0.0, 20.0, Multiplier;
        /// Doctrine learning pace multiple against a friendly civilization.
        doctrine_pull_friendly: f32 = 0.25, 0.0, 20.0, Multiplier;
    }

    // ---- apex elders -----------------------------------------------------------------------------
    group "apex" {
        /// The banner "APEX: NAME stirs" posts once when an apex first comes within this of the ship.
        apex_notice_range: f32 = 3600.0, 0.0, 100_000.0, Distance;
        /// The HUD line and edge arrow follow the nearest apex this far.
        apex_hud_range: f32 = 9000.0, 0.0, 200_000.0, Distance;
        /// Score for slaying one (times the place's threat), halved for a lesser one.
        apex_score: f32 = 2500.0, 0.0, 10_000_000.0, Amount;
        /// Of each material a slain apex drops (halved for a lesser one).
        apex_material: f32 = 90.0, 0.0, 10_000.0, Amount;
    }

    // ---- run summary titles ----------------------------------------------------------------------
    group "titles" {
        /// Extirpations that make a Scourge.
        title_scourge_extirpations: usize = 2, 0.0, 10_000.0, Count;
        /// Material mined that makes a Prospector.
        title_prospect_mined: f32 = 150.0, 0.0, 1_000_000.0, Amount;
        /// Tithes that make a Tither.
        title_tithes: u32 = 5, 0.0, 10_000.0, Count;
        /// Parries that make a Parrier.
        title_parries: u32 = 10, 0.0, 10_000.0, Count;
        /// Dashes that make a Dasher.
        title_dashes: u32 = 40, 0.0, 10_000.0, Count;
        /// Sectors explored that make a Cartographer.
        title_cartographer_sectors: usize = 12, 0.0, 10_000.0, Count;
        /// Regions seen that make a Cartographer.
        title_cartographer_regions: usize = 3, 0.0, 10_000.0, Count;
        /// A gentle cartographer destroyed at most one creature per this many sectors explored.
        title_gentle_kills_per_sector: u32 = 3, 0.0, 10_000.0, Count;
        /// Deaths that make a Reckless.
        title_reckless_deaths: u32 = 3, 0.0, 10_000.0, Count;
        /// A Reckless survived at most this long.
        title_reckless_seconds: f32 = 360.0, 0.0, 100_000.0, Seconds;
        /// A Hermit mined at least this much.
        title_hermit_mined: f32 = 300.0, 0.0, 1_000_000.0, Amount;
        /// A Hermit destroyed at most this many creatures.
        title_hermit_kills: u32 = 10, 0.0, 10_000.0, Count;
    }

    // ---- wildlife and civilizations --------------------------------------------------------------
    group "wildlife" {
        /// Seconds between re-reading who is near whom.
        fauna_period: f32 = 0.25, 0.05, 10.0, Seconds;
        /// Wildlife nearer than this to a civil body it is hostile to sets upon it.
        hostile_reach: f32 = 1100.0, 0.0, 50_000.0, Distance;
        /// A friendly species within this of a settlement drifts toward it.
        friend_range: f32 = 1600.0, 0.0, 50_000.0, Distance;
        /// Distance at which a drifting friendly species holds around a settlement.
        herd_ring: f32 = 480.0, 0.0, 50_000.0, Distance;
        /// Pull toward the settlement as a multiple of cruise speed.
        herd_pull: f32 = 1.8, 0.0, 20.0, Multiplier;
        /// Most wildlife on one civil body at once.
        max_attackers: usize = 3, 0.0, 100.0, Count;
        /// Most wildlife on one civilization at once.
        max_assault: usize = 8, 0.0, 100.0, Count;
        /// Most wildlife on one peaceful settlement at once.
        max_assault_peaceful: usize = 3, 0.0, 100.0, Count;
        /// Flat damage of a bite.
        fauna_bite: f32 = 5.0, 0.0, 1000.0, Amount;
        /// Bite damage per point of the genome's contact damage.
        fauna_bite_per_contact: f32 = 0.5, 0.0, 100.0, Multiplier;
        /// Seconds between bites.
        fauna_bite_period: f32 = 0.9, 0.05, 60.0, Seconds;
        /// Bite reach from the other's rim.
        fauna_bite_reach: f32 = 45.0, 0.0, 2000.0, Distance;
        /// Share of a bite a structure takes.
        fauna_structure_scale: f32 = 0.35, 0.0, 1.0, Ratio;
        /// Further share of a bite a settlement's people and works take.
        fauna_peaceful_scale: f32 = 0.4, 0.0, 1.0, Ratio;
        /// Wildlife never destroys a structure or elder: they bottom out at this share of their hull.
        fauna_structure_floor: f32 = 0.4, 0.0, 1.0, Ratio;
        /// Pace of a hostile on the attack, of its full speed.
        attack_pace: f32 = 0.9, 0.0, 2.0, Ratio;
        /// Pace of a defender on the hunt, of its full speed.
        defend_pace: f32 = 0.8, 0.0, 2.0, Ratio;
        /// Idle civilization people hunt hostile wildlife within this of themselves.
        defend_range: f32 = 1000.0, 0.0, 50_000.0, Distance;
        /// ...and within this of their post.
        defend_leash: f32 = 1500.0, 0.0, 50_000.0, Distance;
        /// An armed defender strikes from this share of its weapon range.
        defend_reach_share: f32 = 0.6, 0.0, 1.0, Ratio;
        /// Damage of a defender's strike (times its threat).
        strike_damage: f32 = 9.0, 0.0, 1000.0, Amount;
        /// Least seconds between a defender's strikes.
        strike_period_min: f32 = 0.8, 0.05, 60.0, Seconds;
        /// A fortress turret reaches this far.
        turret_defend_range: f32 = 950.0, 0.0, 50_000.0, Distance;
        /// Damage of a fortress turret's strike.
        turret_strike: f32 = 16.0, 0.0, 1000.0, Amount;
        /// Regard cost of killing a species the civilization is friendly to (times its affinity).
        friend_kill_cost: f32 = 3.0, 0.0, 1000.0, Regard;
        /// Regard earned for killing a hostile species (times how much it is hated).
        hostile_kill_gain: f32 = 1.5, 0.0, 1000.0, Regard;
        /// Each earlier kill in the window multiplies the gain by this.
        gain_diminish: f32 = 0.7, 0.0, 1.0, Ratio;
        /// Most regard per civilization per window, so it cannot be farmed.
        gain_cap: f32 = 6.0, 0.0, 1000.0, Regard;
        /// Seconds of the farming window.
        gain_window: f32 = 300.0, 1.0, 36_000.0, Seconds;
        /// Least seconds between kill banners.
        kill_banner_every: f32 = 4.0, 0.0, 600.0, Seconds;
        /// The HUD names up to this many species near the ship.
        tag_count: usize = 3, 0.0, 20.0, Count;
        /// ...within this of the ship.
        tag_range: f32 = 2600.0, 0.0, 100_000.0, Distance;
    }

    // ---- drops -----------------------------------------------------------------------------------
    group "drops" {
        /// A creature's drop chance is multiplied by this plus hardness slope times (hull + shield) / reference.
        hard_floor: f32 = 0.7, 0.0, 10.0, Multiplier;
        /// Hardness gained per reference pool of hull plus shield.
        hard_slope: f32 = 1.0, 0.0, 10.0, Multiplier;
        /// The hull plus shield pool the hardness slope is measured against.
        hard_ref: f32 = 100.0, 1.0, 100_000.0, Amount;
        /// Hardness never exceeds this.
        hard_cap: f32 = 3.5, 0.0, 100.0, Multiplier;
        /// No creature drops more often than this.
        drop_chance_cap: f32 = 0.6, 0.0, 1.0, Ratio;
        /// Parts a capital pays before the fortress tier bonus.
        capital_parts: u32 = 2, 0.0, 20.0, Count;
        /// Fortress tier at which a capital's first part is Epic.
        capital_epic_tier: u8 = 3, 0.0, 10.0, Count;
        /// Rolls an outpost seat pays.
        outpost_rolls: u32 = 1, 0.0, 20.0, Count;
        /// Fortress tier at which an outpost pays one more of each.
        outpost_bonus_tier: u8 = 2, 0.0, 10.0, Count;
        /// Fortress tier at which an elder pays one more part.
        elder_bonus_tier: u8 = 2, 0.0, 10.0, Count;
        /// Threat at or below which a jointed body has one whole-creature health pool (about ring 3).
        pool_full_threat: f32 = 1.9, 0.0, 100.0, Amount, Regen;
        /// Threat from which each part is its own life again (about ring 9).
        pool_none_threat: f32 = 3.7, 0.0, 100.0, Amount, Regen;
        /// At full pool each part beyond the head counts only this share of its hull toward the pool.
        pool_floor: f32 = 0.15, 0.0, 1.0, Ratio;
        /// Seconds pieces shed as a pooled body weakens drift before they vanish.
        pool_drift: f32 = 2.5, 0.1, 60.0, Seconds;
        /// Top speed of a shed piece.
        pool_fling: f32 = 70.0, 0.0, 5000.0, Speed;
        /// Sustained hostile creature overlap applies one sting per this many seconds.
        drone_contact_seconds: f32 = 0.65, 0.05, 60.0, Seconds;
    }
}

/// The default tunables as a constant, for const contexts and tests (`DEFAULT.adapt_max`).
pub const DEFAULT: Tunables = Tunables::DEFAULT;

/// The cross-field rules every set of tunables must satisfy, defaults included. `set` refuses a
/// change that would break one, so the resolved struct is always coherent.
fn validate(t: &Tunables) -> Result<(), String> {
    macro_rules! rule {
        ($cond:expr, $($msg:tt)+) => {
            let holds: bool = $cond;
            if !holds {
                return Err(format!($($msg)+));
            }
        };
    }
    rule!(
        t.adapt_max < 1.0,
        "adapt_max {} must stay under 1 so a hit always lands for something",
        t.adapt_max
    );
    rule!(
        t.regard_min < t.hostile_at
            && t.hostile_at < t.wary_at
            && t.wary_at < t.friendly_at
            && t.friendly_at < t.regard_max,
        "regard tiers must stay ordered: min {} < hostile {} < wary {} < friendly {} < max {}",
        t.regard_min,
        t.hostile_at,
        t.wary_at,
        t.friendly_at,
        t.regard_max
    );
    rule!(
        (t.regard_min..=t.regard_max).contains(&t.regard_start)
            && (t.regard_min..=t.regard_max).contains(&t.regard_start_outpost),
        "regard_start and regard_start_outpost must lie within regard_min..regard_max"
    );
    rule!(
        t.rest_cap >= t.regard_start && t.rest_cap_outpost >= t.regard_start_outpost,
        "a rest cap may not sit below where its civilization starts"
    );
    rule!(
        t.rest_cap <= t.regard_max && t.rest_cap_outpost <= t.regard_max,
        "a rest cap may not exceed regard_max"
    );
    rule!(
        t.impact_min_speed < t.impact_speed_cap,
        "impact_min_speed {} must stay under impact_speed_cap {}",
        t.impact_min_speed,
        t.impact_speed_cap
    );
    rule!(
        t.parry_perfect <= t.parry_window,
        "the perfect parry window {} cannot exceed the window {}",
        t.parry_perfect,
        t.parry_window
    );
    rule!(
        t.parry_refund <= t.parry_cost,
        "a perfect parry's refund {} cannot exceed its cost {} (it is at best free)",
        t.parry_refund,
        t.parry_cost
    );
    rule!(
        t.dash_graze_refund < t.dash_cost,
        "a graze's refund {} must stay under a dash's cost {} (dashing is never free profit)",
        t.dash_graze_refund,
        t.dash_cost
    );
    let top = f32::from(SKILL_MAX - 1);
    rule!(
        t.dash_cooldown - t.dash_cooldown_step * top > 0.0,
        "dash cooldown must stay positive at the top level"
    );
    rule!(
        t.parry_cooldown - t.parry_cooldown_step * top > 0.0,
        "parry cooldown must stay positive at the top level"
    );
    rule!(
        t.travel_charge_level_cut * top < 1.0,
        "the travel charge cut per level must leave some charge at the top level"
    );
    rule!(
        t.travel_charge_base <= t.travel_charge_max,
        "travel_charge_base {} cannot exceed travel_charge_max {}",
        t.travel_charge_base,
        t.travel_charge_max
    );
    rule!(
        t.barrage_every_enraged <= t.barrage_every_calm
            && t.barrage_shots_calm <= t.barrage_shots_enraged,
        "an enraged apex barrages at least as often and at least as hard as a calm one"
    );
    rule!(
        t.pool_full_threat < t.pool_none_threat,
        "pool_full_threat {} must stay under pool_none_threat {}",
        t.pool_full_threat,
        t.pool_none_threat
    );
    rule!(
        t.hard_floor <= t.hard_cap,
        "hard_floor {} cannot exceed hard_cap {}",
        t.hard_floor,
        t.hard_cap
    );
    rule!(
        t.legacy_fraction_bare <= t.legacy_fraction && t.legacy_cap_bare <= t.legacy_cap,
        "bare legacy may not exceed insured legacy"
    );
    rule!(
        t.max_assault_peaceful <= t.max_assault,
        "max_assault_peaceful {} cannot exceed max_assault {}",
        t.max_assault_peaceful,
        t.max_assault
    );
    rule!(
        t.food_hunt_hunger <= t.food_graze_hunger && t.food_graze_hunger <= t.food_full,
        "hunger lines must stay ordered: hunt {} <= graze {} <= full {}",
        t.food_hunt_hunger,
        t.food_graze_hunger,
        t.food_full
    );
    rule!(
        t.growth_grow_stalled < t.growth_grow_full,
        "growth_grow_stalled {} must stay under growth_grow_full {}",
        t.growth_grow_stalled,
        t.growth_grow_full
    );
    rule!(
        t.growth_lineage_cap <= t.growth_lineage_world_cap,
        "growth_lineage_cap {} cannot exceed growth_lineage_world_cap {}",
        t.growth_lineage_cap,
        t.growth_lineage_world_cap
    );
    rule!(
        t.flock_near < t.flock_mid,
        "flock_near {} must stay under flock_mid {}",
        t.flock_near,
        t.flock_mid
    );
    rule!(
        t.creature_school_loose_radius <= t.creature_loose_radius,
        "a true school ({}) holds no looser than a crowd ({})",
        t.creature_school_loose_radius,
        t.creature_loose_radius
    );
    rule!(
        t.elder_charge_range_min <= t.elder_charge_range_max,
        "elder_charge_range_min {} cannot exceed elder_charge_range_max {}",
        t.elder_charge_range_min,
        t.elder_charge_range_max
    );
    rule!(
        t.elder_charge_every_enraged <= t.elder_charge_every_calm
            && t.elder_escort_every_enraged <= t.elder_escort_every_calm
            && t.elder_pull_every_enraged <= t.elder_pull_every_calm
            && t.elder_escort_cap_calm <= t.elder_escort_cap_enraged,
        "an enraged elder acts at least as often as a calm one"
    );
    rule!(
        t.civ_war_at <= t.civ_raid_at,
        "civ_war_at {} (first war party) cannot come after civ_raid_at {} (first big raid)",
        t.civ_war_at,
        t.civ_raid_at
    );
    rule!(
        t.farm_sprout <= t.farm_ripe
            && t.farm_stump <= t.farm_ripe
            && t.farm_graze_floor <= t.farm_ripe,
        "plant growth marks (sprout, stump, graze floor) must not exceed the ripe mark"
    );
    rule!(
        t.farm_ripe_seed_cum_none <= t.farm_ripe_seed_cum_one,
        "the seed table's cumulative marks must stay ordered: none {} <= one {}",
        t.farm_ripe_seed_cum_none,
        t.farm_ripe_seed_cum_one
    );
    rule!(
        t.farm_field_crops_min <= t.farm_field_crops_max,
        "farm_field_crops_min {} cannot exceed farm_field_crops_max {}",
        t.farm_field_crops_min,
        t.farm_field_crops_max
    );
    rule!(
        t.pad_reload_raid_damage_min <= t.pad_reload_raid_damage_max,
        "pad_reload_raid_damage_min {} cannot exceed pad_reload_raid_damage_max {}",
        t.pad_reload_raid_damage_min,
        t.pad_reload_raid_damage_max
    );
    rule!(
        t.world_fling_speed <= t.world_fling_max_speed
            && t.world_fling_max_speed <= t.world_fling_hard_cap,
        "fling speeds must stay ordered: min {} <= max {} <= hard cap {}",
        t.world_fling_speed,
        t.world_fling_max_speed,
        t.world_fling_hard_cap
    );
    super::tuning_gen::validate(t)
}

// ---- structural constants (not in the registry) ---------------------------------------------------
//
// These stay consts: they size or identify things rather than tune them, or they are read by a
// value type with no access to the game (the next slices thread them). Everything else numeric in
// this module is a registry entry above.

/// Stream salt of the roll that decides whether an ability fizzles (see `realm::Effects`). An
/// identity, never a tunable.
pub const FIZZLE_SALT: u64 = 0xF122_1E00_0000_0071;
/// Levels every mining upgrade can reach. Sizes level-indexed tables and is used in pattern and
/// const positions; the level scaling rules in `validate` are written against it.
pub const SKILL_MAX: u8 = 4;
/// An organ is an owned strain, level 1 to `ORGAN_LEVELS` (a level indexes `organ_level_gain_*`).
pub const ORGAN_LEVELS: u8 = 3;
/// SYMBIOSIS opens up to this many organ slots (a fixed-size layout).
pub const SYMBIOSIS_SLOTS: usize = 3;
/// How much of each material the hold carries before cargo upgrades. Read by `Cargo::cap`, a
/// value type used in about sixty places without game access; becomes a registry entry with the
/// other hold caps (fuel 120, water 30) when `Cargo` is threaded.
pub const CAP: f32 = 200.0;
/// Ore a planetoid will give before it is spent (it never shrinks). Read by `Body::ore` through
/// the pure `ore_for`, used widely without game access; deferred with `CAP`.
pub const PLANETOID_BUDGET: f32 = 400.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tunables::{Effect, Kind, Registry, TuneError};

    #[test]
    fn every_entry_is_well_formed() {
        let mut seen = std::collections::HashSet::new();
        assert_eq!(TUNABLES.len(), Tunables::COUNT);
        for info in TUNABLES {
            assert!(seen.insert(info.name), "duplicate name {}", info.name);
            assert!(
                !info.name.is_empty()
                    && info
                        .name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    && !info.name.starts_with('_')
                    && !info.name.ends_with('_')
                    && !info.name.starts_with(|c: char| c.is_ascii_digit()),
                "{} is not snake_case",
                info.name
            );
            assert!(!info.group.is_empty(), "{} has no group", info.name);
            assert!(info.doc.len() > 8, "{} has no doc", info.name);
            assert!(
                info.doc.ends_with('.'),
                "{} doc is not a sentence",
                info.name
            );
            assert!(
                !info.doc.contains('\u{2014}'),
                "{} doc has an em dash",
                info.name
            );
            for v in [info.default, info.min, info.max] {
                assert!(v.is_finite(), "{} has a non-finite bound", info.name);
            }
            assert!(
                info.min <= info.default && info.default <= info.max,
                "{}: default {} outside {} to {}",
                info.name,
                info.default,
                info.min,
                info.max
            );
            if info.kind == Kind::Int {
                assert_eq!(info.default.fract(), 0.0, "{}", info.name);
                assert_eq!(info.min.fract(), 0.0, "{}", info.name);
                assert_eq!(info.max.fract(), 0.0, "{}", info.name);
            }
            // The struct holds exactly the table's default.
            assert_eq!(DEFAULT.get(info.name), Some(info.default), "{}", info.name);
        }
        assert_eq!(DEFAULT.read_all().len(), TUNABLES.len());
        assert_eq!(Tunables::default(), DEFAULT);
        assert!(validate(&DEFAULT).is_ok());
        assert!(TUNABLES.iter().all(|i| i.effect != Effect::Structural));
        assert!(Tunables::groups().len() >= 15);
    }

    /// The values the game shipped with before the registry (a spot check; the whole table was
    /// compared to the old constants once when it was written).
    #[test]
    fn defaults_are_the_shipped_values() {
        let d = DEFAULT;
        assert_eq!(d.shield_recharge_delay, 2.0);
        assert_eq!(d.npc_shield_rate, 6.0);
        assert_eq!(d.asteroid_speed_floor, 120.0);
        assert_eq!(d.threat_sense_range, 14_000.0);
        assert_eq!(d.falloff_span, 1400.0);
        assert_eq!(d.adapt_max, 0.55);
        assert_eq!(d.adapt_idle_decay, 0.09);
        assert_eq!((d.barrage_every_calm, d.barrage_every_enraged), (8.0, 5.0));
        assert_eq!((d.barrage_shots_calm, d.barrage_shots_enraged), (9, 13));
        assert_eq!(d.rock_hull_factor, 80.0);
        assert_eq!(d.impact_scale, 3.0e-5);
        assert_eq!(d.grip_ref_mass, 18.0);
        assert_eq!(d.plating_all_from, 3);
        assert_eq!(
            (
                d.organ_level_gain_1,
                d.organ_level_gain_2,
                d.organ_level_gain_3
            ),
            (1.0, 1.5, 2.0)
        );
        assert_eq!(d.relic_one_in, 14);
        assert_eq!(d.rate_plain, 4.5);
        assert_eq!(d.price_growth, 1.8);
        assert_eq!((d.price_power_metal, d.price_power_crystal), (30.0, 4.0));
        assert_eq!(d.parry_chance, 0.70);
        assert_eq!(d.parry_cooldown, 1.6);
        assert_eq!(d.parry_cost, 15.0);
        assert_eq!(d.dash_distance, 240.0);
        assert_eq!(d.dash_cost, 8.0);
        assert_eq!(d.ping_reach_step, 4_000.0);
        assert_eq!(d.max_pins, 24);
        assert_eq!(d.travel_cooldown, 180.0);
        assert_eq!(d.legacy_fraction, 0.25);
        assert_eq!(d.max_wrecks, 3);
        assert_eq!(d.regard_min, -100.0);
        assert_eq!(
            (d.hostile_at, d.wary_at, d.friendly_at),
            (-40.0, -12.0, 40.0)
        );
        assert_eq!(d.kill_capital, 45.0);
        assert_eq!(d.tithe_amount, 20.0);
        assert_eq!(d.share_margin, 1);
        assert_eq!(d.doctrine_pull_friendly, 0.25);
        assert_eq!(d.apex_score, 2500.0);
        assert_eq!(d.title_dashes, 40);
        assert_eq!(d.fauna_period, 0.25);
        assert_eq!(d.max_assault, 8);
        assert_eq!(d.hard_cap, 3.5);
        assert_eq!(d.capital_parts, 2);
        assert_eq!((d.pool_full_threat, d.pool_none_threat), (1.9, 3.7));
        assert_eq!(d.drone_contact_seconds, 0.65);
    }

    #[test]
    fn set_never_lets_a_bad_value_into_the_struct() {
        for info in TUNABLES {
            let mut t = DEFAULT;
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                assert_eq!(
                    t.set(info.name, bad),
                    Err(TuneError::NotFinite(info.name)),
                    "{}",
                    info.name
                );
            }
            assert_eq!(t, DEFAULT);
            for wild in [f32::MAX, f32::MIN, 1e30, -1e30, 0.0, -0.0, 1e-30] {
                // Whatever happens (applied, clamped, or a refused rule), the struct stays
                // finite, in range and coherent.
                let _ = t.set(info.name, wild);
                for (i, v) in t.read_all().into_iter().enumerate() {
                    let other = &TUNABLES[i];
                    assert!(v.is_finite(), "{} -> {}", info.name, other.name);
                    assert!(
                        v >= other.min && v <= other.max,
                        "{} -> {} = {v}",
                        info.name,
                        other.name
                    );
                }
                assert!(validate(&t).is_ok(), "{} left a rule broken", info.name);
                t = DEFAULT;
            }
        }
    }

    #[test]
    fn every_entry_round_trips_through_an_overrides_file() {
        for info in TUNABLES {
            let mut t = DEFAULT;
            // Move each entry to a value that is in range and differs from the default.
            let target = if info.default != info.max {
                info.max
                    .min(info.default + (info.max - info.default).max(1.0) * 0.5)
            } else {
                info.min
            };
            let target = if info.kind == Kind::Int {
                target.round()
            } else {
                target
            };
            if t.set(info.name, target).is_err() {
                continue; // a cross-field rule refuses this one alone; covered below
            }
            let (back, report) = Tunables::from_overrides(&t.overrides_to_string());
            assert!(report.ok(), "{}: {report:?}", info.name);
            assert_eq!(back, t, "{}", info.name);
        }
    }

    #[test]
    fn the_cross_field_rules_hold() {
        let refused = |name: &str, value: f32| {
            let mut t = DEFAULT;
            let err = t.set(name, value).unwrap_err();
            assert!(
                matches!(err, TuneError::Invariant { .. }),
                "{name} = {value}: {err:?}"
            );
            assert_eq!(t, DEFAULT, "{name} changed despite the refusal");
        };
        // Resistance never reaches 1 (the range already stops it; the rule backs it up).
        let mut t = DEFAULT;
        t.adapt_max = 1.0;
        assert!(validate(&t).is_err());
        t = DEFAULT;
        t.travel_charge_level_cut = 0.34;
        assert!(validate(&t).is_err());
        // Tier lines stay ordered.
        refused("hostile_at", 0.0);
        refused("wary_at", -50.0);
        refused("friendly_at", -20.0);
        refused("regard_max", 30.0);
        refused("regard_min", -10.0);
        refused("regard_start", 150.0);
        refused("rest_cap", -5.0);
        // Floors below caps.
        refused("impact_min_speed", 1500.0);
        refused("impact_speed_cap", 200.0);
        refused("parry_perfect", 1.0);
        refused("parry_window", 0.05);
        refused("parry_refund", 20.0);
        refused("dash_graze_refund", 8.0);
        refused("dash_cooldown_step", 0.5);
        refused("parry_cooldown_step", 0.6);
        refused("travel_charge_base", 20.0);
        refused("barrage_every_enraged", 9.0);
        refused("barrage_shots_enraged", 5.0);
        refused("pool_full_threat", 4.0);
        refused("hard_floor", 4.0);
        refused("legacy_cap_bare", 200.0);
        refused("legacy_fraction_bare", 0.5);
        refused("max_assault_peaceful", 20.0);
        // Moving the pair in the right order is fine.
        let mut t = DEFAULT;
        t.set("impact_speed_cap", 4000.0).unwrap();
        t.set("impact_min_speed", 2000.0).unwrap();
        assert!(validate(&t).is_ok());
    }

    #[test]
    fn a_file_in_any_order_applies_cleanly() {
        // Raising a floor above the old cap only works once the cap has moved; the file loader
        // retries rule-blocked entries, so the order in the file does not matter.
        let (t, report) = Tunables::from_overrides(
            r#"{ "impact_min_speed": 2000.0, "impact_speed_cap": 4000.0 }"#,
        );
        assert!(report.ok(), "{report:?}");
        assert_eq!((t.impact_min_speed, t.impact_speed_cap), (2000.0, 4000.0));
    }
}
