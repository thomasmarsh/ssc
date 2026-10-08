//! Builder creatures (workstream 11, slice one). A creature whose genome has a `builder`
//! section follows a blueprint (`StructurePlan`, grown from its grammar genes) and places
//! one block per work interval, in plan index order, so a child block never exists before
//! the block it hangs from. Blocks are pinned rocks of the builder's material: ordinary
//! bodies that collide, can be shot or mined, and are remembered (`Structure`, saved and restored
//! block by block) when their sector unloads or the game is saved.
//!
//! Civilizations build too (slice three, `update_civ_builders`): a rank-and-file member who
//! is not mining lays a structure from the territory's own gene set, near where it stands,
//! by the same machinery. The gene set and plan are the territory's, not the worker's, so a
//! civilization has a recognisable building style and the nth structure is the same whoever
//! lays it. Blocks stay plain pinned rocks; those of a civilization wear its tint.
//!
//! A builder lays its structure around the place it first worked from and walks it as it rises
//! (the leash target is the next site, `builder_homes`); building waits while the builder is
//! out of reach or a fixed body sits on the site. Gathering rocks as a visible step and
//! player interaction are later slices (`docs/WORKSTREAMS.md`, 11).

use super::*;
use crate::builder::{Builder, Material};
use crate::grammar::entity_key;
use crate::structure::StructurePlan;
use crate::territory::{CivRole, Standing};
use crate::world::SectorId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// The identity of a structure across unloads and saves: the spawn of the wild builder that
/// raised it, or a territory and the ordinal of the structure it raised.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StructureKey {
    Wild(SectorId, u32),
    Civ(u64, u8),
}

/// One block of a kept structure, by site index in its plan.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Block {
    pub at: Vec2,
    pub radius: f32,
    pub health: f32,
}

/// What survives of a structure when its blocks leave the world (a sector unloads) or the
/// game is saved: where it stands, how far it got and which blocks are still whole. Blocks
/// that are not listed were destroyed. The plan is not kept: a wild builder regrows it from
/// its genome and spawn, and a finished block needs only its place and health.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Structure {
    pub material: Material,
    /// The territory that raised it, for tinting.
    pub civ: Option<u64>,
    pub origin: Vec2,
    pub cursor: u16,
    pub done: bool,
    pub blocks: BTreeMap<u16, Block>,
}

/// Most structures under construction at once, across the loaded world.
pub const MAX_WORKS: usize = 6;
/// A builder places a block only when it is within this of the site.
pub const REACH: f32 = 260.0;
/// A wandering builder is drawn back toward its site beyond this distance (see `home_pull`).
pub const LEASH: f32 = 130.0;
/// Most civilization structures under construction at once, across the loaded world (their
/// own cap, so civilizations never starve wild builders of slots, nor the reverse).
pub const MAX_CIV_WORKS: usize = 4;
/// Structures one civilization starts in a session: a few landmarks, not an endless sprawl.
pub const CIV_STRUCTURES: u8 = 3;
/// Salt for a territory's building style.
const CIV_SALT: u64 = 0xC171_B01D_0000_0051;
/// Seconds a site may stay blocked before the builder gives the structure up as it stands.
pub const STALL: f32 = 90.0;

/// One structure in progress: its blueprint, where it stands and how far it has got.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Work {
    pub builder: u64,
    /// Its identity for saving; none for a builder with no spawn (a bred one, a dev spawn).
    pub key: Option<StructureKey>,
    /// The genes it follows: the builder's own, or its territory's style.
    pub gene: Builder,
    /// The territory that raises it, for a civilization's work.
    pub civ: Option<u64>,
    pub plan: StructurePlan,
    pub origin: Vec2,
    /// The next site to place; the sites before it are laid.
    pub cursor: usize,
    /// Seconds until the next block may be placed.
    pub clock: f32,
    /// The blocks laid so far, in order.
    pub placed: Vec<u64>,
    /// Seconds the next site has been waiting (out of reach or occupied).
    pub stalled: f32,
    /// Finished or given up: nothing more is placed and it no longer counts against
    /// `MAX_WORKS` or tethers the builder.
    pub done: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct BuildState {
    pub works: Vec<Work>,
    /// Structures each territory has started this session (the budget `CIV_STRUCTURES`).
    pub civ_started: std::collections::HashMap<u64, u8>,
    /// Every structure the world has seen, as of the last sync: the memory that outlives
    /// unloaded sectors and the game. See `Game::sync_structures`.
    pub kept: BTreeMap<StructureKey, Structure>,
}

impl Game {
    /// Starts structures for builders that have none, then lets every builder with a
    /// structure in progress place its next block when its clock runs out.
    pub(super) fn update_builders(&mut self, dt: f32) {
        // Structures whose builder is gone are left as they stand.
        let alive: Vec<u64> = self
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature)
            .map(|b| b.id)
            .collect();
        if self
            .builds
            .works
            .iter()
            .any(|w| !alive.contains(&w.builder))
        {
            self.sync_structures();
        }
        self.builds.works.retain(|w| alive.contains(&w.builder));

        let starters: Vec<(u64, Vec2, Builder, u64, Option<StructureKey>)> = self
            .bodies
            .iter()
            .filter(|b| can_build(b, false) && b.genome.builder.is_some())
            .filter(|b| !self.builds.works.iter().any(|w| w.builder == b.id))
            .filter_map(|b| {
                let key = b
                    .origin
                    .map_or(b.id, |(sector, index)| entity_key(sector, index));
                let kept = b
                    .origin
                    .map(|(sector, index)| StructureKey::Wild(sector, index));
                Some((b.id, b.position, b.genome.builder?, key, kept))
            })
            .collect();
        for (id, at, builder, key, kept) in starters {
            if self
                .builds
                .works
                .iter()
                .filter(|w| !w.done && w.civ.is_none())
                .count()
                >= MAX_WORKS
            {
                break;
            }
            let plan = builder.blueprint(self.seed, key);
            // A builder that reloads picks its structure up where it was left (or leaves a
            // finished one alone) instead of raising a second one.
            let resumed = kept
                .and_then(|k| self.builds.kept.get(&k).map(|s| (k, s.clone())))
                .filter(|(_, s)| s.cursor as usize <= plan.len());
            let (origin, cursor, done) = resumed.as_ref().map_or((at, 0, false), |(_, s)| {
                (s.origin, s.cursor as usize, s.done)
            });
            let placed = resumed.as_ref().map_or_else(Vec::new, |(k, _)| {
                (0..cursor).map(|i| self.block_id(*k, i as u16)).collect()
            });
            let finished = cursor >= plan.len();
            self.builds.works.push(Work {
                builder: id,
                key: kept,
                gene: builder,
                civ: None,
                clock: plan
                    .sites
                    .get(cursor)
                    .map_or(0.0, |s| builder.work(s.reach)),
                plan,
                origin,
                cursor,
                placed,
                stalled: 0.0,
                done: done || finished,
            });
        }

        self.update_civ_builders();

        for w in 0..self.builds.works.len() {
            let (id, cursor, origin) = {
                let work = &mut self.builds.works[w];
                work.clock = (work.clock - dt).max(0.0);
                (work.builder, work.cursor, work.origin)
            };
            let work = &mut self.builds.works[w];
            let Some(site) = work.plan.sites.get(cursor).copied() else {
                work.done = true;
                continue;
            };
            if work.done || work.clock > 0.0 {
                continue;
            }
            let builder = self.builds.works[w].gene;
            let civ = self.builds.works[w].civ.is_some();
            let Some(body) = self
                .body(id)
                .filter(|b| can_build(b, civ) && b.active && b.kind == BodyKind::Creature)
            else {
                continue;
            };
            let at = origin + site.offset;
            let radius = Builder::block_radius(site.radius);
            let work = &self.builds.works[w];
            if body.position.distance(at) > REACH || self.site_occupied(at, radius, &work.placed) {
                let work = &mut self.builds.works[w];
                work.stalled += dt;
                work.done = work.stalled >= STALL;
                continue;
            }
            let mut block = self.block_body(builder.material, at, radius);
            let block_id = block.id;
            block.structure = self.builds.works[w].key.map(|k| (k, cursor as u16));
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
            work.stalled = 0.0;
            work.done = work.cursor >= work.plan.len();
        }
    }

    /// Civilization construction: a rank-and-file member of a living territory who is not
    /// mining and has no work of its own starts the territory's next structure, one at a
    /// time per territory, up to `CIV_STRUCTURES` a session and `MAX_CIV_WORKS` at once.
    fn update_civ_builders(&mut self) {
        let live = |g: &Game| {
            g.builds
                .works
                .iter()
                .filter(|w| !w.done && w.civ.is_some())
                .count()
        };
        if live(self) >= MAX_CIV_WORKS {
            return;
        }
        let mut starts: Vec<(u64, u64, Vec2)> = Vec::new();
        for body in self
            .bodies
            .iter()
            .filter(|b| !b.follower && can_build(b, true))
        {
            let Some((tid, CivRole::Member)) = self.civ_of(body) else {
                continue;
            };
            let n = self.builds.civ_started.get(&tid).copied().unwrap_or(0);
            let busy = self
                .builds
                .works
                .iter()
                .any(|w| w.civ == Some(tid) && !w.done)
                || starts.iter().any(|s| s.0 == tid);
            let digging = self
                .civ_mining
                .get(&tid)
                .is_some_and(|m| m.miners.iter().any(|m| m.body == body.id));
            let has_work = self.builds.works.iter().any(|w| w.builder == body.id);
            if n >= CIV_STRUCTURES
                || busy
                || digging
                || has_work
                || self.civ_standing(tid) == Standing::Fallen
            {
                continue;
            }
            // Only where the whole plan fits among the walls and rocks already there.
            let gene = civ_style(tid);
            let plan = gene.blueprint(self.seed, civ_key(tid, n));
            if self.plan_blocked(body.position, &plan) > 0 {
                continue;
            }
            starts.push((tid, body.id, body.position));
        }
        for (tid, id, at) in starts {
            if live(self) >= MAX_CIV_WORKS {
                break;
            }
            let n = self.builds.civ_started.entry(tid).or_insert(0);
            let key = civ_key(tid, *n);
            *n += 1;
            let gene = civ_style(tid);
            let plan = gene.blueprint(self.seed, key);
            let ordinal = *self.builds.civ_started.get(&tid).unwrap_or(&1) - 1;
            self.builds.works.push(Work {
                builder: id,
                key: Some(StructureKey::Civ(tid, ordinal)),
                gene,
                civ: Some(tid),
                clock: plan.sites.first().map_or(0.0, |s| gene.work(s.reach)),
                plan,
                origin: at,
                cursor: 0,
                placed: Vec::new(),
                stalled: 0.0,
                done: false,
            });
        }
    }

    /// How many sites of `plan`, laid out around `origin`, sit on a fixed body (a wall, a
    /// planetoid, a rock). Creatures do not count: they move off.
    fn plan_blocked(&self, origin: Vec2, plan: &StructurePlan) -> usize {
        plan.sites
            .iter()
            .filter(|site| {
                let at = origin + site.offset;
                let radius = Builder::block_radius(site.radius);
                self.bodies.iter().any(|b| {
                    b.active
                        && b.kind != BodyKind::Creature
                        && b.kind != BodyKind::Player
                        && b.position.distance(at) < b.radius + radius
                })
            })
            .count()
    }

    /// The middle of the structure with the most blocks laid (finished or not), for bounded
    /// renders that need the camera beside a structure (`SSC_AT_STRUCTURE`).
    pub fn structure_focus(&self) -> Option<Vec2> {
        let work = self.builds.works.iter().max_by_key(|w| w.placed.len())?;
        let laid: Vec<Vec2> = work
            .placed
            .iter()
            .filter_map(|id| self.body(*id).map(|b| b.position))
            .collect();
        (!laid.is_empty()).then(|| laid.iter().sum::<Vec2>() / laid.len() as f32)
    }

    /// The tint of a civilization's block, if `id` is one.
    pub(super) fn civ_block_tint(&self, id: u64) -> Option<[f32; 3]> {
        let (key, _) = self.body(id)?.structure?;
        let StructureKey::Civ(tid, _) = key else {
            return None;
        };
        self.civ_colors.get(&tid).copied()
    }

    /// A block of `material` at `at`: a pinned rock, tough and heavy as the material makes it.
    fn block_body(&mut self, material: Material, at: Vec2, radius: f32) -> Body {
        let (toughness, density) = material.toughness_density();
        let mut block = self.make_body(BodyKind::Asteroid, at);
        block.rock = material.rock();
        block.radius = radius;
        block.health = radius * 80.0 / 35.0 * toughness;
        block.max_health = block.health;
        block.mass = 25.0 * (radius / 35.0).powi(2) * density;
        block.pinned = true;
        block
    }

    /// The id of the live block at `site` of structure `key`, or none (`u64::MAX`, which no
    /// body has) when it is destroyed or its sector is not loaded.
    fn block_id(&self, key: StructureKey, site: u16) -> u64 {
        self.bodies
            .iter()
            .find(|b| b.structure == Some((key, site)))
            .map_or(u64::MAX, |b| b.id)
    }

    /// Every structure as it stands now: the kept memory overlaid with the live works and the
    /// live blocks. A kept block whose sector is loaded but which has no body was destroyed
    /// and is dropped; one in an unloaded sector keeps its last state. Pure.
    pub(super) fn structures_snapshot(&self) -> BTreeMap<StructureKey, Structure> {
        let mut map = self.builds.kept.clone();
        for w in &self.builds.works {
            let Some(key) = w.key else { continue };
            let s = map.entry(key).or_insert_with(|| Structure {
                material: w.gene.material,
                civ: w.civ,
                origin: w.origin,
                cursor: 0,
                done: false,
                blocks: BTreeMap::new(),
            });
            s.origin = w.origin;
            s.cursor = w.cursor as u16;
            s.done = w.done;
        }
        let mut live: HashSet<(StructureKey, u16)> = HashSet::new();
        for b in &self.bodies {
            let Some((key, site)) = b.structure else {
                continue;
            };
            if let Some(s) = map.get_mut(&key) {
                s.blocks.insert(
                    site,
                    Block {
                        at: b.position,
                        radius: b.radius,
                        health: b.health,
                    },
                );
                live.insert((key, site));
            }
        }
        for (key, s) in &mut map {
            s.blocks.retain(|site, b| {
                live.contains(&(*key, *site)) || !self.loaded.contains(&SectorId::containing(b.at))
            });
        }
        map
    }

    /// Writes the current structures into the kept memory; called before the blocks can
    /// leave the world (a sector unloads, a builder is lost).
    pub(super) fn sync_structures(&mut self) {
        self.builds.kept = self.structures_snapshot();
    }

    /// Raises again the kept blocks that lie in sector `id`, which has just loaded.
    pub(super) fn restore_structures(&mut self, id: SectorId) {
        let live: HashSet<(StructureKey, u16)> =
            self.bodies.iter().filter_map(|b| b.structure).collect();
        let due: Vec<(StructureKey, u16, Material, Block)> = self
            .builds
            .kept
            .iter()
            .flat_map(|(key, s)| {
                s.blocks
                    .iter()
                    .filter(|(site, b)| {
                        SectorId::containing(b.at) == id && !live.contains(&(*key, **site))
                    })
                    .map(|(site, b)| (*key, *site, s.material, *b))
            })
            .collect();
        for (key, site, material, saved) in due {
            let mut block = self.block_body(material, saved.at, saved.radius);
            block.health = saved.health.min(block.max_health);
            block.structure = Some((key, site));
            self.bodies.push(block);
        }
    }

    /// Where each builder with a structure in progress works: the next site, so a builder
    /// walks its structure as it rises and stays within reach of every block (see `home_pull`).
    pub(super) fn builder_homes(&self) -> std::collections::HashMap<u64, Vec2> {
        self.builds
            .works
            .iter()
            .filter(|w| !w.done)
            .map(|w| {
                let site = w.plan.sites.get(w.cursor).map_or(Vec2::ZERO, |s| s.offset);
                (w.builder, w.origin + site)
            })
            .collect()
    }

    /// Whether a block of `radius` at `at` would overlap a body other than the structure's own
    /// blocks (`own`, which overlap by design). Creatures do not count: they are pushed clear
    /// of a block, and a crowd around a civilization's worker would otherwise stall it.
    fn site_occupied(&self, at: Vec2, radius: f32, own: &[u64]) -> bool {
        self.bodies.iter().any(|b| {
            b.active
                && b.kind != BodyKind::Creature
                && !own.contains(&b.id)
                && b.position.distance(at) < b.radius + radius
        })
    }
}

/// The identity of a territory's `n`th structure, for its blueprint stream.
fn civ_key(territory: u64, n: u8) -> u64 {
    territory.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ u64::from(n)
}

/// A territory's building style: a gene set fixed by its identity (so every structure it
/// raises shares material, grammar and scale), made a little more industrious than a nest
/// builder (patience at most 5 s) and at least 14 blocks big, so a city has presence.
pub fn civ_style(territory: u64) -> Builder {
    let mut b = Builder::from_hash(territory ^ CIV_SALT);
    b.patience = b.patience.min(5.0);
    b.blocks = b.blocks.max(14);
    b.limited()
}

/// The steering a builder with a structure in progress adds to its wandering: nothing within
/// `LEASH` of its site, then a pull back that grows with the distance, to twice its cruise
/// speed. It keeps the builder within `REACH` so it can place its blocks, without pinning it.
pub(super) fn home_pull(at: Vec2, home: Vec2, cruise: f32) -> Vec2 {
    let gap = at.distance(home);
    if !gap.is_finite() || gap <= LEASH {
        return Vec2::ZERO;
    }
    (home - at) / gap * cruise * ((gap - LEASH) / LEASH).min(2.0)
}

/// A creature that is up and about and may lay a block. A civilization's worker also stops
/// while it is alert (a fight comes first).
fn can_build(body: &Body, civ: bool) -> bool {
    body.active
        && body.kind == BodyKind::Creature
        && body.health > 0.0
        && !body.phased
        && body.panic <= 0.0
        && !(civ && body.alert)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::sectormap::GENERATOR_VERSION;
    use crate::simulation::save::SaveState;

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
        run(&mut game, 25.0);
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
        run(&mut game, 25.0);
        assert!(blocks(&game).is_empty());
        game.bodies.retain(|b| b.id != rock_id);
        run(&mut game, 25.0);
        assert!(!blocks(&game).is_empty());
    }

    #[test]
    fn a_blocked_structure_is_given_up_and_frees_its_slot() {
        let (mut game, id) = builder_game(Genome::builder(), 5);
        game.update_builders(DT);
        let site = game.builds.works[0].plan.sites[0];
        let origin = game.builds.works[0].origin;
        let mut rock = game.make_body(BodyKind::Asteroid, origin + site.offset);
        rock.radius = 30.0;
        game.bodies.push(rock);
        run(&mut game, STALL - 10.0);
        assert!(!game.builds.works[0].done);
        assert!(game.builder_homes().contains_key(&id));
        run(&mut game, 20.0);
        assert!(game.builds.works[0].done);
        assert!(game.builder_homes().is_empty(), "no longer tethered");
        assert!(blocks(&game).is_empty());
    }

    #[test]
    fn a_finished_structure_no_longer_counts_against_the_cap() {
        let species = Species::of(Genome::builder());
        let (mut game, _) = builder_game(Genome::builder(), 5);
        run(&mut game, 400.0);
        assert!(game.builds.works[0].done);
        for k in 0..MAX_WORKS {
            let mut body =
                game.make_creature(&species, Vec2::new(900.0 + 300.0 * k as f32, 3000.0));
            body.velocity = Vec2::ZERO;
            game.bodies.push(body);
        }
        game.update_builders(DT);
        let live = game.builds.works.iter().filter(|w| !w.done).count();
        assert_eq!(live, MAX_WORKS);
    }

    #[test]
    fn the_leash_pulls_a_wanderer_home_and_leaves_a_near_builder_alone() {
        let home = Vec2::new(100.0, 100.0);
        assert_eq!(
            home_pull(home + Vec2::new(LEASH - 1.0, 0.0), home, 40.0),
            Vec2::ZERO
        );
        let pull = home_pull(home + Vec2::new(LEASH * 2.0, 0.0), home, 40.0);
        assert!(pull.x < 0.0 && pull.y.abs() < 1e-3);
        assert!(pull.length() <= 80.0 + 1e-3);
        assert!(home_pull(home + Vec2::new(1.0e9, 0.0), home, 40.0).length() <= 80.0 + 1e-3);
        assert_eq!(home_pull(Vec2::NAN, home, 40.0), Vec2::ZERO);
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

    /// The whole chain as a player meets it: fly to a sector where a wild nest builder species
    /// lives and, after a while, creatures there have raised structures of pinned blocks.
    #[test]
    fn wild_nest_builders_raise_structures_in_their_own_sectors() {
        let seed = crate::config::MASTER_SEED;
        let id = (-40..=40)
            .flat_map(|x| (-40..=40).map(move |y| crate::world::SectorId { x, y }))
            .filter(|id| crate::range::ring(*id) >= 5)
            .find(|id| {
                crate::range::ecology(seed, *id)
                    .presence
                    .iter()
                    .any(|p| p.species.genome.builder.is_some() && p.weight > 0.3)
            })
            .expect("a sector with a nest builder");
        let mut game = Game::new(seed);
        game.teleport(id.center());
        for _ in 0..(120.0 / DT) as usize {
            game.step(DT, Input::default());
        }
        let builders = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature && b.genome.builder.is_some())
            .count();
        assert!(builders > 0, "no builders spawned in {id:?}");
        assert!(!game.builds.works.is_empty(), "no works started");
        let laid: usize = game.builds.works.iter().map(|w| w.placed.len()).sum();
        assert!(laid > 0, "{} works, no blocks", game.builds.works.len());
    }

    #[test]
    fn a_builder_walks_its_structure_staying_within_reach_of_the_next_site() {
        let (mut game, id) = builder_game(Genome::builder(), 5);
        game.update_builders(DT);
        let work = game.builds.works[0].clone();
        assert_eq!(
            game.builder_homes()[&id],
            work.origin + work.plan.sites[0].offset
        );
        run(&mut game, 400.0);
        // Finished: no longer tethered anywhere.
        assert!(game.builder_homes().is_empty());
        let farthest = work
            .plan
            .sites
            .iter()
            .map(|s| s.offset.length())
            .fold(0.0, f32::max);
        assert!(
            farthest > LEASH,
            "plan reaches beyond the leash, so walking matters"
        );
    }

    #[test]
    fn the_structure_focus_is_the_middle_of_the_laid_blocks() {
        let (mut game, _) = builder_game(Genome::builder(), 5);
        assert_eq!(game.structure_focus(), None);
        run(&mut game, 400.0);
        let focus = game.structure_focus().expect("blocks laid");
        let origin = game.builds.works[0].origin;
        assert!(focus.distance(origin) < 400.0);
    }

    fn horde() -> crate::territory::Territory {
        let seed = crate::config::MASTER_SEED;
        for x in -40..=40 {
            for y in -40..=40 {
                if let Some(t) = crate::world::territory(seed, crate::world::SectorId { x, y })
                    && t.capital == (crate::world::SectorId { x, y })
                    && t.shape == crate::territory::CivShape::Horde
                {
                    return t;
                }
            }
        }
        panic!("no horde");
    }

    /// A player's visit to a quiet corner of a horde's capital sector, held for `seconds`.
    fn visit(seconds: f32) -> (Game, crate::territory::Territory) {
        let t = horde();
        let spot = t.capital.center() + Vec2::new(0.0, 2800.0);
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.player_invulnerability = 1e9;
        game.teleport(spot);
        for _ in 0..(seconds / 0.05) as usize {
            crate::simulation::tests::set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
        (game, t)
    }

    #[test]
    fn a_civilization_raises_structures_in_its_own_style_within_every_cap() {
        let (game, t) = visit(240.0);
        let mine: Vec<&Work> = game
            .builds
            .works
            .iter()
            .filter(|w| w.civ == Some(t.id))
            .collect();
        assert!(!mine.is_empty(), "the horde started a structure");
        assert!(mine.len() <= usize::from(CIV_STRUCTURES));
        assert!(
            mine.iter().filter(|w| !w.done).count() <= 1,
            "one at a time"
        );
        assert!(mine.iter().all(|w| w.gene == civ_style(t.id)));
        let laid: usize = mine.iter().map(|w| w.placed.len()).sum();
        assert!(laid > 0, "blocks were laid");
        // Its workers are rank-and-file members, never the miners' rocks or the wild's slots.
        for w in &mine {
            let body = game.body(w.builder).expect("builder lives");
            assert_eq!(game.civ_of(body), Some((t.id, CivRole::Member)));
            assert!(body.genome.builder.is_none());
        }
        let block = game.body(mine[0].placed[0]).unwrap();
        assert!(game.civ_tint(block).is_some(), "blocks wear the tint");
        assert!(
            game.builds
                .works
                .iter()
                .filter(|w| !w.done && w.civ.is_some())
                .count()
                <= MAX_CIV_WORKS
        );
    }

    #[test]
    fn civ_building_is_deterministic_and_each_territory_has_its_own_style() {
        let layout = || {
            let (game, t) = visit(120.0);
            let w = game
                .builds
                .works
                .iter()
                .find(|w| w.civ == Some(t.id))
                .cloned();
            w.map(|w| {
                w.plan
                    .sites
                    .iter()
                    .map(|s| s.offset.to_array().map(f32::to_bits))
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(layout(), layout());
        assert_ne!(civ_style(1), civ_style(2));
        assert_eq!(civ_style(1), civ_style(1).limited());
        assert!(civ_style(7).blocks >= 14);
    }

    #[test]
    fn a_fallen_civilization_starts_nothing() {
        let t = horde();
        let spot = t.capital.center() + Vec2::new(0.0, 2800.0);
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.player_invulnerability = 1e9;
        game.civ_fall.insert(
            t.id,
            crate::territory::Fall {
                capital: true,
                elder: true,
            },
        );
        game.teleport(spot);
        for _ in 0..(120.0 / 0.05) as usize {
            crate::simulation::tests::set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
        assert!(game.builds.works.iter().all(|w| w.civ != Some(t.id)));
    }

    /// Every block of a kept structure now in the world: key, site, place and health, sorted.
    fn laid(game: &Game) -> Vec<(StructureKey, u16, [u32; 2], u32)> {
        let mut out: Vec<_> = game
            .bodies
            .iter()
            .filter_map(|b| {
                let (key, site) = b.structure?;
                Some((
                    key,
                    site,
                    b.position.to_array().map(f32::to_bits),
                    b.health.to_bits(),
                ))
            })
            .collect();
        out.sort_by_key(|(k, s, ..)| (*k, *s));
        out
    }

    fn play(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            game.step(DT, Input::default());
        }
    }

    /// A sector where a wild nest builder lives, and a game that has watched it work there.
    fn wild_scene() -> (Game, crate::world::SectorId) {
        let seed = crate::config::MASTER_SEED;
        let id = (-40..=40)
            .flat_map(|x| (-40..=40).map(move |y| crate::world::SectorId { x, y }))
            .filter(|id| crate::range::ring(*id) >= 5)
            .find(|id| {
                crate::range::ecology(seed, *id)
                    .presence
                    .iter()
                    .any(|p| p.species.genome.builder.is_some() && p.weight > 0.3)
            })
            .expect("a sector with a nest builder");
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(id.center());
        play(&mut game, 150.0);
        (game, id)
    }

    /// Wounds one block and destroys another, returning the destroyed key.
    fn hurt_blocks(game: &mut Game) -> (StructureKey, u16) {
        let tagged: Vec<u64> = game
            .bodies
            .iter()
            .filter(|b| b.structure.is_some())
            .map(|b| b.id)
            .collect();
        assert!(tagged.len() >= 3, "the builders laid blocks");
        let wounded = game.bodies.iter_mut().find(|b| b.id == tagged[0]).unwrap();
        wounded.health *= 0.5;
        let doomed = game.bodies.iter().find(|b| b.id == tagged[1]).unwrap();
        let gone = doomed.structure.unwrap();
        game.bodies.retain(|b| b.id != tagged[1]);
        gone
    }

    #[test]
    fn a_wild_structure_survives_its_sector_unloading_and_reloading() {
        let (mut game, id) = wild_scene();
        let gone = hurt_blocks(&mut game);
        let before = laid(&game);
        let wounded_before = before.len();
        // Fly far away: the sector unloads and its blocks leave the world.
        game.teleport(id.center() + Vec2::new(crate::world::SECTOR_SIZE * 14.0, 0.0));
        play(&mut game, 5.0);
        assert!(
            !laid(&game)
                .iter()
                .any(|(k, ..)| matches!(k, StructureKey::Wild(s, _) if *s == id)),
            "unloaded"
        );
        // Come back: the same blocks stand where they stood, wounded and missing as left.
        game.teleport(id.center());
        game.step(DT, Input::default());
        let after = laid(&game);
        assert!(after.len() >= wounded_before - 1);
        for block in before.iter().filter(|(k, s, ..)| (*k, *s) != gone) {
            assert!(after.contains(block), "block {block:?} came back as it was");
        }
        assert!(
            !after.iter().any(|(k, s, ..)| (*k, *s) == gone),
            "a destroyed block stays destroyed"
        );
        // The builders pick their work up where it was left: no site is laid twice.
        play(&mut game, 200.0);
        let sites: Vec<_> = laid(&game).iter().map(|(k, s, ..)| (*k, *s)).collect();
        let mut unique = sites.clone();
        unique.dedup();
        assert_eq!(sites, unique);
        for key in game.builds.works.iter().filter_map(|w| w.key) {
            let works = game.builds.works.iter().filter(|w| w.key == Some(key));
            assert_eq!(works.count(), 1, "one work per structure");
        }
        assert!(
            !after.iter().any(|(k, s, ..)| (*k, *s) == gone)
                && !laid(&game).iter().any(|(k, s, ..)| (*k, *s) == gone),
            "never rebuilt"
        );
    }

    #[test]
    fn a_wild_structure_survives_save_and_load() {
        let (mut game, id) = wild_scene();
        hurt_blocks(&mut game);
        let before = laid(&game);
        assert!(!before.is_empty());
        let text = game.save_state().to_text();
        let (state, generator) = SaveState::from_text(&text).unwrap();
        let (loaded, report) = Game::from_save(state, generator);
        assert!(report.world_deltas_kept);
        assert_eq!(laid(&loaded), before, "every block, in place, as hurt");
        assert!(
            before
                .iter()
                .any(|(k, ..)| matches!(k, StructureKey::Wild(s, _) if *s == id))
        );
        // The save is a fixed point and does not depend on map order.
        assert_eq!(text, loaded.save_state().to_text());
        // A generator change drops the spawn-keyed structures like every other spawn delta.
        let (state, _) = SaveState::from_text(&text).unwrap();
        let (other, _) = Game::from_save(state, GENERATOR_VERSION + 1);
        assert!(other.builds.kept.is_empty());
    }

    #[test]
    fn a_civilization_structure_and_its_budget_survive_save_and_load() {
        let (game, t) = visit(240.0);
        let before = laid(&game);
        assert!(
            before
                .iter()
                .any(|(k, ..)| matches!(k, StructureKey::Civ(id, _) if *id == t.id)),
            "the horde built something"
        );
        let started = game.builds.civ_started.clone();
        let text = game.save_state().to_text();
        let (state, generator) = SaveState::from_text(&text).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert_eq!(laid(&loaded), before);
        assert_eq!(loaded.builds.civ_started, started, "the budget is kept");
        // Blocks still wear the territory's tint.
        let block = loaded
            .bodies
            .iter()
            .find(|b| b.structure.is_some())
            .unwrap();
        assert!(loaded.civ_block_tint(block.id).is_some());
    }
}
