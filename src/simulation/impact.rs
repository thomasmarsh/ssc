//! Kinetic impacts: what two bodies do to each other when they strike. One reusable pure
//! function (`kinetic_damage`) turns a closing speed and the two bodies' inverse masses into a
//! damage figure; the contact solver calls it for every pair that closes fast enough, and the
//! Lunatic fling uses it for the speed it adds. Numbers live in `tuning`.
//!
//! The figure is symmetric (both bodies take it), grows with speed and with either mass, is
//! zero below a minimum closing speed (so resting contact and separation jitter do nothing) and
//! is capped. A fixed body has inverse mass zero, so a wall is struck as hard as the mover is
//! heavy. Nothing here is random.

use super::*;

/// Damage of a strike at `closing_speed` between bodies of inverse mass `inverse_a` and
/// `inverse_b` (zero for a fixed body). Zero if both are fixed, the speed is below the
/// threshold, or an input is not a finite number.
pub fn kinetic_damage(closing_speed: f32, inverse_a: f32, inverse_b: f32, tune: &Tunables) -> f32 {
    let inverse_sum = inverse_a + inverse_b;
    if !(closing_speed.is_finite() && inverse_sum.is_finite()) || inverse_sum <= 0.0 {
        return 0.0;
    }
    let over = closing_speed.min(tune.impact_speed_cap) - tune.impact_min_speed;
    if over <= 0.0 {
        return 0.0;
    }
    let reduced_mass = 1.0 / inverse_sum;
    (tune.impact_scale * 0.5 * reduced_mass * over * over).min(tune.impact_cap)
}

/// Key of an unordered pair of bodies.
pub(super) fn pair_key(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

/// Applies a strike of `raw` damage to both bodies; returns what the ship's side dealt to a
/// creature or base (for the run record). The ship takes its share (nothing under a lunatic
/// field, less under PLATING: `player_factor`) and deals as it does with its other weapons: a free rock shrugs most of it off.
pub(super) fn strike(
    a: &mut Body,
    b: &mut Body,
    raw: f32,
    invulnerability: f32,
    player_factor: f32,
    tune: &Tunables,
) -> f32 {
    let mut dealt = 0.0;
    let amounts = [(&*a, &*b), (&*b, &*a)].map(|(target, other)| {
        // A planetoid is a gentle wall (see `contact_damage`): it hurts nothing that hits it.
        let amount = if other.kind == BodyKind::Asteroid && other.rock == RockKind::Planetoid {
            0.0
        } else if target.kind == BodyKind::Player {
            if target.rig.aura > 0 {
                0.0
            } else {
                raw * tune.impact_player_share * player_factor
            }
        } else {
            armored(target, raw, other.kind == BodyKind::Player, tune)
        };
        (
            amount,
            other.kind == BodyKind::Player,
            other.sling_thrown > 0.0 || other.rune_pushed > 0.0 || other.rift_redirected > 0.0,
            other.rune_pushed > 0.0,
            other.rift_redirected,
        )
    });
    for (target, (amount, from_ship, hostile_rock, rune_push, rift_redirected)) in
        [a, b].into_iter().zip(amounts)
    {
        let invulnerable = if target.kind == BodyKind::Player {
            invulnerability
        } else {
            0.0
        };
        let taken = damage(target, amount, invulnerable, tune);
        if rune_push && taken > 0.0 && target.kind != BodyKind::Player {
            target.rune_pushed = target.rune_pushed.max(1.5);
        }
        if rift_redirected > 0.0 && taken > 0.0 && target.kind != BodyKind::Player {
            target.rift_redirected = target.rift_redirected.max(rift_redirected);
        }
        if hostile_rock && target.health <= 0.0 && target.kind != BodyKind::Player {
            target.hostile_rock_kill = true;
        }
        if from_ship && matches!(target.kind, BodyKind::Creature | BodyKind::Base) {
            dealt += taken;
        }
    }
    dealt
}

impl Game {
    /// Drops pair cooldowns that have run out.
    pub(super) fn prune_impacts(&mut self) {
        let now = self.time;
        self.impact_gap.retain(|_, until| *until > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, add, empty_game};

    /// The same, for two masses (`f32::INFINITY` for a fixed body).
    fn kinetic_damage_for_masses(closing_speed: f32, mass_a: f32, mass_b: f32) -> f32 {
        let inverse = |m: f32| {
            if m.is_finite() && m > 0.0 {
                1.0 / m
            } else {
                0.0
            }
        };
        kinetic_damage(
            closing_speed,
            inverse(mass_a),
            inverse(mass_b),
            &DEFAULT_TUNING,
        )
    }

    #[test]
    fn it_is_symmetric_deterministic_and_zero_below_the_threshold() {
        for (v, a, b) in [(500.0, 10.0, 80.0), (900.0, 3.0, 200.0), (350.0, 8.0, 8.0)] {
            let d = kinetic_damage_for_masses(v, a, b);
            assert_eq!(d, kinetic_damage_for_masses(v, b, a), "symmetric");
            assert_eq!(d, kinetic_damage_for_masses(v, a, b), "deterministic");
            assert!(d > 0.0);
        }
        for v in [0.0, 1.0, 120.0, DEFAULT_TUNING.impact_min_speed] {
            assert_eq!(kinetic_damage_for_masses(v, 50.0, 50.0), 0.0, "{v}");
        }
    }

    #[test]
    fn it_grows_with_speed_and_with_either_mass_until_the_cap() {
        let mut last = 0.0;
        for k in 0..60 {
            let v = DEFAULT_TUNING.impact_min_speed + k as f32 * 20.0;
            let d = kinetic_damage_for_masses(v, 20.0, 60.0);
            assert!(d >= last, "{v}");
            last = d;
        }
        let mut last = 0.0;
        for m in [2.0, 5.0, 10.0, 25.0, 60.0, 150.0, 400.0] {
            let d = kinetic_damage_for_masses(700.0, 12.0, m);
            assert!(d >= last, "{m}");
            last = d;
            assert!(d >= kinetic_damage_for_masses(700.0, 12.0, m / 2.0));
        }
        // A heavier mover does more than a lighter one against the same wall.
        assert!(
            kinetic_damage_for_masses(700.0, 100.0, f32::INFINITY)
                > kinetic_damage_for_masses(700.0, 10.0, f32::INFINITY)
        );
    }

    #[test]
    fn it_is_capped_and_survives_absurd_inputs() {
        let huge = kinetic_damage_for_masses(1.0e9, 1.0e6, f32::INFINITY);
        assert_eq!(huge, DEFAULT_TUNING.impact_cap);
        assert_eq!(
            kinetic_damage_for_masses(DEFAULT_TUNING.impact_speed_cap, 1.0e6, 1.0e6),
            DEFAULT_TUNING.impact_cap
        );
        assert_eq!(kinetic_damage(f32::NAN, 1.0, 1.0, &DEFAULT_TUNING), 0.0);
        assert_eq!(kinetic_damage(900.0, f32::NAN, 1.0, &DEFAULT_TUNING), 0.0);
        assert_eq!(
            kinetic_damage(900.0, 0.0, 0.0, &DEFAULT_TUNING),
            0.0,
            "two fixed bodies"
        );
        assert_eq!(kinetic_damage_for_masses(-900.0, 10.0, 10.0), 0.0);
    }

    fn rock(game: &mut Game, at: Vec2, v: Vec2, radius: f32, mass: f32) -> u64 {
        let id = add(game, BodyKind::Asteroid, at);
        let r = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        r.radius = radius;
        r.mass = mass;
        r.velocity = v;
        r.health = 1000.0;
        r.max_health = 1000.0;
        id
    }

    fn wall(game: &mut Game, at: Vec2, health: f32) -> u64 {
        let id = add(game, BodyKind::Asteroid, at);
        let w = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        w.rock = RockKind::Wall;
        w.pinned = true;
        w.radius = 30.0;
        w.mass = 720.0;
        w.health = health;
        w.max_health = health;
        id
    }

    fn life(game: &Game, id: u64) -> f32 {
        game.body(id).map_or(0.0, |b| b.health)
    }

    #[test]
    fn a_fast_heavy_rock_cracks_a_wall_and_a_slow_or_light_one_does_not() {
        let hit = |speed: f32, mass: f32| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            let w = wall(&mut game, Vec2::new(2000.0, 0.0), 240.0);
            rock(
                &mut game,
                Vec2::new(2000.0 - 65.0, 0.0),
                Vec2::new(speed, 0.0),
                40.0,
                mass,
            );
            game.resolve_contacts();
            240.0 - life(&game, w)
        };
        assert!(hit(750.0, 105.0) > 100.0, "{}", hit(750.0, 105.0));
        assert!(hit(750.0, 105.0) > hit(750.0, 12.0));
        assert!(hit(750.0, 105.0) > hit(450.0, 105.0));
        assert_eq!(hit(250.0, 105.0), 0.0, "a drifting rock does nothing");
        assert!(hit(1300.0, 400.0) <= DEFAULT_TUNING.impact_cap + 1e-3);
    }

    #[test]
    fn rock_on_rock_breaks_only_when_fast_and_heavier_rocks_do_more() {
        let hit = |speed: f32, mass: f32| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            rock(&mut game, Vec2::new(2000.0, 0.0), Vec2::ZERO, 40.0, 24.0);
            let b = rock(
                &mut game,
                Vec2::new(2000.0 - 78.0, 0.0),
                Vec2::new(speed, 0.0),
                40.0,
                mass,
            );
            game.resolve_contacts();
            1000.0 - life(&game, b)
        };
        assert_eq!(hit(280.0, 24.0), 0.0);
        assert!(hit(700.0, 24.0) > 0.0);
        assert!(hit(700.0, 90.0) > hit(700.0, 24.0));
    }

    #[test]
    fn rock_on_creature_hurts_the_creature_normally_and_the_rock_too() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let r = rock(
            &mut game,
            Vec2::new(2000.0, 0.0),
            Vec2::new(800.0, 0.0),
            40.0,
            60.0,
        );
        let c = crate::simulation::tests::spawn(
            &mut game,
            &Species::bogey(),
            Vec2::new(2000.0 + 50.0, 0.0),
        );
        let (cr, cc) = (life(&game, r), life(&game, c));
        game.resolve_contacts();
        assert!(life(&game, c) < cc, "a flung rock hurts a bogey");
        assert!(life(&game, r) < cr, "and the rock is not unscathed");
    }

    #[test]
    fn the_ships_own_ramming_keeps_free_rocks_armoured_but_not_walls() {
        let against = |wall_not_rock: bool| {
            let mut game = empty_game();
            game.player_invulnerability = 0.0;
            game.bodies[0].shield = 0.0;
            game.bodies[0].velocity = Vec2::new(900.0, 0.0);
            let id = if wall_not_rock {
                wall(&mut game, Vec2::new(40.0, 0.0), 240.0)
            } else {
                rock(&mut game, Vec2::new(40.0, 0.0), Vec2::ZERO, 30.0, 720.0)
            };
            let before = (life(&game, id), game.bodies[0].health);
            game.resolve_contacts();
            (before.0 - life(&game, id), before.1 - game.bodies[0].health)
        };
        let (wall_loss, ship_loss) = against(true);
        let (rock_loss, _) = against(false);
        assert!(wall_loss > 10.0, "{wall_loss}");
        assert!(
            (wall_loss / rock_loss / DEFAULT_TUNING.rock_hull_factor - 1.0).abs() < 0.05,
            "free rocks keep the {}x armour ({wall_loss} vs {rock_loss})",
            DEFAULT_TUNING.rock_hull_factor
        );
        assert!(ship_loss > 0.0, "the ship pays too");
    }

    #[test]
    fn resting_contact_and_jitter_do_nothing_and_a_pair_cannot_strike_twice_at_once() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let a = rock(&mut game, Vec2::new(2000.0, 0.0), Vec2::ZERO, 40.0, 60.0);
        let b = rock(
            &mut game,
            Vec2::new(2070.0, 0.0),
            Vec2::new(-20.0, 0.0),
            40.0,
            60.0,
        );
        for _ in 0..600 {
            game.step(DT, Input::default());
        }
        assert_eq!(life(&game, a), 1000.0);
        assert_eq!(life(&game, b), 1000.0);
        // A hard hit lands once, then the pair is quiet for the cooldown even if it overlaps.
        game.bodies.iter_mut().find(|x| x.id == b).unwrap().velocity = Vec2::new(-900.0, 0.0);
        game.bodies.iter_mut().find(|x| x.id == b).unwrap().position = Vec2::new(
            game.body(a).unwrap().position.x + 79.0,
            game.body(a).unwrap().position.y,
        );
        game.resolve_contacts();
        let after_first = life(&game, a);
        assert!(after_first < 1000.0);
        game.bodies.iter_mut().find(|x| x.id == b).unwrap().velocity = Vec2::new(-900.0, 0.0);
        game.bodies
            .iter_mut()
            .find(|x| x.id == b)
            .unwrap()
            .position
            .x -= 5.0;
        game.resolve_contacts();
        assert_eq!(life(&game, a), after_first, "per-pair cooldown");
    }

    #[test]
    fn a_flung_creature_is_a_missile_whose_damage_scales_with_its_mass() {
        // The throw itself is free (so a Lunatic does not grind down its own neighbours); what
        // hurts is where the thrown body lands, at the speed the fling gave it.
        let thrown = |species: Species| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            game.bodies[0].position = Vec2::new(5000.0, 5000.0);
            let spot = Vec2::new(2000.0, 0.0);
            crate::simulation::tests::spawn(&mut game, &Species::lunatic(), spot);
            let victim =
                crate::simulation::tests::spawn(&mut game, &species, spot + Vec2::new(12.0, 0.0));
            let before = {
                let b = game.body(victim).unwrap();
                b.health + b.shield
            };
            game.resolve_contacts();
            let b = game.body(victim).unwrap();
            assert_eq!(b.health + b.shield, before, "the throw itself is free");
            let speed = b.velocity.length();
            assert!(speed >= crate::simulation::FLING_SPEED * 0.9, "{speed}");
            // Into a wall at that speed.
            kinetic_damage(speed, 0.0, 1.0 / b.mass, &DEFAULT_TUNING)
        };
        let bogey = thrown(Species::bogey());
        let fatso = thrown(Species::fatso());
        assert!(bogey > 5.0 && bogey < 60.0, "sensible damage: {bogey}");
        assert!(fatso > bogey, "{fatso} vs {bogey}");
    }
}
