//! Renewable planetoids: some planetoids (a share chosen by a hash of the spawn key, see
//! `mining::renewable`) slowly regrow the ore the beam took, so there is a reason to come back.
//! Regrowth runs at `regrow_rate` ore per second of game time while the sector is loaded, and
//! is caught up from a timestamp when it reloads, so leaving a planetoid alone for a while
//! refills it and unloading never refreshes or stalls one. Only game time matters, so the
//! result is deterministic. Rocks other than planetoids never regrow.

use super::mining::{quantize, renewable};
use super::*;

impl Game {
    /// Ore a renewable planetoid has regrown since `since`, for a sector that was not loaded.
    pub(super) fn regrown_since(&self, key: (SectorId, u32), since: f32) -> f32 {
        if renewable(self.seed, key, &self.tune) {
            ((self.time - since).max(0.0)) * self.tune.regrow_rate
        } else {
            0.0
        }
    }

    /// Grows back what the beam (or a civilization) took from loaded renewable planetoids.
    pub(super) fn update_regrowth(&mut self, dt: f32) {
        let seed = self.seed;
        let time = self.time;
        for body in self.bodies.iter_mut() {
            if !body.active || body.kind != BodyKind::Asteroid || body.rock != RockKind::Planetoid {
                continue;
            }
            let Some(key) = body.origin else { continue };
            if !self.mined.contains_key(&key) || !renewable(seed, key, &self.tune) {
                continue;
            }
            body.init_lode();
            let full = body.lode.full;
            let ore = (body.lode.ore + self.tune.regrow_rate * dt).min(full);
            body.set_ore(ore);
            if ore >= full - 1e-3 {
                self.mined.remove(&key);
                self.regrow_stamp.remove(&key);
            } else {
                self.mined.insert(key, quantize(full - ore));
                self.regrow_stamp.insert(key, time);
            }
        }
    }

    /// Whether this planetoid is a renewable one (for the HUD and tests).
    pub fn is_renewable(&self, body: &Body) -> bool {
        body.rock == RockKind::Planetoid
            && body
                .origin
                .is_some_and(|k| renewable(self.seed, k, &self.tune))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{DT, empty_game};
    use super::*;

    /// A loaded planetoid of the given kind (renewable or not) drained by `spent` ore.
    fn planetoid(game: &mut Game, want_renewable: bool, spent: f32) -> (u64, (SectorId, u32)) {
        let sector = SectorId { x: 0, y: 0 };
        let index = (1..500)
            .find(|&i| renewable(game.seed, (sector, i), &DEFAULT_TUNING) == want_renewable)
            .unwrap();
        let key = (sector, index);
        let id = super::super::tests::add(game, BodyKind::Asteroid, Vec2::new(0.0, 1500.0));
        let body = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        body.rock = RockKind::Planetoid;
        body.radius = 300.0;
        body.pinned = true;
        body.origin = Some(key);
        body.init_lode();
        body.set_ore(body.lode.full - spent);
        game.mined.insert(key, quantize(spent));
        (id, key)
    }

    fn ore(game: &Game, id: u64) -> f32 {
        game.body(id).unwrap().lode.ore
    }

    fn run(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            game.step(DT, Input::default());
        }
    }

    #[test]
    fn renewable_is_a_stable_share_of_planetoids() {
        let n = 2000;
        let hits = (0..n)
            .filter(|&i| {
                renewable(
                    42,
                    (
                        SectorId {
                            x: i % 40,
                            y: i / 40,
                        },
                        i as u32,
                    ),
                    &DEFAULT_TUNING,
                )
            })
            .count();
        let share = hits as f32 / n as f32;
        assert!(
            (share - DEFAULT_TUNING.renewable_share).abs() < 0.06,
            "{share}"
        );
        assert_eq!(
            renewable(42, (SectorId { x: 3, y: -2 }, 7), &DEFAULT_TUNING),
            renewable(42, (SectorId { x: 3, y: -2 }, 7), &DEFAULT_TUNING)
        );
    }

    #[test]
    fn a_renewable_planetoid_regrows_while_loaded_and_a_plain_one_never_does() {
        let mut game = empty_game();
        let (a, _) = planetoid(&mut game, true, 100.0);
        let (b, _) = planetoid(&mut game, false, 100.0);
        let (a0, b0) = (ore(&game, a), ore(&game, b));
        run(&mut game, 20.0);
        let gain = ore(&game, a) - a0;
        assert!(
            (gain - 20.0 * DEFAULT_TUNING.regrow_rate).abs() < 0.2,
            "regrew {gain} in 20 s"
        );
        assert_eq!(ore(&game, b), b0, "an ordinary planetoid stays spent");
    }

    #[test]
    fn regrowth_stops_at_full_and_forgets_the_wound() {
        let mut game = empty_game();
        let (id, key) = planetoid(&mut game, true, 5.0);
        run(&mut game, 5.0 / DEFAULT_TUNING.regrow_rate + 2.0);
        let body = game.body(id).unwrap();
        assert!((body.lode.ore - body.lode.full).abs() < 1e-2);
        assert!(!game.mined.contains_key(&key), "nothing left to remember");
        run(&mut game, 5.0);
        assert!(game.body(id).unwrap().lode.ore <= game.body(id).unwrap().lode.full);
    }

    #[test]
    fn an_unloaded_planetoid_catches_up_from_its_stamp_and_never_refreshes_early() {
        let mut game = empty_game();
        let (_, key) = planetoid(&mut game, true, 100.0);
        run(&mut game, 1.0);
        let spent_then = game.mined[&key];
        let stamp = game.regrow_stamp[&key];
        // Sixty seconds pass while the sector is away: the same sum a reload applies.
        game.time += 60.0;
        let back = game.regrown_since(key, stamp);
        assert!((back - 60.0 * DEFAULT_TUNING.regrow_rate).abs() < 1e-3);
        assert!(spent_then > back, "still partly spent after a minute");
        assert_eq!(game.regrown_since(key, game.time), 0.0);
        // A fresh body for the same planetoid comes back with the catch-up applied.
        let mut fresh = game.make_body(BodyKind::Asteroid, Vec2::ZERO);
        fresh.rock = RockKind::Planetoid;
        fresh.radius = 300.0;
        fresh.origin = Some(key);
        game.apply_mined(&mut fresh);
        let want = fresh.lode.full - (spent_then - back).max(0.0);
        assert!(
            (fresh.lode.ore - want).abs() < 1e-2,
            "{} vs {want}",
            fresh.lode.ore
        );
        let mut plain = game.make_body(BodyKind::Asteroid, Vec2::ZERO);
        plain.rock = RockKind::Planetoid;
        plain.radius = 300.0;
        let other = (1..500)
            .map(|i| (key.0, i))
            .find(|&k| !renewable(game.seed, k, &DEFAULT_TUNING))
            .unwrap();
        plain.origin = Some(other);
        game.mined.insert(other, 100.0);
        game.regrow_stamp.insert(other, 0.0);
        game.apply_mined(&mut plain);
        assert!(
            (plain.lode.full - plain.lode.ore - 100.0).abs() < 1e-2,
            "no catch-up"
        );
    }

    #[test]
    fn regrowth_is_deterministic() {
        let go = || {
            let mut game = empty_game();
            let (id, _) = planetoid(&mut game, true, 80.0);
            run(&mut game, 12.0);
            ore(&game, id)
        };
        assert_eq!(go(), go());
    }
}
