//! Builder creatures (workstream 11, slice one). A creature whose genome has a `builder`
//! section follows a blueprint (`StructurePlan`, grown from its grammar genes) and places
//! one block per work interval, in plan index order, so a child block never exists before
//! the block it hangs from. Blocks are pinned rocks of the builder's material: ordinary
//! bodies that collide, can be shot or mined, and unload with their sector.
//!
//! Slice one stays where the builder stands: the structure is laid out around the place the
//! builder first worked from, and building waits while the builder is out of reach or the
//! site is occupied. Steering to gather and walk the site, nests as a species, civilization
//! construction and player interaction are later slices (`docs/WORKSTREAMS.md`, 11).

use super::*;
use crate::builder::Builder;
use crate::grammar::entity_key;
use crate::structure::StructurePlan;

/// Most structures under construction at once, across the loaded world.
pub const MAX_WORKS: usize = 6;
/// A builder places a block only when it is within this of the site.
pub const REACH: f32 = 260.0;

/// One structure in progress: its blueprint, where it stands and how far it has got.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Work {
    pub builder: u64,
    pub plan: StructurePlan,
    pub origin: Vec2,
    /// The next site to place; the sites before it are laid.
    pub cursor: usize,
    /// Seconds until the next block may be placed.
    pub clock: f32,
    /// The blocks laid so far, in order.
    pub placed: Vec<u64>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct BuildState {
    pub works: Vec<Work>,
}

impl Game {
    /// Starts structures for builders that have none, then lets every builder with a
    /// structure in progress place its next block when its clock runs out.
    pub(super) fn update_builders(&mut self, dt: f32) {
        // Structures whose builder is gone are left as they stand.
        let alive: Vec<u64> = self
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature && b.genome.builder.is_some())
            .map(|b| b.id)
            .collect();
        self.builds.works.retain(|w| alive.contains(&w.builder));

        let starters: Vec<(u64, Vec2, Builder, u64)> = self
            .bodies
            .iter()
            .filter(|b| can_build(b))
            .filter(|b| !self.builds.works.iter().any(|w| w.builder == b.id))
            .filter_map(|b| {
                let key = b
                    .origin
                    .map_or(b.id, |(sector, index)| entity_key(sector, index));
                Some((b.id, b.position, b.genome.builder?, key))
            })
            .collect();
        for (id, at, builder, key) in starters {
            if self.builds.works.len() >= MAX_WORKS {
                break;
            }
            let plan = builder.blueprint(self.seed, key);
            self.builds.works.push(Work {
                builder: id,
                clock: plan.sites.first().map_or(0.0, |s| builder.work(s.reach)),
                plan,
                origin: at,
                cursor: 0,
                placed: Vec::new(),
            });
        }

        for w in 0..self.builds.works.len() {
            let (id, cursor, origin) = {
                let work = &mut self.builds.works[w];
                work.clock = (work.clock - dt).max(0.0);
                (work.builder, work.cursor, work.origin)
            };
            let work = &self.builds.works[w];
            let Some(site) = work.plan.sites.get(cursor).copied() else {
                continue;
            };
            if work.clock > 0.0 {
                continue;
            }
            let Some(body) = self.body(id).filter(|b| can_build(b)) else {
                continue;
            };
            let Some(builder) = body.genome.builder else {
                continue;
            };
            let at = origin + site.offset;
            let radius = Builder::block_radius(site.radius);
            if body.position.distance(at) > REACH || self.site_occupied(at, radius, &work.placed) {
                continue;
            }
            let (toughness, density) = builder.material.toughness_density();
            let mut block = self.make_body(BodyKind::Asteroid, at);
            block.rock = builder.material.rock();
            block.radius = radius;
            block.health = radius * 80.0 / 35.0 * toughness;
            block.max_health = block.health;
            block.mass = 25.0 * (radius / 35.0).powi(2) * density;
            block.pinned = true;
            let block_id = block.id;
            self.bodies.push(block);
            let next = builder.work(
                self.builds.works[w]
                    .plan
                    .sites
                    .get(cursor + 1)
                    .map_or(0.0, |s| s.reach),
            );
            let work = &mut self.builds.works[w];
            work.placed.push(block_id);
            work.cursor += 1;
            work.clock = next;
        }
    }

    /// Whether a block of `radius` at `at` would overlap a body other than the structure's own
    /// blocks (`own`, which overlap by design).
    fn site_occupied(&self, at: Vec2, radius: f32, own: &[u64]) -> bool {
        self.bodies.iter().any(|b| {
            b.active && !own.contains(&b.id) && b.position.distance(at) < b.radius + radius
        })
    }
}

/// An adult builder that is up and about.
fn can_build(body: &Body) -> bool {
    body.active
        && body.kind == BodyKind::Creature
        && body.genome.builder.is_some()
        && body.health > 0.0
        && !body.phased
        && body.panic <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species};

    const DT: f32 = 1.0 / 60.0;

    /// A quiet game with one builder far from the ship, standing still.
    fn builder_game(genome: Genome, seed: u64) -> (Game, u64) {
        let mut game = Game::new(seed);
        game.bodies.retain(|b| b.kind == BodyKind::Player);
        let species = Species::of(genome);
        let at = Vec2::new(0.0, 1200.0);
        let mut body = game.make_creature(&species, at);
        body.velocity = Vec2::ZERO;
        let id = body.id;
        game.bodies.push(body);
        (game, id)
    }

    fn blocks(game: &Game) -> Vec<&Body> {
        game.bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Asteroid && b.pinned && b.origin.is_none())
            .collect()
    }

    fn run(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            game.update_builders(DT);
        }
    }

    #[test]
    fn a_builder_places_blocks_over_time_following_the_plan_in_index_order() {
        let (mut game, id) = builder_game(Genome::builder(), 5);
        game.update_builders(DT);
        let work = game.builds.works[0].clone();
        assert_eq!(work.builder, id);
        assert!(work.plan.len() >= 8);
        // Nothing yet, then blocks arrive one at a time at the planned places.
        assert!(blocks(&game).is_empty());
        // Blocks arrive one at a time, spread over time, never all at once.
        let mut arrivals = vec![];
        for step in 0..(400.0 / DT) as usize {
            let before = game.builds.works[0].placed.len();
            game.update_builders(DT);
            let after = game.builds.works[0].placed.len();
            assert!(after <= before + 1, "two blocks in one step");
            if after > before {
                arrivals.push(step);
            }
        }
        assert_eq!(arrivals.len(), work.plan.len());
        assert!(arrivals.windows(2).all(|w| w[1] > w[0] + 10), "spread out");
        let done = &game.builds.works[0];
        assert_eq!(done.cursor, done.placed.len());
        for (i, block_id) in done.placed.iter().enumerate() {
            let block = game.body(*block_id).expect("block exists");
            let site = done.plan.sites[i];
            assert!(block.position.distance(done.origin + site.offset) < 0.01);
            assert_eq!(block.rock, RockKind::Plain);
            // A parent block was laid before its child.
            if let Some(p) = site.parent {
                assert!(done.placed[..i].contains(&done.placed[p as usize]));
            }
        }
        assert!(done.placed.len() > 1);
    }

    #[test]
    fn the_whole_structure_is_laid_by_the_time_the_work_is_done_and_no_more() {
        let (mut game, _) = builder_game(Genome::builder(), 5);
        run(&mut game, 400.0);
        let work = &game.builds.works[0];
        assert!(work.cursor <= work.plan.len());
        // Blocked sites only delay; with a clear field the structure completes.
        assert_eq!(work.placed.len(), work.plan.len());
        let laid = blocks(&game).len();
        run(&mut game, 100.0);
        assert_eq!(
            blocks(&game).len(),
            laid,
            "a finished structure grows no more"
        );
    }

    #[test]
    fn a_builder_out_of_reach_waits_and_an_occupied_site_delays() {
        let (mut game, id) = builder_game(Genome::builder(), 5);
        game.update_builders(DT);
        let site = game.builds.works[0].plan.sites[0];
        let origin = game.builds.works[0].origin;
        // Carry the builder away: nothing is placed.
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .position += Vec2::new(5000.0, 0.0);
        run(&mut game, 60.0);
        assert!(blocks(&game).is_empty());
        // Bring it back with a rock sitting on the first site: still nothing, until it leaves.
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .position = origin;
        let mut rock = game.make_body(BodyKind::Asteroid, origin + site.offset);
        rock.radius = 30.0;
        let rock_id = rock.id;
        game.bodies.push(rock);
        run(&mut game, 60.0);
        assert!(blocks(&game).is_empty());
        game.bodies.retain(|b| b.id != rock_id);
        run(&mut game, 60.0);
        assert!(!blocks(&game).is_empty());
    }

    #[test]
    fn building_is_deterministic_by_seed_and_the_seed_shapes_the_structure() {
        let layout = |seed| {
            let (mut game, _) = builder_game(Genome::builder(), seed);
            run(&mut game, 400.0);
            let origin = game.builds.works[0].origin;
            blocks(&game)
                .iter()
                .map(|b| (b.position - origin).to_array().map(f32::to_bits))
                .collect::<Vec<_>>()
        };
        assert_eq!(layout(5), layout(5));
        assert_ne!(layout(5), layout(6));
    }

    #[test]
    fn other_materials_lay_other_rock_and_the_work_count_is_capped() {
        let mut ore = Genome::builder();
        ore.builder.as_mut().unwrap().material = crate::builder::Material::Ore;
        let (mut game, _) = builder_game(ore, 5);
        run(&mut game, 30.0);
        assert!(blocks(&game).iter().all(|b| b.rock == RockKind::Ore));
        // Many builders: structures in progress never exceed the cap.
        let species = Species::of(Genome::builder());
        for k in 0..20 {
            let at = Vec2::new(2000.0 + 400.0 * k as f32, 1200.0);
            let body = game.make_creature(&species, at);
            game.bodies.push(body);
        }
        run(&mut game, 5.0);
        assert!(game.builds.works.len() <= MAX_WORKS);
    }

    #[test]
    fn juveniles_and_the_dead_do_not_build_and_ordinary_creatures_never_start_works() {
        let species = Species::of(Genome::builder());
        let mut game = Game::new(5);
        game.bodies.retain(|b| b.kind == BodyKind::Player);
        let mut young = game.make_creature(
            &Species::of(species.genome.juvenile()),
            Vec2::new(0.0, 1200.0),
        );
        young.velocity = Vec2::ZERO;
        game.bodies.push(young);
        let mut dead = game.make_creature(&species, Vec2::new(800.0, 1200.0));
        dead.health = 0.0;
        game.bodies.push(dead);
        run(&mut game, 60.0);
        assert!(game.builds.works.is_empty() && blocks(&game).is_empty());
        let mut plain = Game::new(5);
        for _ in 0..600 {
            plain.step(DT, Input::default());
        }
        assert!(plain.builds.works.is_empty());
    }

    #[test]
    fn a_spawned_builder_raises_a_structure_in_a_running_game() {
        let mut game = Game::new(5);
        game.dev_change(crate::simulation::dev::DevRow::Invulnerable, 0);
        game.dev_spawn("builder");
        for _ in 0..(300.0 / DT) as usize {
            game.step(DT, Input::default());
        }
        assert!(!game.builds.works.is_empty(), "a work was started");
        let laid: usize = game.builds.works.iter().map(|w| w.placed.len()).sum();
        assert!(laid > 0, "blocks were laid in a running game");
    }
}
