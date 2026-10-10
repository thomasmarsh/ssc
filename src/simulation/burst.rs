//! The burst budget: how much a creature's guns may land against the reference ship, held at
//! fire time (docs/BALANCE.md section 5.4, slice 1).
//!
//! Every cap is a ratio of the reference pool at the creature's level (`pool_ref`), so it holds
//! at any depth. The cap scales the damage of each shot by `min(1, cap / sum)`: a hundred needles
//! stay a spray but each needle is lighter. It is a cap on a sum, not on genes, so a new weapon
//! is covered automatically. The simulation (`creature::fire_weapons`) and the threat model
//! (`threat::assess_genome`) both call `budget_scale`, so the model describes what is played.

use crate::genome::{Genome, Weapon};
use crate::simulation::tuning::Tunables;
use crate::simulation::upgrades::Stats;
use crate::threat::{SHIP_RADIUS, STAGGER, WINDOW};
use crate::world::Phenotype;

/// The damage the reference ship at level `threat` absorbs: the bare ship's hull plus shield
/// times `threat^balance_ref_exponent` (docs/BALANCE.md 5.1).
/// A share `pith` of the damage skips the shield, as in `threat::Tier::pool`.
pub fn pool_ref(threat: f32, pith: f32, tune: &Tunables) -> f32 {
    (Stats::BASE.max_hull + Stats::BASE.max_shield * (1.0 - pith.clamp(0.0, 1.0)))
        * threat.max(1.0).powf(tune.balance_ref_exponent)
}

/// What one creature (or gun) can throw: the inputs of `budget_scale`.
#[derive(Clone, Copy, Debug)]
pub struct Salvo {
    /// The level of the place, for the reference pool.
    pub threat: f32,
    /// Share of the damage that skips the shield.
    pub pith: f32,
    /// One shot, after sharpness and before the budget.
    pub shot_damage: f32,
    pub shots: u32,
    pub armed: u32,
    pub volleys_in_window: f32,
    /// Share of a volley expected to land at the model's range.
    pub hits: f32,
}

/// The factor on every shot of a volley so that one volley (`shot_damage * shots * armed`, all
/// landing) stays within `balance_volley_cap` of the reference pool and the expected window
/// (`volleys_in_window` volleys, `hits` of them landing) within `balance_window_cap`.
pub fn budget_scale(tune: &Tunables, b: &Salvo) -> f32 {
    let Salvo {
        threat,
        pith,
        shot_damage,
        shots,
        armed,
        volleys_in_window,
        hits,
    } = *b;
    let volley = shot_damage * shots as f32 * armed as f32;
    if volley <= 0.0 {
        return 1.0;
    }
    let pool = pool_ref(threat, pith, tune);
    let by_volley = tune.balance_volley_cap * pool / volley;
    let landed = volley * volleys_in_window.max(1.0) * hits.max(0.0);
    let by_window = if landed > 0.0 {
        tune.balance_window_cap * pool / landed
    } else {
        f32::INFINITY
    };
    by_volley.min(by_window).min(1.0)
}

/// The factor on the shots of a telegraphed barrage of `shots` shots of `each` damage: the
/// whole fan stays within `balance_telegraph_cap` of the reference pool.
pub fn barrage_scale(tune: &Tunables, threat: f32, each: f32, shots: u32) -> f32 {
    // Barrage pellets carry no pith.
    let sum = each * shots as f32;
    if sum <= 0.0 {
        return 1.0;
    }
    (tune.balance_telegraph_cap * pool_ref(threat, 0.0, tune) / sum).min(1.0)
}

/// `budget_scale` for a gun with no genome (a station arm or fortress turret) firing `volley`
/// shots of `weapon` with the given `reach`, at the sharpness `genes` give.
pub fn structure_scale(
    tune: &Tunables,
    genes: &Phenotype,
    weapon: Weapon,
    volley: u8,
    fort: bool,
    reach: f32,
) -> f32 {
    let (base, shots, pace) = weapon_numbers(weapon, volley, tune);
    if shots == 0 {
        return 1.0;
    }
    let period = if fort { tune.fort_period } else { 2.4 } * pace / genes.aggression.max(0.2);
    let volleys = 1.0 + (WINDOW / period.max(0.05)).floor();
    let at = (reach * 0.5).clamp(150.0, 500.0);
    let hits = hit_fraction(weapon, shots, at, SHIP_RADIUS);
    budget_scale(
        tune,
        &Salvo {
            threat: genes.threat,
            pith: 0.0,
            shot_damage: base * genes.sharpness(),
            shots,
            armed: 1,
            volleys_in_window: volleys,
            hits,
        },
    )
}

/// `budget_scale` for a genome born with `genes`, `armed` of whose parts carry the gun, read
/// at half weapon range (the convention of the threat model) and the mean fire stagger.
pub fn genome_scale(g: &Genome, genes: &Phenotype, armed: u32, tune: &Tunables) -> f32 {
    let (base, shots, pace) = weapon_numbers(g.weapon, g.volley, tune);
    if shots == 0 {
        return 1.0;
    }
    let aggression = genes.aggression.max(0.2);
    let rage = if g.rage > 0.0 { 0.4 / 1.15 } else { 1.0 };
    let period = ((g.fire_period + STAGGER) * pace / aggression).max(0.05) * rage;
    let volleys = 1.0 + (WINDOW / period).floor();
    let at = (g.weapon_range * 0.5).clamp(150.0, 500.0);
    let hits = hit_fraction(g.weapon, shots, at, SHIP_RADIUS);
    budget_scale(
        tune,
        &Salvo {
            threat: genes.threat,
            pith: g.bypass_share(),
            shot_damage: base * genes.sharpness(),
            shots,
            armed: armed.max(1),
            volleys_in_window: volleys,
            hits,
        },
    )
}

/// The share of a volley expected to land on a target of `radius` at `distance`.
pub fn hit_fraction(weapon: Weapon, count: u32, distance: f32, radius: f32) -> f32 {
    let n = count.max(1) as f32;
    let d = distance.max(40.0);
    match weapon {
        Weapon::Projectile if count <= 1 => 1.0,
        Weapon::Projectile => {
            let spacing = d * 0.11;
            (1.0 + 2.0 * (radius + 3.0) / spacing).floor().min(n) / n
        }
        // A uniform cone of +/- 0.055 rad.
        Weapon::Needles => ((radius + 1.8) / (d * 0.055)).min(1.0),
        Weapon::Missile => 1.0,
        Weapon::Nova | Weapon::Spiral => {
            (2.0 * (radius + 4.5) / d / std::f32::consts::TAU).min(1.0)
        }
        Weapon::Mine => {
            if d < 250.0 {
                1.0
            } else {
                0.0
            }
        }
        Weapon::None | Weapon::Tether => 0.0,
    }
}

/// Seconds between volleys, the shot damage before sharpness and the shots per volley.
pub fn weapon_numbers(weapon: Weapon, volley: u8, tune: &Tunables) -> (f32, u32, f32) {
    let count = u32::from(volley.max(1));
    match weapon {
        Weapon::Projectile => {
            let share = if count == 1 { 1.0 } else { 0.7 };
            (tune.weapon_pellet_damage * share, count, 1.0)
        }
        Weapon::Needles => (tune.weapon_needle_damage, count, 1.3),
        Weapon::Missile => (tune.weapon_missile_damage, count, 1.0),
        Weapon::Nova => (tune.weapon_orb_damage, count, 1.0),
        Weapon::Spiral => (tune.weapon_spiral_damage, count, 0.1),
        Weapon::Mine => (tune.weapon_mine_damage, count, 1.8),
        Weapon::None | Weapon::Tether => (0.0, 0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tuning::DEFAULT;

    fn salvo(threat: f32, shot_damage: f32, shots: u32, armed: u32) -> Salvo {
        Salvo {
            threat,
            pith: 0.0,
            shot_damage,
            shots,
            armed,
            volleys_in_window: if shots > 1 { 2.0 } else { 1.0 },
            hits: if shots > 1 { 0.96 } else { 1.0 },
        }
    }

    #[test]
    fn a_tiny_creatures_nail_blast_stays_under_the_volley_cap() {
        // 128 needles of sharpness 3.8 (a ring 14 glass cannon): 1070 raw against a pool of 148.
        let threat = 6.0;
        let shot = DEFAULT.weapon_needle_damage * 3.8;
        let raw = shot * 128.0;
        assert!(raw > pool_ref(threat, 0.0, &DEFAULT));
        let s = budget_scale(&DEFAULT, &salvo(threat, shot, 128, 1));
        assert!(s < 1.0);
        let cap = DEFAULT.balance_volley_cap * pool_ref(threat, 0.0, &DEFAULT);
        assert!(raw * s <= cap * 1.0001, "{} vs {cap}", raw * s);
        // Rage and extra hardpoints raise the rate only up to the window cap.
        let s9 = budget_scale(&DEFAULT, &salvo(threat, shot, 128, 9));
        let window = DEFAULT.balance_window_cap * pool_ref(threat, 0.0, &DEFAULT);
        assert!(raw * 9.0 * 2.0 * 0.96 * s9 <= window * 1.0001);
    }

    #[test]
    fn a_light_gun_is_untouched() {
        // A lone pellet at HOME is far below every cap.
        let s = budget_scale(&DEFAULT, &salvo(1.0, 18.0, 1, 1));
        assert_eq!(s, 1.0);
    }
}
