//! The record of a run (one game, from launch to game over) and species extirpation.
//!
//! **Run stats** (`RunStats`) are plain counters bumped at the existing event sites (kills,
//! mining, shots, damage, eggs, pads, parts). They are deterministic, cost nothing per
//! tick beyond a couple of additions, and reset with the game (`Game::reset` rebuilds it).
//!
//! **Extirpation.** A species' *range* is the connected group of sectors where its lineage
//! is generated. The lineage is a pure function of the seed (`world::generate`), so a range
//! is computed by flood fill from the sector of a kill, cached per lineage, and never
//! changes. A lineage is extirpated from a range when nothing of it remains there:
//!
//! - every generated spawn of the lineage in the range is destroyed (`Game::fallen`; a
//!   husk holding the lineage counts as a spawn of it; a spawn belonging to a fallen
//!   civilization counts as gone), and
//! - no live creature body (adult, juvenile, rooted tenant, trailing part) and no egg of
//!   the lineage sits in a range sector.
//!
//! The check is event-driven: it runs when a creature or an egg of the lineage is lost, never
//! per tick. Each (lineage, range) counts once, and posts a notice and a cue.
//!
//! Limits, by design:
//! - Adjacency is the 8-neighborhood (a shared corner joins sectors), and a range stops
//!   `RANGE_RADIUS` sectors (Chebyshev) from the sector it was first computed from, so
//!   cost is bounded (at most `RANGE_RADIUS * 2 + 1` squared sectors are generated, once).
//!   A truncated range is judged as if it ended there. A flood that touches an
//!   already-cached range of the lineage returns that one, so a range is never counted twice.
//! - Persistence remembers only kills, so a sector that is not loaded is judged by its
//!   generated spawns minus the fallen. Creatures bred or wandering in from outside the
//!   range do not block it (a body counts by where it is). Bases and what they breed do not
//!   count as spawns of the lineage, so a standing base is not what keeps a species alive;
//!   whatever it already bred is, while it sits in the range.

use super::*;
use crate::simulation::upgrades::Rarity;
use std::collections::VecDeque;

/// Sectors the flood fill may stray from where it started (Chebyshev). Matches the lineage
/// lattice cell, so a range spans about one founder's territory.
pub const RANGE_RADIUS: i32 = 8;

/// How long the per-life recap stays up.
pub const RECAP_SECONDS: f32 = 7.0;

/// Cached sector summaries kept before the cache is simply rebuilt.
const SECTOR_CACHE_CAP: usize = 4096;

/// One species-range wiped out.
#[derive(Clone, Debug, PartialEq)]
pub struct Extirpation {
    /// Display name as the HUD shows it (uppercase).
    pub name: String,
    pub lineage: u64,
    /// Sectors in the range.
    pub sectors: u32,
    /// Where the last of it died, and how far from HOME that is.
    pub at: SectorId,
    pub depth: f32,
}

/// Counters for the current run. Everything resets with the game.
#[derive(Clone, Debug, Default)]
pub struct RunStats {
    /// Enemies destroyed (creatures and eggs excluded; see `eggs`), by any hand but hunger.
    pub kills: u32,
    /// Destroyed creatures by lineage: (display name, count), in order of first kill.
    pub by_species: Vec<(u64, String, u32)>,
    /// Boss-like creatures and civilization elders.
    pub elders: u32,
    pub bases: u32,
    pub civs_toppled: u32,
    /// Offerings made at civilization seats, and the material they came to.
    pub tithes: u32,
    pub tithed: f32,
    pub juveniles: u32,
    pub eggs: u32,
    /// Creatures eaten by predators or starved, anywhere near the ship.
    pub lost_to_nature: u32,
    pub sectors: HashSet<SectorId>,
    /// Regions entered (by key), after the hysteresis that keeps a border from flickering.
    pub regions: HashSet<u64>,
    /// Farthest sector depth reached and the deepest threat multiplier faced.
    pub deepest: f32,
    pub threat: f32,
    /// Ore taken by the beam, per material (metal, volatiles, crystal).
    pub mined: [f32; 3],
    pub rocks_depleted: u32,
    pub planetoids_drained: u32,
    pub shots: u32,
    pub damage_dealt: f32,
    pub damage_taken: f32,
    pub deaths: u32,
    pub weapons: u32,
    pub pads: u32,
    pub parts: u32,
    pub distance: f32,
    pub extirpated: Vec<Extirpation>,
    /// Seconds the death recap stays up after losing a ship that was not the last.
    pub recap: f32,
}

impl RunStats {
    /// Records a first visit to a sector (and how deep it is).
    pub fn visit(&mut self, id: SectorId, depth: f32) {
        if self.sectors.insert(id) {
            self.deepest = self.deepest.max(depth);
            self.threat = self.threat.max(world::threat(depth));
        }
    }

    fn tally(&mut self, lineage: u64, name: &str) {
        match self.by_species.iter_mut().find(|s| s.0 == lineage) {
            Some(entry) => entry.2 += 1,
            None => self.by_species.push((lineage, name.to_uppercase(), 1)),
        }
    }

    pub fn total_mined(&self) -> f32 {
        self.mined.iter().sum()
    }
}

/// What one sector generates of each lineage: spawn indices (with the civilization tag, if
/// any, so a fallen civilization's spawns can be discounted).
#[derive(Default)]
struct Residents {
    lineages: HashMap<u64, Vec<(u32, Option<u64>)>>,
}

/// A cached range: sorted sectors, and whether it has been extirpated.
struct Range {
    sectors: Vec<SectorId>,
    done: bool,
}

/// Deterministic caches behind extirpation; pure functions of the seed.
#[derive(Default)]
pub(super) struct Lore {
    residents: HashMap<SectorId, Residents>,
    ranges: HashMap<u64, Vec<Range>>,
}

impl Lore {
    fn residents(&mut self, seed: u64, id: SectorId) -> &Residents {
        if self.residents.len() >= SECTOR_CACHE_CAP {
            self.residents.clear();
        }
        self.residents.entry(id).or_insert_with(|| {
            let mut found = Residents::default();
            for spawn in world::generate(seed, id) {
                let tag = spawn.civ.map(|c| c.territory);
                let species = spawn.species.map(|s| s.lineage);
                let den = spawn.den.map(|(s, _)| s.lineage);
                for lineage in species.into_iter().chain(den) {
                    found
                        .lineages
                        .entry(lineage)
                        .or_default()
                        .push((spawn.index, tag));
                }
            }
            found
        })
    }

    /// Index of the range of `lineage` that contains `from`, computing it on first need.
    /// `None` when the lineage is not generated in that sector at all.
    fn range_of(&mut self, seed: u64, lineage: u64, from: SectorId) -> Option<usize> {
        if let Some(i) = self.ranges.get(&lineage).and_then(|rs| {
            rs.iter()
                .position(|r| r.sectors.binary_search(&from).is_ok())
        }) {
            return Some(i);
        }
        if !self.residents(seed, from).lineages.contains_key(&lineage) {
            return None;
        }
        let mut seen: HashSet<SectorId> = HashSet::from([from]);
        let mut queue = VecDeque::from([from]);
        let mut sectors = Vec::new();
        while let Some(at) = queue.pop_front() {
            sectors.push(at);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let next = SectorId {
                        x: at.x + dx,
                        y: at.y + dy,
                    };
                    if (dx, dy) == (0, 0)
                        || next.x.abs_diff(from.x) > RANGE_RADIUS as u32
                        || next.y.abs_diff(from.y) > RANGE_RADIUS as u32
                        || !seen.insert(next)
                    {
                        continue;
                    }
                    if self.residents(seed, next).lineages.contains_key(&lineage) {
                        queue.push_back(next);
                    }
                }
            }
        }
        sectors.sort();
        let ranges = self.ranges.entry(lineage).or_default();
        // A flood that runs into a cached range is that range.
        if let Some(i) = ranges
            .iter()
            .position(|r| sectors.iter().any(|s| r.sectors.binary_search(s).is_ok()))
        {
            return Some(i);
        }
        ranges.push(Range {
            sectors,
            done: false,
        });
        Some(ranges.len() - 1)
    }

    /// The sectors of the range `lineage` has around `from`, for tests and tools.
    pub(super) fn range(&mut self, seed: u64, lineage: u64, from: SectorId) -> Vec<SectorId> {
        self.range_of(seed, lineage, from)
            .map(|i| self.ranges[&lineage][i].sectors.clone())
            .unwrap_or_default()
    }
}

impl Game {
    /// Called once per step with the ship's sector: counts first visits.
    pub(super) fn note_sector(&mut self) {
        let here = self.sector();
        if !self.run.sectors.contains(&here) {
            let depth = self.params().depth;
            self.run.visit(here, depth);
        }
    }

    /// The sectors of the range a lineage has around `from` (empty if it is not generated
    /// there). Computed deterministically from the seed, cached.
    pub fn species_range(&mut self, lineage: u64, from: SectorId) -> Vec<SectorId> {
        self.lore.range(self.seed, lineage, from)
    }

    /// A creature body is gone. `eaten`: starved or preyed upon rather than destroyed.
    pub(super) fn note_creature_lost(&mut self, body: &Body, eaten: bool) {
        if body.kind != BodyKind::Creature {
            return;
        }
        let name = body.genome.name();
        if eaten {
            self.run.lost_to_nature += 1;
        } else if !body.follower {
            // Trailing segments of a jointed body are not separate kills.
            self.run.kills += 1;
            self.run.tally(body.species, &name);
            if body.adult.is_some() && body.growth < 1.0 {
                self.run.juveniles += 1;
            }
            if self.is_elder(body) || body.genome.hull >= 150.0 || body.genome.bounty >= 250.0 {
                self.run.elders += 1;
            }
        }
        let sector = body
            .origin
            .map_or_else(|| SectorId::containing(body.position), |(q, _)| q);
        self.check_extirpation(body.species, sector, &name);
    }

    /// An egg was broken or spoiled.
    pub(super) fn note_egg_lost(&mut self, egg: &growth::Egg, broken: bool) {
        if broken {
            self.run.eggs += 1;
        }
        let sector = SectorId::containing(egg.position);
        self.check_extirpation(egg.lineage, sector, &egg.adult.name());
    }

    /// Announces the extirpation of `lineage`'s range around `sector` if nothing of it is left.
    fn check_extirpation(&mut self, lineage: u64, sector: SectorId, name: &str) {
        let seed = self.seed;
        let Some(index) = self.lore.range_of(seed, lineage, sector) else {
            return;
        };
        if self.lore.ranges[&lineage][index].done {
            return;
        }
        let sectors = self.lore.ranges[&lineage][index].sectors.clone();
        for &id in &sectors {
            let fallen = self.fallen.get(&id);
            let spawns = self
                .lore
                .residents(seed, id)
                .lineages
                .get(&lineage)
                .cloned()
                .unwrap_or_default();
            for (spawn, civ) in spawns {
                let gone = fallen.is_some_and(|f| f.contains(&spawn))
                    || civ.is_some_and(|t| {
                        self.civ_standing(t) == crate::territory::Standing::Fallen
                    });
                if !gone {
                    return;
                }
            }
        }
        let inside = |at: Vec2| sectors.binary_search(&SectorId::containing(at)).is_ok();
        if self
            .bodies
            .iter()
            .any(|b| b.kind == BodyKind::Creature && b.species == lineage && inside(b.position))
            || self
                .eggs
                .iter()
                .any(|e| e.lineage == lineage && inside(e.position))
        {
            return;
        }
        self.lore.ranges.get_mut(&lineage).expect("range")[index].done = true;
        let name = name.to_uppercase();
        let depth = world::latent(seed, sector).depth;
        self.run.extirpated.push(Extirpation {
            name: name.clone(),
            lineage,
            sectors: sectors.len() as u32,
            at: sector,
            depth,
        });
        self.notify(format!("Species extirpated: {name}"), Rarity::Epic);
        self.cue(Cue::Extirpated);
    }

    /// Distance, damage taken and anything else measured over a whole step.
    pub(super) fn note_step(&mut self, dt: f32, travelled: f32, taken: f32) {
        self.run.recap = (self.run.recap - dt).max(0.0);
        self.run.distance += travelled;
        self.run.damage_taken += taken;
    }

    /// The run as a report for the death and game-over screens.
    pub fn run_report(&self) -> RunReport {
        let r = &self.run;
        let minutes = (self.time / 60.0).floor() as u32;
        let seconds = (self.time % 60.0).floor() as u32;
        let most = r
            .by_species
            .iter()
            .max_by_key(|s| s.2)
            .map(|(_, name, n)| format!("MOST DESTROYED {name} x{n}   "))
            .unwrap_or_default();
        let lines = vec![
            format!(
                "SCORE {}   TIME {minutes}:{seconds:02}   LIVES USED {}",
                self.score, r.deaths
            ),
            format!(
                "SECTORS EXPLORED {}   REGIONS {}   DEEPEST {:.0}   THREAT FACED x{:.1}   FLOWN {:.1}K",
                r.sectors.len(),
                r.regions.len(),
                r.deepest,
                r.threat.max(1.0),
                r.distance / 1000.0
            ),
            format!(
                "DESTROYED {}   ELDERS {}   STATIONS {}   CIVILIZATIONS {}   EGGS {}   JUVENILES {}",
                r.kills, r.elders, r.bases, r.civs_toppled, r.eggs, r.juveniles
            ),
            format!("{most}LOST TO NATURE {}", r.lost_to_nature),
            format!(
                "MINED {:.0}   METAL {:.0}  VOLATILES {:.0}  CRYSTAL {:.0}",
                r.total_mined(),
                r.mined[0],
                r.mined[1],
                r.mined[2]
            ),
            format!(
                "ROCKS WORKED OUT {}   PLANETOIDS DRAINED {}",
                r.rocks_depleted, r.planetoids_drained
            ),
            format!(
                "SHOTS {}   DAMAGE DEALT {:.0}  TAKEN {:.0}",
                r.shots, r.damage_dealt, r.damage_taken
            ),
            format!(
                "WEAPONS {}   PARTS {}   PADS {}",
                r.weapons, r.parts, r.pads
            ),
        ];
        // Nothing here changes the rules; it only reads.
        let extirpated = r
            .extirpated
            .iter()
            .map(|e| {
                let plural = if e.sectors == 1 { "SECTOR" } else { "SECTORS" };
                format!(
                    "{}   ({} {plural}, LAST SEEN AT {}, {})",
                    e.name, e.sectors, e.at.x, e.at.y
                )
            })
            .collect();
        RunReport {
            lines,
            extirpated,
            quip: quip(r.extirpated.len()),
        }
    }
}

/// The wry line under the list. Mild, and deadpan on purpose.
pub fn quip(count: usize) -> &'static str {
    match count {
        0 => "No species were harmed in the making of this run.",
        1 => "One species, gone. It was in the way.",
        2 | 3 => "The local ecologists have been notified. They are not surprised.",
        _ => "At this point it is a policy.",
    }
}

/// What the summary panel shows.
#[derive(Clone, Debug, PartialEq)]
pub struct RunReport {
    pub lines: Vec<String>,
    /// One entry per extirpated species-range, ready to print.
    pub extirpated: Vec<String>,
    pub quip: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    use crate::simulation::upgrades::{Effect, Part as ShipPart, Slot, Stat};

    const GHOST: u64 = 0xFEED_0000_0000_0001;

    fn ghost() -> Species {
        Species {
            lineage: GHOST,
            generation: 0,
            genome: Genome::default(),
        }
    }

    fn sector(x: i32, y: i32) -> SectorId {
        SectorId { x, y }
    }

    /// Declares that `GHOST` is generated at these (sector, spawn index) pairs, ahead of
    /// the real generator, so a test controls the world exactly.
    fn seed_ghosts(game: &mut Game, spawns: &[(i32, i32, u32)]) {
        for &(x, y, index) in spawns {
            game.lore
                .residents
                .entry(sector(x, y))
                .or_default()
                .lineages
                .entry(GHOST)
                .or_default()
                .push((index, None));
        }
    }

    /// A ghost creature standing in `at`'s sector, remembered as spawn `index` of `home`.
    fn ghost_at(game: &mut Game, at: Vec2, origin: Option<(SectorId, u32)>) -> u64 {
        let id = spawn(game, &ghost(), at);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().origin = origin;
        id
    }

    fn kill(game: &mut Game, id: u64) {
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 0.0;
        game.step(DT, Input::default());
    }

    #[test]
    fn adjacent_sectors_join_one_range_and_disjoint_ones_stay_apart() {
        let mut game = empty_game();
        // A row, a diagonal neighbor, and a far island.
        seed_ghosts(
            &mut game,
            &[(0, 0, 1), (1, 0, 2), (2, 1, 3), (6, 6, 4), (-3, 0, 5)],
        );
        let near = game.species_range(GHOST, sector(0, 0));
        assert_eq!(near, vec![sector(0, 0), sector(1, 0), sector(2, 1)]);
        // The same range from any member, and a different one for the island and the loner.
        assert_eq!(game.species_range(GHOST, sector(2, 1)), near);
        assert_eq!(game.species_range(GHOST, sector(6, 6)), vec![sector(6, 6)]);
        assert_eq!(
            game.species_range(GHOST, sector(-3, 0)),
            vec![sector(-3, 0)]
        );
        // A sector without the lineage has no range.
        assert!(game.species_range(GHOST, sector(0, 1)).is_empty());
    }

    #[test]
    fn a_range_stops_at_the_radius() {
        let mut game = empty_game();
        let row: Vec<(i32, i32, u32)> = (0..=RANGE_RADIUS + 4).map(|x| (x, 0, x as u32)).collect();
        seed_ghosts(&mut game, &row);
        let range = game.species_range(GHOST, sector(0, 0));
        assert_eq!(range.len(), RANGE_RADIUS as usize + 1);
    }

    #[test]
    fn extirpation_fires_exactly_once_when_the_last_member_of_the_range_dies() {
        let mut game = empty_game();
        seed_ghosts(&mut game, &[(0, 0, 900), (1, 0, 901)]);
        let a = ghost_at(&mut game, Vec2::new(0.0, 2000.0), Some((sector(0, 0), 900)));
        let b = ghost_at(&mut game, Vec2::new(6000.0, 0.0), Some((sector(1, 0), 901)));
        kill(&mut game, a);
        assert!(game.run.extirpated.is_empty(), "one of two is not enough");
        assert!(game.notices.iter().all(|n| !n.text.contains("extirpated")));
        kill(&mut game, b);
        let name = Genome::default().name().to_uppercase();
        assert_eq!(game.run.extirpated.len(), 1);
        let found = &game.run.extirpated[0];
        assert_eq!((found.name.as_str(), found.sectors), (name.as_str(), 2));
        assert!(
            game.notices
                .iter()
                .any(|n| n.text == format!("Species extirpated: {name}"))
        );
        assert!(game.drain_cues().contains(&Cue::Extirpated));
        // More events of the lineage never announce it again.
        let c = ghost_at(&mut game, Vec2::new(0.0, 2000.0), None);
        kill(&mut game, c);
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.run.extirpated.len(), 1);
        let announced = game
            .notices
            .iter()
            .filter(|n| n.text.starts_with("Species extirpated"))
            .count();
        assert_eq!(announced, 1);
    }

    #[test]
    fn unloaded_sectors_are_judged_by_spawns_minus_fallen() {
        let mut game = empty_game();
        // A far sector is part of the range but not loaded, with two spawns.
        seed_ghosts(&mut game, &[(0, 0, 900), (1, 0, 901), (1, 0, 902)]);
        let a = ghost_at(&mut game, Vec2::new(0.0, 2000.0), Some((sector(0, 0), 900)));
        game.bodies
            .retain(|b| b.kind == BodyKind::Player || b.id == a);
        game.fallen.entry(sector(1, 0)).or_default().insert(901);
        kill(&mut game, a);
        assert!(game.run.extirpated.is_empty(), "spawn 902 still stands");
        // Its fall is remembered the way kills always are, then a later death settles it.
        game.fallen.entry(sector(1, 0)).or_default().insert(902);
        let d = ghost_at(&mut game, Vec2::new(0.0, -2000.0), None);
        kill(&mut game, d);
        assert_eq!(game.run.extirpated.len(), 1);
    }

    #[test]
    fn creatures_from_outside_the_range_do_not_block_but_inside_ones_do() {
        let mut game = empty_game();
        seed_ghosts(&mut game, &[(0, 0, 900)]);
        let a = ghost_at(&mut game, Vec2::new(0.0, 2000.0), Some((sector(0, 0), 900)));
        // Bred or wandering from elsewhere: a sector that is not in the range.
        let outsider = ghost_at(&mut game, Vec2::new(-6000.0, 0.0), None);
        // Bred inside the range: it is part of what remains.
        let insider = ghost_at(&mut game, Vec2::new(0.0, -2000.0), None);
        kill(&mut game, a);
        assert!(game.run.extirpated.is_empty());
        kill(&mut game, insider);
        assert_eq!(game.run.extirpated.len(), 1);
        assert!(game.body(outsider).is_some());
    }

    #[test]
    fn an_egg_in_the_range_blocks_and_breaking_it_finishes_the_job() {
        let mut game = empty_game();
        seed_ghosts(&mut game, &[(0, 0, 900)]);
        let a = ghost_at(&mut game, Vec2::new(0.0, 2000.0), Some((sector(0, 0), 900)));
        let mut egg = growth::Egg::laid(GHOST, 0, Genome::default(), Phenotype::default());
        egg.position = Vec2::new(0.0, 1500.0);
        game.eggs.push(egg);
        kill(&mut game, a);
        assert!(game.run.extirpated.is_empty());
        game.bullets
            .push(Bullet::friendly(Vec2::new(0.0, 1500.0), Vec2::ZERO, 1.0));
        game.step(DT, Input::default());
        assert_eq!(game.run.eggs, 1);
        assert_eq!(game.run.extirpated.len(), 1);
    }

    #[test]
    fn a_fallen_civilization_is_not_waiting_to_come_back() {
        // Spawns of a ruined territory count as gone; unknown territories do not.
        let mut game = empty_game();
        seed_ghosts(&mut game, &[(0, 0, 900)]);
        game.lore
            .residents
            .get_mut(&sector(0, 0))
            .unwrap()
            .lineages
            .get_mut(&GHOST)
            .unwrap()[0]
            .1 = Some(7);
        let before = game.civ_standing(7);
        assert_ne!(before, crate::territory::Standing::Fallen);
        let d = ghost_at(&mut game, Vec2::new(0.0, -2000.0), None);
        kill(&mut game, d);
        assert!(game.run.extirpated.is_empty());
    }

    /// Plays out wiping the start Bogeys from their local range, and reports the run.
    fn wipe_home_bogeys() -> (Game, RunReport) {
        let mut game = Game::new(42);
        game.player_invulnerability = 1e9;
        let lineage = Species::bogey().lineage;
        let school = crate::range::start_sector(42, Species::bogey());
        game.teleport(school.center());
        game.step(DT, Input::default());
        let range = game.species_range(lineage, school);
        assert!(range.contains(&school));
        assert!(range.len() as i32 <= (2 * RANGE_RADIUS + 1).pow(2));
        // Everything of theirs that is not a live creature right now is gone already.
        let live: HashSet<(SectorId, u32)> = game
            .bodies
            .iter()
            .filter(|b| b.species == lineage)
            .filter_map(|b| b.origin)
            .collect();
        for &id in &range {
            let spawns = game.lore.residents(42, id).lineages[&lineage].clone();
            for (index, _) in spawns {
                if !live.contains(&(id, index)) {
                    game.fallen.entry(id).or_default().insert(index);
                }
            }
        }
        let mut ids: Vec<u64> = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature && b.species == lineage)
            .map(|b| b.id)
            .collect();
        ids.sort_unstable();
        assert!(ids.len() > 3, "the start holds a school of bogeys");
        let last = ids.pop().unwrap();
        for id in ids {
            kill(&mut game, id);
        }
        assert!(game.run.extirpated.is_empty(), "one bogey still lives");
        kill(&mut game, last);
        let report = game.run_report();
        (game, report)
    }

    #[test]
    fn wiping_out_the_start_bogeys_extirpates_them_and_only_them() {
        let (game, report) = wipe_home_bogeys();
        assert_eq!(game.run.extirpated.len(), 1);
        let found = &game.run.extirpated[0];
        assert_eq!(found.name, Species::bogey().name().to_uppercase());
        assert!(found.sectors >= 1);
        assert_eq!(report.extirpated.len(), 1);
        assert!(report.extirpated[0].starts_with(&found.name));
        assert!(game.run.kills > 3);
        assert_eq!(game.run.by_species[0].1, found.name);
        // Nothing else was wiped out with them.
        assert!(
            game.run
                .by_species
                .iter()
                .skip(1)
                .all(|(_, name, _)| *name != found.name)
        );
    }

    #[test]
    fn extirpation_and_reports_are_deterministic() {
        let (a, ra) = wipe_home_bogeys();
        let (b, rb) = wipe_home_bogeys();
        assert_eq!(ra, rb);
        assert_eq!(a.run.extirpated, b.run.extirpated);
        assert_eq!(a.run.kills, b.run.kills);
    }

    #[test]
    fn sectors_explored_counts_distinct_sectors_and_depth() {
        let mut game = empty_game();
        game.step(DT, Input::default());
        assert_eq!(game.run.sectors.len(), 1);
        assert_eq!(game.run.deepest, 0.0);
        game.teleport(Vec2::new(6000.0, 0.0));
        game.step(DT, Input::default());
        game.teleport(Vec2::new(0.0, 0.0));
        game.step(DT, Input::default());
        assert_eq!(game.run.sectors.len(), 2);
        assert!(game.run.deepest > 0.0);
        assert!(game.run.threat > 1.0);
        game.teleport(Vec2::new(-12_000.0, 6000.0));
        game.step(DT, Input::default());
        assert_eq!(game.run.sectors.len(), 3);
    }

    #[test]
    fn kills_shots_damage_and_distance_are_counted_where_they_happen() {
        let mut game = empty_game();
        let bogey = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 1500.0));
        let bogey2 = spawn(&mut game, &Species::bogey(), Vec2::new(300.0, 1500.0));
        // A friendly shot lands.
        game.bullets.push(Bullet::friendly(
            body(&game, bogey).position,
            Vec2::ZERO,
            1.0,
        ));
        game.step(DT, Input::default());
        assert!(game.run.damage_dealt > 0.0);
        // A hostile one hurts the ship.
        let at = game.player().unwrap().position;
        game.bullets
            .push(Bullet::hostile(at, Vec2::ZERO, 1.0, 10.0));
        game.step(DT, Input::default());
        assert!(game.run.damage_taken > 5.0);
        // Firing is counted, flying too.
        let fire = Input {
            fire: true,
            ..Default::default()
        };
        game.step(DT, fire);
        assert!(game.run.shots >= 1);
        set_player(&mut game, Vec2::ZERO, Vec2::new(120.0, 0.0));
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert!(
            (game.run.distance - 120.0).abs() < 30.0,
            "{}",
            game.run.distance
        );
        // Kills by species, with juveniles and creatures lost to nature set apart.
        kill(&mut game, bogey);
        assert_eq!((game.run.kills, game.run.juveniles), (1, 0));
        {
            let b = game.bodies.iter_mut().find(|b| b.id == bogey2).unwrap();
            b.adult = Some(Genome::bogey());
            b.growth = 0.5;
        }
        kill(&mut game, bogey2);
        assert_eq!((game.run.kills, game.run.juveniles), (2, 1));
        assert_eq!(game.run.by_species.len(), 1);
        assert_eq!(game.run.by_species[0].2, 2);
        let eaten = spawn(&mut game, &Species::lunatic(), Vec2::new(0.0, -1500.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == eaten)
            .unwrap()
            .consumed = true;
        kill(&mut game, eaten);
        assert_eq!((game.run.kills, game.run.lost_to_nature), (2, 1));
        let station = add(&mut game, BodyKind::Base, Vec2::new(500.0, 1500.0));
        kill(&mut game, station);
        assert_eq!(game.run.bases, 1);
    }

    #[test]
    fn mining_parts_and_deaths_are_counted() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        game.bodies[0].max_shield = 1e6;
        game.bodies[0].shield = 1e6;
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(110.0, 0.0));
        let r = game.bodies.iter_mut().find(|b| b.id == rock).unwrap();
        r.rock = RockKind::Ice;
        r.radius = 40.0;
        r.health = 64.0;
        r.max_health = 64.0;
        let beam = Input {
            mine: true,
            aim_direction: Some(Vec2::X),
            ..Default::default()
        };
        for _ in 0..60 * 80 {
            if game.body(rock).is_none() {
                break;
            }
            game.step(DT, beam);
        }
        assert!(game.run.mined[1] > 1.0, "{:?}", game.run.mined);
        assert_eq!(game.run.rocks_depleted, 1);
        game.collect(Item::Part(ShipPart {
            name: "Test Plate".into(),
            slot: Slot::Plating,
            rarity: upgrades::Rarity::Rare,
            grade: 1.0,
            stem: String::new(),
            core: usize::MAX,
            effects: vec![Effect::Stat(Stat::Hull, 0.5)],
        }));
        assert_eq!(game.run.parts, 1);
    }

    #[test]
    fn stats_follow_the_lives_and_reset_on_game_over() {
        let mut game = empty_game();
        seed_ghosts(&mut game, &[(0, 0, 900)]);
        let g = ghost_at(&mut game, Vec2::new(0.0, 2000.0), Some((sector(0, 0), 900)));
        kill(&mut game, g);
        for _ in 0..3 {
            game.bodies
                .iter_mut()
                .find(|b| b.kind == BodyKind::Player)
                .unwrap()
                .health = 0.0;
            game.step(DT, Input::default());
        }
        assert!(game.game_over);
        assert_eq!(game.run.deaths, 3);
        assert_eq!(game.run.extirpated.len(), 1);
        let report = game.run_report();
        assert_eq!(report.extirpated.len(), 1);
        assert!(report.lines.iter().any(|l| l.contains("LIVES USED 3")));
        game.reset();
        assert_eq!(game.run.deaths, 0);
        assert_eq!(game.run.kills, 0);
        assert!(game.run.extirpated.is_empty());
        assert_eq!(game.run.sectors.len(), 0);
        assert_eq!(game.run_report().extirpated.len(), 0);
    }

    #[test]
    fn range_search_cost_stays_bounded() {
        // The worst case is a lineage present everywhere: the whole box, generated once.
        let mut game = empty_game();
        let all: Vec<(i32, i32, u32)> = (-RANGE_RADIUS..=RANGE_RADIUS)
            .flat_map(|x| (-RANGE_RADIUS..=RANGE_RADIUS).map(move |y| (x, y, 1)))
            .collect();
        seed_ghosts(&mut game, &all);
        let range = game.species_range(GHOST, sector(0, 0));
        assert_eq!(range.len() as i32, (2 * RANGE_RADIUS + 1).pow(2));
    }
}
