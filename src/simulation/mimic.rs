//! Lurefish (the `mimic` gene). A carrier below gene value `MIMIC_LURE` poses as a free rock
//! and drifts toward the ship; from it, as a bright pickup that hangs still. While disguised it
//! is not alert, fires nothing, and is left off the guide arrows, the radar and the pressure
//! count. It shows itself (a crack and a `MIMIC_TELL` second tell, then it hunts) when the ship
//! comes within `MIMIC_REVEAL` of its reach, when it is hurt, or when the ship has idled near
//! it for `MIMIC_IDLE` seconds. Nothing about the disguise persists: it is a state of the
//! live body, and a body that unloads is a fresh disguise.

use super::powers::PowerState;
use super::*;
use crate::power::{self, Power};

/// What a disguised body poses as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disguise {
    Rock,
    Lure,
}

impl Game {
    /// What `body` is posing as, if it is a disguised mimic.
    pub fn disguise(&self, body: &Body) -> Option<Disguise> {
        if body.kind != BodyKind::Creature || !Power::Mimic.active(&body.genome) || body.follower {
            return None;
        }
        let revealed = self
            .apexes
            .power
            .get(&body.id)
            .is_some_and(|s| s.revealed || s.reveal.is_some());
        if revealed {
            return None;
        }
        Some(if body.genome.mimic >= power::MIMIC_LURE {
            Disguise::Lure
        } else {
            Disguise::Rock
        })
    }

    /// A cracking mimic: 0 to 1 over the tell (zero when not revealing).
    pub fn reveal_progress(&self, body: &Body) -> f32 {
        self.apexes
            .power
            .get(&body.id)
            .and_then(|s| s.reveal)
            .map_or(0.0, |left| (1.0 - left / power::MIMIC_TELL).clamp(0.0, 1.0))
    }

    pub(super) fn step_mimic(
        &mut self,
        index: usize,
        state: &mut PowerState,
        dt: f32,
        ship: Option<Vec2>,
        cues: &mut Vec<Cue>,
    ) {
        let body = &self.bodies[index];
        let (at, g) = (body.position, body.genome);
        let hurt = body.health < body.max_health - 0.01 || body.shield < body.max_shield - 0.01;
        if state.revealed {
            return;
        }
        if let Some(left) = state.reveal.as_mut() {
            *left -= dt;
            let body = &mut self.bodies[index];
            body.alert = false;
            body.velocity *= (1.0 - 6.0 * dt).max(0.0);
            if *left <= 0.0 {
                state.reveal = None;
                state.revealed = true;
                self.bodies[index].alert = true;
                self.bodies[index].provoked = 4.0;
            }
            return;
        }
        let reach = g.power_params(Power::Mimic).reach;
        let idle_near = ship.is_some_and(|s| {
            at.distance(s) < reach
                && self
                    .player()
                    .is_some_and(|p| p.velocity.length() < power::MIMIC_IDLE_SPEED)
        });
        state.idle = if idle_near { state.idle + dt } else { 0.0 };
        let close = ship.is_some_and(|s| at.distance(s) < reach * power::MIMIC_REVEAL);
        if hurt || close || state.idle >= power::MIMIC_IDLE {
            state.reveal = Some(power::MIMIC_TELL);
            self.bodies[index].alert = false;
            cues.push(Cue::Reveal { at });
            self.effect(
                at,
                body_radius(&self.bodies[index]) * 2.5,
                0.3,
                EffectKind::Pair,
            );
            return;
        }
        // Posing: quiet, unarmed, and (as a rock) drifting toward the ship.
        let body = &mut self.bodies[index];
        body.alert = false;
        body.panic = 0.0;
        body.fire_cooldown = body.fire_cooldown.max(0.5);
        let cap = if g.mimic >= power::MIMIC_LURE {
            power::MIMIC_LURE_DRIFT
        } else {
            power::MIMIC_ROCK_DRIFT
        };
        if g.mimic < power::MIMIC_LURE
            && let Some(s) = ship
        {
            let toward = (s - body.position).normalize_or_zero();
            body.velocity += toward * cap * dt;
        }
        body.velocity = body.velocity.clamp_length_max(cap);
    }
}

fn body_radius(body: &Body) -> f32 {
    body.radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species, Trigger};
    use crate::simulation::tests::{DT, empty_game, set_player, spawn};

    fn lurefish(mimic: f32) -> Genome {
        Genome {
            power_params: crate::power::params_for(crate::power::Power::Mimic, 5.0, 260.0, 1.0),
            mimic,
            diet: crate::genome::Diet::Hunt,
            trigger: Trigger::Proximity,
            contact_damage: 22.0,
            bounty: 220.0,
            speed: 120.0,
            cruise: 20.0,
            hull: 40.0,
            radius: 14.0,
            weapon: crate::genome::Weapon::None,
            ..Genome::default()
        }
    }

    fn run(game: &mut Game, keep: &[u64], seconds: f32, mut each: impl FnMut(&Game)) {
        for _ in 0..(seconds / DT) as usize {
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || keep.contains(&b.id));
            game.step(DT, Input::default());
            each(game);
        }
    }

    #[test]
    fn a_lurefish_poses_as_a_rock_or_a_pickup_and_is_not_alert_or_guided_to() {
        for (mimic, want) in [(0.5, Disguise::Rock), (0.8, Disguise::Lure)] {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            let id = spawn(
                &mut game,
                &Species::of(lurefish(mimic)),
                Vec2::new(0.0, 700.0),
            );
            run(&mut game, &[id], 1.0, |g| {
                let b = g.body(id).unwrap();
                assert_eq!(g.disguise(b), Some(want));
                assert!(!b.alert, "a disguised mimic is quiet");
                assert_eq!(g.pressure(), (0, 0), "and counts for nothing");
            });
            let b = game.body(id).unwrap();
            assert!(game.disguise(b).is_some());
            assert!(
                game.guide_bearings(Vec2::ZERO, Vec2::new(900.0, 450.0))
                    .iter()
                    .all(|g| !matches!(g.kind, GuideKind::Wildlife { .. })),
                "no arrow points at it"
            );
        }
    }

    #[test]
    fn it_cracks_for_the_minimum_tell_before_it_hunts_when_the_ship_comes_close() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = spawn(
            &mut game,
            &Species::of(lurefish(0.8)),
            Vec2::new(0.0, 600.0),
        );
        set_player(&mut game, Vec2::new(0.0, 600.0 - 100.0), Vec2::ZERO);
        let (mut cracked, mut hunting) = (None, None);
        run(&mut game, &[id], 3.0, |g| {
            let b = g.body(id).unwrap();
            if g.reveal_progress(b) > 0.0 && cracked.is_none() {
                cracked = Some(g.time);
            }
            if cracked.is_some()
                && g.disguise(b).is_none()
                && g.reveal_progress(b) == 0.0
                && b.alert
                && hunting.is_none()
            {
                hunting = Some(g.time);
            }
        });
        let (c, h) = (cracked.expect("a crack"), hunting.expect("then it hunts"));
        assert!(h - c >= power::MIMIC_TELL - 0.05, "{}", h - c);
    }

    #[test]
    fn being_shot_or_an_idle_ship_near_it_reveals_it_and_a_far_ship_does_not() {
        // Hurt.
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = spawn(
            &mut game,
            &Species::of(lurefish(0.5)),
            Vec2::new(0.0, 900.0),
        );
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 30.0;
        run(&mut game, &[id], 1.0, |_| {});
        assert!(game.disguise(game.body(id).unwrap()).is_none());
        // Idle near it (inside reach, outside the close ring) for the idle time.
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = spawn(
            &mut game,
            &Species::of(lurefish(0.8)),
            Vec2::new(0.0, 700.0),
        );
        set_player(&mut game, Vec2::new(0.0, 700.0 - 200.0), Vec2::ZERO);
        let mut shown = None;
        run(&mut game, &[id], 5.0, |g| {
            if g.disguise(g.body(id).unwrap()).is_none() && shown.is_none() {
                shown = Some(g.time);
            }
        });
        let t = shown.expect("revealed by idling");
        assert!(t >= power::MIMIC_IDLE - 0.1, "{t}");
        // Far away: stays hidden forever.
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let id = spawn(
            &mut game,
            &Species::of(lurefish(0.8)),
            Vec2::new(0.0, 1800.0),
        );
        run(&mut game, &[id], 8.0, |_| {});
        assert!(game.disguise(game.body(id).unwrap()).is_some());
    }

    #[test]
    fn a_rock_mimic_drifts_toward_the_ship_but_slowly_and_a_lure_hangs_still() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let rock = spawn(
            &mut game,
            &Species::of(lurefish(0.5)),
            Vec2::new(0.0, 1200.0),
        );
        run(&mut game, &[rock], 1.5, |_| {});
        let b = game.body(rock).unwrap();
        assert!(b.position.y < 1200.0, "toward the ship");
        assert!(b.velocity.length() <= power::MIMIC_ROCK_DRIFT + 1e-3);
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let lure = spawn(
            &mut game,
            &Species::of(lurefish(0.8)),
            Vec2::new(0.0, 1200.0),
        );
        run(&mut game, &[lure], 1.5, |_| {});
        assert!(game.body(lure).unwrap().velocity.length() <= power::MIMIC_LURE_DRIFT + 1e-3);
    }
}
