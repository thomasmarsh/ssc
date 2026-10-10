//! One health for a jointed body in easier places.
//!
//! A creature of many parts is a compound enemy: every part is its own body with its own hull,
//! so a ten-part serpent took about ten times the damage of a single circle of the same head
//! (and paid ten bounties). In easy places (threat at most `pool_full_threat`) the whole body
//! shares one pool instead, sized in head-equivalents: the head's hull plus `pool_floor` of
//! every other part's. Hull lost by any part drains the pool, and the parts are kept whole
//! (no part dies alone). As the pool falls the body sheds parts from the tail end, which break
//! off, lose their weapons and drift away for `pool_drift` s before they vanish (silently, like
//! anything consumed); at zero the head dies as an ordinary kill (one bounty, scaled by the pool
//! size in heads, one drop) and every remaining part breaks off. Deeper (threat from
//! `pool_full_threat` to `pool_none_threat`) the pool grows back toward the plain sum of
//! parts, and from `pool_none_threat` the chain is as it always was. Apex elders keep their
//! own rules (`apexes`). Only behaviour: nothing here changes generation.

use super::*;

/// The shared health of one chain.
#[derive(Clone, Copy, Debug)]
pub struct Pool {
    /// What is left, 0 to 1.
    pub left: f32,
    /// Hull the whole pool is worth, in raw hull points.
    total: f32,
    /// Parts when it began; the share of them kept follows `left`.
    parts: usize,
    /// Bounty multiplier: the pool's size in heads.
    reward: f32,
}

/// How pooled a chain born where the threat is `threat` is: 1 fully, 0 not at all.
pub fn share(threat: f32, tune: &Tunables) -> f32 {
    let t = ((threat - tune.pool_full_threat) / (tune.pool_none_threat - tune.pool_full_threat))
        .clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

impl Game {
    /// The pool of the chain `chain`, if it has one (for the drawing and tests).
    pub fn chain_pool(&self, chain: u32) -> Option<Pool> {
        self.pools.get(&chain).copied()
    }

    /// Runs pooled health and the drifting pieces; after every source of damage this step and
    /// before the dead are removed.
    pub(super) fn update_breakups(&mut self, dt: f32) {
        self.age_adrift(dt);
        if self.chains.is_empty() {
            self.pools.clear();
            return;
        }
        let ids: Vec<u32> = self.chains.keys().copied().collect();
        self.pools.retain(|id, _| self.chains.contains_key(id));
        for id in ids {
            self.step_pool(id);
        }
    }

    /// Pieces that broke off fade out: gone without a bang when their time is up.
    fn age_adrift(&mut self, dt: f32) {
        for body in self.bodies.iter_mut().filter(|b| b.adrift > 0.0) {
            body.adrift -= dt;
            if body.adrift <= 0.0 {
                body.consumed = true;
                body.health = 0.0;
            }
        }
    }

    fn step_pool(&mut self, id: u32) {
        let Some(chain) = self.chains.get(&id) else {
            return;
        };
        let ids: Vec<u64> = chain.parts.iter().map(|p| p.id).collect();
        let Some(&head_id) = ids.first() else {
            return;
        };
        if ids.len() < 2 && !self.pools.contains_key(&id) {
            return;
        }
        let Some(head) = self.body(head_id) else {
            return;
        };
        // Elders and the ship's own kind keep their rules; deep bodies are plain sums.
        let elder = head
            .origin
            .is_some_and(|key| self.apexes.info.contains_key(&key));
        let pooled = share(head.genes.threat, &self.tune);
        if elder
            || head.kind != BodyKind::Creature
            || (pooled <= 0.0 && !self.pools.contains_key(&id))
        {
            return;
        }
        let mut pool = match self.pools.get(&id) {
            Some(pool) => *pool,
            None => {
                let (head_max, others): (f32, f32) =
                    ids.iter()
                        .enumerate()
                        .fold((0.0, 0.0), |(h, o), (n, part)| {
                            let max = self.body(*part).map_or(0.0, |b| b.max_health);
                            if n == 0 { (max, o) } else { (h, o + max) }
                        });
                let counted = self.tune.pool_floor + (1.0 - self.tune.pool_floor) * (1.0 - pooled);
                let total = (head_max + others * counted).max(1.0);
                Pool {
                    left: 1.0,
                    total,
                    parts: ids.len(),
                    reward: (total / head_max.max(1.0)).max(1.0),
                }
            }
        };
        // Hull lost this step, wherever it landed; every part is made whole again.
        let mut lost = 0.0;
        for part in &ids {
            if let Some(body) = self.bodies.iter_mut().find(|b| b.id == *part) {
                if body.consumed {
                    continue;
                }
                lost += (body.max_health - body.health.max(0.0)).max(0.0);
                body.health = body.max_health;
            }
        }
        pool.left = (pool.left - lost / pool.total).max(0.0);
        if pool.left <= 0.0 {
            self.pools.remove(&id);
            self.break_apart(id, head_id, pool.reward);
            return;
        }
        // Shed from the tail as the pool falls.
        let keep = ((pool.left * pool.parts as f32).ceil() as usize).clamp(1, pool.parts);
        while self
            .chains
            .get(&id)
            .is_some_and(|c| c.parts.len() > keep.max(1))
        {
            self.shed_last(id);
        }
        self.pools.insert(id, pool);
    }

    /// The last part of chain `id` breaks off and drifts away.
    fn shed_last(&mut self, id: u32) {
        let Some(part) = self
            .chains
            .get_mut(&id)
            .and_then(|c| (c.parts.len() > 1).then(|| c.parts.pop()))
        else {
            return;
        };
        let Some(part) = part else {
            return;
        };
        self.cast_off(part.id);
    }

    /// Detaches body `id` from its creature as a harmless drifting piece.
    fn cast_off(&mut self, id: u64) {
        let seed = self.seed;
        let Some(body) = self.bodies.iter_mut().find(|b| b.id == id) else {
            return;
        };
        let h = world::hash2(seed ^ 0xB4EA_C0FF, (id & 0x7FFF_FFFF) as i32, 17);
        let angle = (h >> 40) as f32 / 16_777_216.0 * TAU;
        let speed = self.tune.pool_fling * (0.4 + 0.6 * ((h >> 20) & 0xFF) as f32 / 255.0);
        body.chain = None;
        body.follower = true;
        body.adrift = self.tune.pool_drift;
        body.health = body.max_health.max(1.0);
        body.velocity += Vec2::from_angle(angle) * speed;
        body.genome.weapon = crate::genome::Weapon::None;
        body.genome.contact_damage = 0.0;
        body.genome.fling = 0.0;
        body.fire_cooldown = f32::MAX;
    }

    /// The pool is gone: every part but the head breaks off and the head dies as a kill.
    fn break_apart(&mut self, id: u32, head: u64, reward: f32) {
        if let Some(chain) = self.chains.get_mut(&id) {
            let rest: Vec<u64> = chain.parts.iter().skip(1).map(|p| p.id).collect();
            chain.parts.truncate(1);
            for part in rest {
                self.cast_off(part);
            }
        }
        if let Some(body) = self.bodies.iter_mut().find(|b| b.id == head) {
            body.genome.bounty *= reward;
            body.health = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species, Weapon};
    use crate::simulation::tests::{DT, empty_game, set_player};

    fn serpent(parts: u8) -> Genome {
        Genome {
            segments: parts,
            taper: 1.0,
            radius: 14.0,
            hull: 40.0,
            speed: 0.0,
            cruise: 0.0,
            wave: 0.0,
            weapon: Weapon::None,
            contact_damage: 0.0,
            ..Genome::default()
        }
    }

    fn place(game: &mut Game, genome: Genome, threat: f32) -> u32 {
        let mut head = game.make_creature(&Species::of(genome), Vec2::new(300.0, 0.0));
        head.wander = 0.0;
        head.genes.threat = threat;
        game.spawn_chain(head);
        for body in game
            .bodies
            .iter_mut()
            .filter(|b| b.kind == BodyKind::Creature)
        {
            body.genes.threat = threat;
        }
        *game.chains.keys().last().unwrap()
    }

    /// Seconds of the ship's gun until no creature is left.
    fn kill_time(parts: u8, threat: f32) -> f32 {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        place(&mut game, serpent(parts), threat);
        let mut t = 0.0;
        while game
            .bodies
            .iter()
            .any(|b| b.kind == BodyKind::Creature && b.adrift <= 0.0)
            && t < 120.0
        {
            let ship = game.player().unwrap().position;
            let target = game
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Creature && b.adrift <= 0.0)
                .map(|b| b.position)
                .min_by(|a, b| a.distance(ship).total_cmp(&b.distance(ship)))
                .unwrap();
            game.step(
                DT,
                Input {
                    fire: true,
                    aim_direction: Some((target - ship).normalize()),
                    ..Input::default()
                },
            );
            t += DT;
        }
        t
    }

    #[test]
    fn pooling_fades_with_threat_and_is_neutral_at_home() {
        assert_eq!(share(1.0, &DEFAULT_TUNING), 1.0);
        assert_eq!(share(DEFAULT_TUNING.pool_full_threat, &DEFAULT_TUNING), 1.0);
        assert_eq!(share(DEFAULT_TUNING.pool_none_threat, &DEFAULT_TUNING), 0.0);
        assert_eq!(share(9.0, &DEFAULT_TUNING), 0.0);
        let mid = share(
            (DEFAULT_TUNING.pool_full_threat + DEFAULT_TUNING.pool_none_threat) / 2.0,
            &DEFAULT_TUNING,
        );
        assert!((0.4..0.6).contains(&mid));
    }

    #[test]
    fn a_ten_part_body_in_an_easy_place_dies_in_about_the_time_of_two_heads_not_ten() {
        let single = kill_time(1, 1.0);
        let ten = kill_time(10, 1.0);
        let deep = kill_time(10, 6.0);
        eprintln!("MEASURE single {single:.2}s ten pooled {ten:.2}s ten deep {deep:.2}s");
        assert!(ten < single * 4.5, "{ten} vs {single}");
        assert!(deep > single * 5.0, "deep chains are plain sums: {deep}");
    }

    #[test]
    fn a_hurt_body_sheds_pieces_that_drift_harmlessly_then_vanish() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        set_player(&mut game, Vec2::new(0.0, -3000.0), Vec2::ZERO);
        let mut g = serpent(6);
        g.weapon = Weapon::Projectile;
        g.contact_damage = 9.0;
        let chain = place(&mut game, g, 1.0);
        let head = game.chains[&chain].parts[0].id;
        game.step(DT, Input::default());
        assert_eq!(game.chain_pool(chain).unwrap().left, 1.0);
        // Hurt the head by a third of the pool.
        let total = game.chain_pool(chain).unwrap().total;
        game.bodies
            .iter_mut()
            .find(|b| b.id == head)
            .unwrap()
            .health -= total * 0.4;
        game.step(DT, Input::default());
        let left = game.chain_pool(chain).unwrap().left;
        assert!((0.55..0.65).contains(&left), "{left}");
        assert!(game.chains[&chain].len() < 6, "tail pieces were shed");
        let drifting: Vec<&Body> = game.bodies.iter().filter(|b| b.adrift > 0.0).collect();
        assert!(!drifting.is_empty());
        assert!(drifting.iter().all(|b| b.chain.is_none()
            && b.genome.contact_damage == 0.0
            && b.genome.weapon == Weapon::None));
        // The head itself is whole again: no part dies alone.
        assert_eq!(
            game.body(head).unwrap().health,
            game.body(head).unwrap().max_health
        );
        for _ in 0..(DEFAULT_TUNING.pool_drift / DT) as usize + 5 {
            game.step(DT, Input::default());
        }
        assert!(game.bodies.iter().all(|b| b.adrift <= 0.0));
        let alive = game
            .bodies
            .iter()
            .filter(|b| {
                b.kind == BodyKind::Creature && b.position.distance(Vec2::new(300.0, 0.0)) < 600.0
            })
            .count();
        assert_eq!(alive, game.chains[&chain].len());
    }

    #[test]
    fn the_kill_pays_one_bounty_scaled_by_the_pool_and_breaks_the_rest_apart() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        set_player(&mut game, Vec2::new(0.0, -3000.0), Vec2::ZERO);
        let mut g = serpent(8);
        g.bounty = 100.0;
        let chain = place(&mut game, g, 1.0);
        game.step(DT, Input::default());
        let reward = game.chain_pool(chain).unwrap().reward;
        assert!(reward > 1.5 && reward < 3.5, "{reward}");
        let ids: Vec<u64> = game.chains[&chain].parts.iter().map(|p| p.id).collect();
        for id in &ids {
            game.bodies.iter_mut().find(|b| b.id == *id).unwrap().health = 0.0;
        }
        let before = game.score;
        game.step(DT, Input::default());
        let earned = game.score - before;
        assert!(
            (earned as f32 - 100.0 * reward).abs() < 2.0,
            "one bounty times the pool: {earned} vs {reward}"
        );
        assert!(game.chain_pool(chain).is_none());
        // Everything else is drifting, not dead on the spot.
        assert!(game.bodies.iter().filter(|b| b.adrift > 0.0).count() >= 6);
    }

    #[test]
    fn deep_chains_and_elders_keep_every_part_a_life_of_its_own() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        set_player(&mut game, Vec2::new(0.0, -3000.0), Vec2::ZERO);
        let chain = place(&mut game, serpent(6), 5.0);
        let part = game.chains[&chain].parts[3].id;
        game.bodies
            .iter_mut()
            .find(|b| b.id == part)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(game.chain_pool(chain).is_none());
        assert_eq!(game.chains[&chain].len(), 5);
    }
}
