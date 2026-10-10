//! Splitter (the `split` gene): when a carrier dies it comes apart into two pieces (three above
//! `SPLIT_TRIPLE` strength), a beat later, and the pieces fly apart. Each piece has a third of
//! the hull, `SPLIT_SIZE` of the radius, a little more speed, a fraction of the bounty, and no
//! power of its own (children never split again). The seam on a living carrier brightens as it
//! nears death, and the death itself pays and drops as for any kill; the pieces pay little.
//! Nothing splits past the body budgets, and a creature that was eaten or starved does not.

use super::*;
use crate::genome::Species;
use crate::power::{self, Power};

/// The pieces of a dead splitter, waiting to fly apart.
#[derive(Clone, Debug)]
pub struct Pending {
    left: f32,
    at: Vec2,
    velocity: Vec2,
    species: Species,
    alert: bool,
}

impl Game {
    /// Queues the pieces of a destroyed carrier (called as it is removed).
    pub(super) fn split_dead(&mut self, body: &Body) {
        if body.kind != BodyKind::Creature
            || body.consumed
            || body.follower
            || !Power::Split.active(&body.genome)
            || self.splits.len() >= power::SPLIT_QUEUE
        {
            return;
        }
        let g = body.genome;
        let s = Power::Split.strength(&g);
        let n = if g.split >= power::SPLIT_TRIPLE { 3 } else { 2 };
        let mut child = g;
        child.split = 0.0;
        child.hull = (g.hull / 3.0).max(power::SPLIT_MIN_HULL);
        child.radius = (g.radius * power::SPLIT_SIZE).max(power::SPLIT_MIN_RADIUS);
        child.speed *= power::SPLIT_SPEED;
        child.bounty *= power::SPLIT_BOUNTY;
        child.shield *= 0.5;
        let species = Species {
            lineage: body.species,
            generation: body.generation,
            genome: child,
        };
        // Each piece heads off in its own direction, from the parent's id (no draws).
        let base = (body.id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as f32 / 16_777_216.0 * TAU;
        for k in 0..n {
            let a = base + k as f32 * TAU / n as f32;
            let dir = Vec2::from_angle(a);
            self.splits.push(Pending {
                left: power::SPLIT_TELL,
                at: body.position + dir * body.radius * 0.4,
                velocity: body.velocity * 0.5 + dir * (power::SPLIT_FLING * (0.7 + 0.3 * s)),
                species,
                alert: body.alert,
            });
        }
        self.effect(
            body.position,
            body.radius * 2.2,
            power::SPLIT_TELL,
            EffectKind::Pair,
        );
        self.cue(Cue::Split { at: body.position });
    }

    /// Lets the pieces fly apart when their beat is up, within the body budgets.
    pub(super) fn update_splits(&mut self, dt: f32) {
        if self.splits.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.splits);
        for mut p in pending {
            p.left -= dt;
            if p.left > 0.0 {
                self.splits.push(p);
                continue;
            }
            let sector = SectorId::containing(p.at);
            let here = self
                .bodies
                .iter()
                .filter(|b| {
                    b.kind == BodyKind::Creature && SectorId::containing(b.position) == sector
                })
                .count();
            if self.bodies.len() + self.food.len() + self.eggs.len() + 2
                >= self.tune.world_max_bodies
                || here + 1 >= world::SECTOR_BODY_BUDGET as usize
                || !self.active.contains(&sector)
            {
                continue;
            }
            let mut body = self.make_creature(&p.species, p.at);
            body.velocity = p.velocity;
            body.provisioned = true;
            body.alert = p.alert;
            self.add_body(body);
            self.effect(p.at, 18.0, 0.3, EffectKind::Birth);
        }
    }

    /// How close a carrier is to coming apart, 0 to 1: the seam glows with it.
    pub fn seam(&self, body: &Body) -> f32 {
        if !Power::Split.active(&body.genome) || body.max_health <= 0.0 {
            return 0.0;
        }
        (1.0 - body.health / body.max_health).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::simulation::tests::{DT, empty_game, spawn};

    fn splitter(split: f32) -> Genome {
        Genome {
            split,
            hull: 90.0,
            radius: 24.0,
            shield: 0.0,
            speed: 0.0,
            cruise: 0.0,
            bounty: 100.0,
            weapon: crate::genome::Weapon::None,
            trigger: crate::genome::Trigger::Harm,
            ..Genome::default()
        }
    }

    fn creatures(game: &Game) -> Vec<&Body> {
        game.bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature)
            .collect()
    }

    fn step(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            game.step(DT, Input::default());
        }
    }

    #[test]
    fn a_splitter_comes_apart_into_two_after_a_beat_and_three_when_strong() {
        for (split, want) in [(0.5, 2), (0.9, 3)] {
            let mut game = empty_game();
            let id = spawn(
                &mut game,
                &Species::of(splitter(split)),
                Vec2::new(0.0, 900.0),
            );
            game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 0.0;
            game.step(DT, Input::default());
            assert!(creatures(&game).is_empty(), "not yet: the beat");
            step(&mut game, power::SPLIT_TELL - 0.1);
            assert!(creatures(&game).is_empty(), "still waiting");
            step(&mut game, 0.3);
            let kids = creatures(&game);
            assert_eq!(kids.len(), want);
            for k in kids {
                assert!((k.max_health - 30.0).abs() < 1.0, "{}", k.max_health);
                assert!((k.radius - 24.0 * power::SPLIT_SIZE).abs() < 0.5);
                assert_eq!(k.genome.split, 0.0, "children never split again");
                assert!(k.velocity.length() > 50.0, "they fly apart");
            }
        }
    }

    #[test]
    fn the_pieces_can_be_killed_and_do_not_split_again() {
        let mut game = empty_game();
        let id = spawn(
            &mut game,
            &Species::of(splitter(0.5)),
            Vec2::new(0.0, 900.0),
        );
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 0.0;
        step(&mut game, 0.6);
        for b in game
            .bodies
            .iter_mut()
            .filter(|b| b.kind == BodyKind::Creature)
        {
            b.health = 0.0;
        }
        step(&mut game, 1.5);
        assert!(creatures(&game).is_empty());
        assert!(game.splits.is_empty());
    }

    #[test]
    fn the_pieces_pay_little_and_an_eaten_splitter_leaves_none() {
        let mut game = empty_game();
        let id = spawn(
            &mut game,
            &Species::of(splitter(0.5)),
            Vec2::new(0.0, 900.0),
        );
        {
            let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            b.health = 0.0;
            b.consumed = true;
        }
        step(&mut game, 1.0);
        assert!(creatures(&game).is_empty(), "eaten: no pieces");
        let mut game = empty_game();
        let id = spawn(
            &mut game,
            &Species::of(splitter(0.5)),
            Vec2::new(0.0, 900.0),
        );
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 0.0;
        step(&mut game, 0.6);
        let kid = creatures(&game)[0];
        assert!(kid.genome.bounty <= 100.0 * power::SPLIT_BOUNTY + 1e-3);
    }

    #[test]
    fn the_queue_is_capped_and_the_seam_glows_as_it_nears_death() {
        let mut game = empty_game();
        let id = spawn(
            &mut game,
            &Species::of(splitter(0.9)),
            Vec2::new(0.0, 900.0),
        );
        let body = game.bodies.iter().find(|b| b.id == id).unwrap().clone();
        assert_eq!(game.seam(&body), 0.0);
        for _ in 0..40 {
            game.split_dead(&body);
        }
        assert!(game.splits.len() <= power::SPLIT_QUEUE);
        let mut hurt = body.clone();
        hurt.health = hurt.max_health * 0.25;
        assert!((game.seam(&hurt) - 0.75).abs() < 1e-4);
    }
}
