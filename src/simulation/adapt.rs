//! Adaptive resistance: the counter to spamming one gun. A tough creature or an apex elder
//! builds resistance to the damage family (`arsenal::Family`) that has hurt it most lately: each
//! family has a meter that fills as that family deals damage (a third of the creature's pool
//! fills it) and bleeds away with time, faster once the family is no longer being used. The meter
//! cuts the family's damage by up to `adapt_max` (so a hit always lands for something), and the
//! hull bar shows a pip per family with a meter, so the player can read it and switch guns
//! (`[` and `]`). Everything is plain numbers in `tuning`; nothing is hidden.

use super::arsenal::Family;
use super::*;

/// The meters of one creature.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Resist {
    /// How built up the resistance to each family is, in [0, 1].
    meter: [f32; 4],
    /// Seconds since each family last hit.
    idle: [f32; 4],
}

impl Resist {
    /// The share of a family's damage that still lands, in `[1 - adapt_max, 1]`.
    pub fn scale(&self, family: Family, tune: &Tunables) -> f32 {
        1.0 - tune.adapt_max * self.meter[family.index()]
    }

    /// The meter of each family, in `Family::ALL` order.
    pub fn meters(&self) -> [f32; 4] {
        self.meter
    }

    /// A family dealt `dealt` hull to a creature with a pool of `pool`.
    pub fn hit(&mut self, family: Family, dealt: f32, pool: f32, tune: &Tunables) {
        let i = family.index();
        self.meter[i] = (self.meter[i] + tune.adapt_gain * dealt / pool.max(1.0)).min(1.0);
        self.idle[i] = 0.0;
    }

    /// Time passes: every meter bleeds, the idle ones faster.
    pub fn tick(&mut self, dt: f32, tune: &Tunables) {
        for i in 0..4 {
            self.idle[i] += dt;
            let rate = tune.adapt_decay
                + if self.idle[i] > tune.adapt_idle {
                    tune.adapt_idle_decay
                } else {
                    0.0
                };
            self.meter[i] = (self.meter[i] - rate * dt).max(0.0);
        }
    }

    /// Whether any meter is worth showing.
    pub fn shown(&self, tune: &Tunables) -> bool {
        self.meter.iter().any(|m| *m >= tune.adapt_shown)
    }
}

/// Whether a body adapts: an apex elder, or a creature with a big enough pool.
pub(super) fn adaptive(
    body: &Body,
    apexes: &BTreeMap<(SectorId, u32), apexes::ApexInfo>,
    tune: &Tunables,
) -> bool {
    body.kind == BodyKind::Creature
        && !body.follower
        && (body.origin.is_some_and(|key| apexes.contains_key(&key))
            || body.max_health + body.max_shield >= tune.adapt_min_pool)
}

impl Game {
    /// Records that `family` dealt `dealt` to the adaptive body `id` with the given pool.
    pub(super) fn note_family_hit(&mut self, id: u64, family: Family, dealt: f32, pool: f32) {
        if dealt > 0.0 {
            self.adapt
                .entry(id)
                .or_default()
                .hit(family, dealt, pool, &self.tune);
        }
    }

    /// Per step: meters bleed, and a creature that is gone takes its meters with it.
    pub(super) fn update_adapt(&mut self, dt: f32) {
        if self.adapt.is_empty() {
            return;
        }
        let bodies = &self.bodies;
        self.adapt
            .retain(|id, _| bodies.iter().any(|b| b.id == *id && b.health > 0.0));
        for resist in self.adapt.values_mut() {
            resist.tick(dt, &self.tune);
        }
        self.adapt
            .retain(|_, r| r.shown(&self.tune) || r.meter.iter().any(|m| *m > 0.0));
    }

    /// The resistance meters of a creature the HUD may draw (None when nothing is worth a pip).
    pub fn resistance_of(&self, id: u64) -> Option<[f32; 4]> {
        self.adapt
            .get(&id)
            .filter(|r| r.shown(&self.tune))
            .map(|r| r.meters())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spamming_one_family_builds_resistance_that_is_bounded_and_never_total() {
        let mut r = Resist::default();
        assert_eq!(r.scale(Family::Kinetic, &DEFAULT_TUNING), 1.0);
        let pool = 1000.0;
        let mut last = 1.0;
        for _ in 0..200 {
            r.hit(Family::Kinetic, 10.0, pool, &DEFAULT_TUNING);
            let now = r.scale(Family::Kinetic, &DEFAULT_TUNING);
            assert!(
                now <= last,
                "resistance only grows while the family keeps hitting"
            );
            last = now;
        }
        assert!(
            (last - (1.0 - DEFAULT_TUNING.adapt_max)).abs() < 1e-6,
            "{last}"
        );
        assert!(last > 0.0, "a hit always lands for something");
        // The other families are untouched.
        for f in [Family::Needle, Family::Lance, Family::Explosive] {
            assert_eq!(r.scale(f, &DEFAULT_TUNING), 1.0);
        }
        // A third of the pool is enough to fill it.
        let mut fresh = Resist::default();
        fresh.hit(
            Family::Needle,
            pool / DEFAULT_TUNING.adapt_gain,
            pool,
            &DEFAULT_TUNING,
        );
        assert!((fresh.meters()[Family::Needle.index()] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_meter_decays_and_decays_faster_once_the_family_is_idle() {
        let mut r = Resist::default();
        r.hit(Family::Lance, 330.0, 1000.0, &DEFAULT_TUNING);
        let full = r.meters()[Family::Lance.index()];
        // While it keeps hitting, only the slow bleed.
        let mut busy = r;
        for _ in 0..40 {
            busy.hit(Family::Lance, 0.0, 1000.0, &DEFAULT_TUNING);
            busy.tick(0.05, &DEFAULT_TUNING);
        }
        let busy_after = busy.meters()[Family::Lance.index()];
        assert!(
            (full - busy_after - DEFAULT_TUNING.adapt_decay * 2.0).abs() < 0.01,
            "{full} {busy_after}"
        );
        // Left alone, it clears within about ten seconds.
        let mut idle = r;
        let mut seconds = 0.0;
        while idle.meters()[Family::Lance.index()] > 0.0 && seconds < 60.0 {
            idle.tick(0.1, &DEFAULT_TUNING);
            seconds += 0.1;
        }
        let expected = DEFAULT_TUNING.adapt_idle
            + (full - DEFAULT_TUNING.adapt_idle * DEFAULT_TUNING.adapt_decay)
                / (DEFAULT_TUNING.adapt_decay + DEFAULT_TUNING.adapt_idle_decay);
        assert!(
            (seconds - expected).abs() < 0.6,
            "{seconds} against {expected}"
        );
        assert!(seconds < 14.0, "{seconds}");
        assert!(!idle.shown(&DEFAULT_TUNING));
    }

    #[test]
    fn switching_weapons_relieves_the_resistance() {
        let pool = 1000.0;
        // Twenty seconds of fire, 40 hull a second dealt, one gun or two taking turns every 4 s.
        let fight = |two: bool| -> (f32, f32) {
            let mut r = Resist::default();
            let mut dealt_total = 0.0;
            for step in 0..400 {
                let f = if two && (step / 80) % 2 == 1 {
                    Family::Needle
                } else {
                    Family::Kinetic
                };
                let share = r.scale(f, &DEFAULT_TUNING);
                let dealt = 2.0 * share;
                dealt_total += dealt;
                r.hit(f, dealt, pool, &DEFAULT_TUNING);
                r.tick(0.05, &DEFAULT_TUNING);
            }
            (dealt_total, r.meters().iter().copied().fold(0.0, f32::max))
        };
        let (one, one_meter) = fight(false);
        let (two, two_meter) = fight(true);
        assert!(
            two > one * 1.12,
            "switching deals more: {two} against {one}"
        );
        assert!(
            two_meter < one_meter,
            "and leaves less resistance: {two_meter} against {one_meter}"
        );
    }

    #[test]
    fn only_tough_creatures_and_elders_adapt_and_it_reaches_the_hud() {
        let mut game = crate::simulation::tests::empty_game();
        let mut species = crate::genome::Species::bogey();
        let small = crate::simulation::tests::spawn(&mut game, &species, Vec2::new(900.0, 0.0));
        species.genome.hull = 900.0;
        let tough = crate::simulation::tests::spawn(&mut game, &species, Vec2::new(-900.0, 0.0));
        let apexes = BTreeMap::new();
        let find = |game: &Game, id: u64| game.bodies.iter().find(|b| b.id == id).unwrap().clone();
        assert!(!adaptive(&find(&game, small), &apexes, &DEFAULT_TUNING));
        assert!(adaptive(&find(&game, tough), &apexes, &DEFAULT_TUNING));
        game.note_family_hit(tough, Family::Kinetic, 400.0, 900.0);
        assert!(game.resistance_of(tough).unwrap()[0] > 0.9);
        assert_eq!(game.resistance_of(small), None);
        assert!(game.adapt[&tough].scale(Family::Kinetic, &DEFAULT_TUNING) < 0.5);
        assert_eq!(
            game.adapt[&tough].scale(Family::Lance, &DEFAULT_TUNING),
            1.0
        );
        // Meters are dropped with the body and with time.
        for _ in 0..400 {
            game.update_adapt(0.1);
        }
        assert_eq!(game.resistance_of(tough), None);
    }

    /// In play: a tough, stationary target under 30 seconds of fire. Spamming the stock gun
    /// hardens it (the stock meter climbs to a real fraction of its cap); taking turns between the
    /// stock gun and the lance deals more in all, because the family not in use bleeds its
    /// resistance away.
    #[test]
    fn switching_guns_in_play_beats_spamming_one() {
        use crate::simulation::arsenal::Profile;
        use crate::simulation::tests::{empty_game, set_player, spawn};
        let fight = |switch: bool| -> (f32, f32) {
            let mut game = empty_game();
            game.cargo.fuel = 200.0;
            game.loadout.arsenal.acquire(Profile::Pierce, 1);
            let mut species = crate::genome::Species::bogey();
            species.genome.hull = 8000.0;
            species.genome.shield = 0.0;
            species.genome.speed = 0.0;
            species.genome.cruise = 0.0;
            species.genome.weapon = crate::genome::Weapon::None;
            let target = spawn(&mut game, &species, Vec2::new(500.0, 0.0));
            let mut peak = 0.0_f32;
            for k in 0..(30.0 / 0.05) as usize {
                if switch {
                    let want = if (k / 80) % 2 == 0 {
                        Profile::Stock
                    } else {
                        Profile::Pierce
                    };
                    if game.loadout.arsenal.active != want {
                        game.loadout.arsenal.set_active(want);
                        game.refresh_stats();
                    }
                }
                set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
                game.step(
                    0.05,
                    Input {
                        fire: true,
                        aim_direction: Some(Vec2::X),
                        ..Input::default()
                    },
                );
                let body = game.bodies.iter_mut().find(|b| b.id == target).unwrap();
                body.position = Vec2::new(500.0, 0.0);
                body.velocity = Vec2::ZERO;
                if let Some(m) = game.resistance_of(target) {
                    peak = peak.max(m.into_iter().fold(0.0, f32::max));
                }
            }
            (game.run.damage_dealt, peak)
        };
        let (spam, spam_peak) = fight(false);
        let (turns, turns_peak) = fight(true);
        assert!(
            (0.5..=1.0).contains(&spam_peak),
            "spamming hardens it: {spam_peak}"
        );
        assert!(
            turns_peak < spam_peak,
            "taking turns leaves less: {turns_peak} against {spam_peak}"
        );
        assert!(
            turns > spam * 1.05,
            "taking turns deals more: {turns} against {spam}"
        );
        // And a hit never lands for less than the floor of what it would have.
        assert!(spam > 0.0 && (1.0 - DEFAULT_TUNING.adapt_max) > 0.0);
    }
}
