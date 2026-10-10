//! Saving and loading a run (workstream 12; the state inventory is `docs/PERSISTENCE.md`).
//!
//! Generation is a pure function of the master seed and the sector, so a save holds only what
//! diverges from it: the ship, its loadout and hold, the pads, the chart, regard, the run record,
//! the legacy, and the per-sector deltas (spawns destroyed, ore taken, relics taken). Creatures,
//! rocks and every other body are regenerated from the seed on load with those deltas applied,
//! so the world around the ship is the same world, not the same instant of it. Nothing here
//! touches rendering or the disk; `crate::savefile` owns the file.
//!
//! The format is RON text under a `Save(...)` header carrying `version` (this file's layout) and
//! `generator` (`GENERATOR_VERSION` when it was written). New fields must be `#[serde(default)]`
//! so existing saves still load; incompatible layouts bump `SAVE_VERSION` and are refused
//! before 1.0. No save migration code or fixtures are introduced.

use super::*;
use crate::sectormap::GENERATOR_VERSION;
use crate::world::SectorId;
use serde::{Deserialize, Serialize};

/// The layout version this build writes.
pub const SAVE_VERSION: u32 = 4;

/// Why a save could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    /// Not valid RON for this layout.
    Parse(String),
    /// Written by a newer build than this one.
    TooNew { found: u32, supported: u32 },
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(why) => write!(f, "unreadable save: {why}"),
            Self::TooNew { found, supported } => {
                write!(
                    f,
                    "save version {found} is newer than this build ({supported})"
                )
            }
        }
    }
}

impl std::error::Error for SaveError {}

/// What loading did, for the adapter to report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadReport {
    /// False when the save came from another generator version: spawn indices no longer mean
    /// the same creatures and rocks, so the per-sector deltas (kills, ore, relics, pads) were
    /// dropped and only the ship's progress and the chart were kept.
    pub world_deltas_kept: bool,
}

/// The ship's body at the moment of the save.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ShipSave {
    position: Vec2,
    velocity: Vec2,
    angle: f32,
    health: f32,
    shield: f32,
}

/// The states of every random stream, so a loaded game does not replay the rolls of a new one.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Streams {
    rng: Rng,
    loot: Rng,
    variation: Rng,
    growth: Rng,
    breeding: Rng,
    parry: Rng,
    civ: Rng,
    apex: Rng,
}

/// Everything player-created or divergent from generation. See the module docs.
#[derive(Serialize, Deserialize)]
pub struct SaveState {
    seed: u64,
    #[serde(default)]
    societies: society::Societies,
    time: f32,
    score: u64,
    lives: u32,
    ship: ShipSave,
    cargo: Cargo,
    loadout: Loadout,
    #[serde(default)]
    home_input_orders: u8,
    #[serde(default)]
    jobs: jobs::Jobs,
    pad: PadState,
    chart: chart::ChartState,
    run: run::RunStats,
    legacy: legacy::Legacy,
    /// Regard of each civilization met, by territory.
    regard: Vec<(u64, Regard)>,
    /// Lasting falls (capital destroyed, elder slain), by territory.
    falls: Vec<(u64, Fall)>,
    /// Spawns destroyed, per sector, sorted.
    fallen: Vec<(SectorId, Vec<u32>)>,
    /// Ore taken from rocks by spawn, and when each renewable entry was last current.
    mined: Vec<((SectorId, u32), f32)>,
    #[serde(default)]
    mined_contents: Vec<((SectorId, u32), [f32; 4])>,
    regrow: Vec<((SectorId, u32), f32)>,
    relics: Vec<SectorId>,
    /// Creature-built structures: what is left of each, block by block (see `build`).
    #[serde(default)]
    structures: Vec<(build::StructureKey, build::Structure)>,
    /// Structures each territory has started, so its building budget survives a reload.
    #[serde(default)]
    civ_started: Vec<(u64, u8)>,
    /// Plants, seeds and biomass (see `farm`).
    #[serde(default)]
    farm: farm::Farm,
    /// Tunables that differ from their defaults, by name (see `tune`); absent in a normal run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tuning: Vec<(String, f32)>,
    streams: Streams,
}

/// The first line of every save; read on its own so an old layout can be recognised before the
/// rest is parsed.
#[derive(Deserialize)]
#[serde(rename = "Save")]
struct Header {
    version: u32,
    generator: u32,
}

#[derive(Serialize)]
#[serde(rename = "Save")]
struct SaveOut<'a> {
    version: u32,
    generator: u32,
    state: &'a SaveState,
}

impl SaveState {
    /// The master seed this save belongs to.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The text written to disk. Deterministic: the same state always gives the same bytes.
    pub fn to_text(&self) -> String {
        let out = SaveOut {
            version: SAVE_VERSION,
            generator: GENERATOR_VERSION,
            state: self,
        };
        let pretty = ron::ser::PrettyConfig::new().depth_limit(3);
        ron::ser::to_string_pretty(&out, pretty).unwrap_or_default()
    }

    /// Reads a compatible save, refusing older incompatible layouts. Returns the state and the generator version it
    /// was written under.
    pub fn from_text(text: &str) -> Result<(SaveState, u32), SaveError> {
        let header: Header = ron::from_str(text).map_err(|e| SaveError::Parse(e.to_string()))?;
        if header.version > SAVE_VERSION {
            return Err(SaveError::TooNew {
                found: header.version,
                supported: SAVE_VERSION,
            });
        }
        let text = compatible_layout(header.version, text)?;
        #[derive(Deserialize)]
        #[serde(rename = "Save")]
        struct Body {
            state: SaveState,
        }
        let body: Body = ron::from_str(&text).map_err(|e| SaveError::Parse(e.to_string()))?;
        Ok((body.state, header.generator))
    }
}

/// Refuses incompatible pre-1.0 layouts without migration.
fn compatible_layout(from: u32, text: &str) -> Result<String, SaveError> {
    match from {
        SAVE_VERSION => Ok(text.to_string()),
        other => Err(SaveError::Parse(format!(
            "incompatible save version {other}"
        ))),
    }
}

fn sorted<K: Ord + Copy, V: Clone>(map: &HashMap<K, V>) -> Vec<(K, V)> {
    let mut pairs: Vec<(K, V)> = map.iter().map(|(k, v)| (*k, v.clone())).collect();
    pairs.sort_by_key(|(k, _)| *k);
    pairs
}

impl Game {
    /// Captures the persistent state of this run. Pure: does not change the game.
    pub fn save_state(&self) -> SaveState {
        let ship = self.player();
        let ship = ShipSave {
            position: ship.map_or(self.focus, |s| s.position),
            velocity: ship.map_or(Vec2::ZERO, |s| s.velocity),
            angle: ship.map_or(FRAC_PI_2, |s| s.angle),
            health: ship.map_or(0.0, |s| s.health),
            shield: ship.map_or(0.0, |s| s.shield),
        };
        let mut fallen: Vec<(SectorId, Vec<u32>)> = self
            .fallen
            .iter()
            .map(|(id, set)| {
                let mut spawns: Vec<u32> = set.iter().copied().collect();
                spawns.sort_unstable();
                (*id, spawns)
            })
            .collect();
        fallen.sort_by_key(|(id, _)| *id);
        let mut relics: Vec<SectorId> = self.relics_taken.iter().copied().collect();
        relics.sort();
        SaveState {
            seed: self.seed,
            societies: self.civs.societies.clone(),
            time: self.time,
            score: self.score,
            lives: self.lives,
            ship,
            cargo: self.cargo,
            loadout: self.loadout.clone(),
            home_input_orders: self.home_input_orders,
            jobs: self.jobs.clone(),
            pad: self.pad.clone(),
            chart: self.chart.clone(),
            run: self.run.clone(),
            legacy: self.legacy.clone(),
            regard: self.civs.regard.iter().map(|(k, v)| (*k, *v)).collect(),
            falls: sorted(&self.civs.fall),
            fallen,
            mined: sorted(&self.mined),
            mined_contents: sorted(&self.mined_contents),
            regrow: sorted(&self.regrow_stamp),
            relics,
            structures: self.structures_snapshot().into_iter().collect(),
            civ_started: sorted(&self.builds.civ_started),
            farm: self.farm.clone(),
            tuning: self
                .tune
                .overrides()
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
            streams: Streams {
                rng: self.rng.clone(),
                loot: self.loot.clone(),
                variation: self.variation.clone(),
                growth: self.growth.clone(),
                breeding: self.breeding.clone(),
                parry: self.parry_rng.clone(),
                civ: self.civs.rng.clone(),
                apex: self.apexes.rng.clone(),
            },
        }
    }

    /// Rebuilds a run from a save. `generator` is the generator version it was written under
    /// (from `SaveState::from_text`); see `LoadReport`.
    pub fn from_save(state: SaveState, generator: u32) -> (Game, LoadReport) {
        let keep = generator == GENERATOR_VERSION;
        let mut game = Game::blank(state.seed);
        // Before anything is generated, so `Regen` entries shape the world that loads. An entry
        // a newer build dropped or a value now out of range is skipped, never fatal.
        tunables::set_many(&mut game.tune, state.tuning);
        game.civs.societies = state.societies.restore(keep);
        game.time = state.time;
        game.score = state.score;
        game.lives = state.lives.max(1);
        game.cargo = state.cargo;
        game.loadout = state.loadout;
        game.home_input_orders = state.home_input_orders;
        game.jobs = state.jobs;
        if !keep {
            game.jobs.invalidate_world();
        }
        game.chart = state.chart;
        game.run = state.run;
        game.legacy = state.legacy;
        game.civs.regard = state.regard.into_iter().collect();
        game.civs.fall = state.falls.into_iter().collect();
        if keep {
            game.pad = state.pad;
            game.fallen = state
                .fallen
                .into_iter()
                .map(|(id, spawns)| (id, spawns.into_iter().collect()))
                .collect();
            game.mined = state.mined.into_iter().collect();
            game.mined_contents = state.mined_contents.into_iter().collect();
            game.regrow_stamp = state.regrow.into_iter().collect();
            game.relics_taken = state.relics.into_iter().collect();
            game.builds.kept = state.structures.into_iter().collect();
            game.builds.civ_started = state.civ_started.into_iter().collect();
        } else {
            // Pads are keyed by spawn too: keep the kits and settings, drop the placed pads and
            // plant the home pad afresh below.
            game.pad.kits = state.pad.kits;
            game.pad.insured = state.pad.insured;
            game.pad.drone_blueprint = state.pad.drone_blueprint;
            game.pad.drone_other_blueprints = state.pad.drone_other_blueprints;
            game.pad.drone_role = state.pad.drone_role;
            game.pad.drone_role_names = state.pad.drone_role_names;
        }
        game.adopt_farm(state.farm, keep);
        game.spawn_player_exact(state.ship.position);
        if let Some(ship) = game.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.velocity = state.ship.velocity;
            ship.angle = state.ship.angle;
            // Never come back dead; a save of a dying ship loads at a sliver of hull.
            ship.health = state.ship.health.clamp(1.0, ship.max_health);
            ship.shield = state.ship.shield.clamp(0.0, ship.max_shield);
        }
        game.stream_sectors();
        if !keep {
            game.seed_home_pad();
        }
        // The saved streams are the state after the world around the ship was loaded, so they go
        // back last: loading the sectors must not spend the rolls the player would have had.
        game.rng = state.streams.rng;
        game.loot = state.streams.loot;
        game.variation = state.streams.variation;
        game.growth = state.streams.growth;
        game.breeding = state.streams.breeding;
        game.parry_rng = state.streams.parry;
        game.civs.rng = state.streams.civ;
        game.apexes.rng = state.streams.apex;
        game.player_invulnerability = 2.5;
        (
            game,
            LoadReport {
                world_deltas_kept: keep,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::arsenal::Profile;
    use crate::simulation::upgrades::{Effect, Part, Rarity, Slot, Stat};

    const DT: f32 = 1.0 / 60.0;

    fn part() -> Part {
        Part {
            name: "Saved Drive".to_string(),
            slot: Slot::Engine,
            rarity: Rarity::Rare,
            grade: 2.0,
            effects: vec![Effect::Stat(Stat::Thrust, 0.3)],
            stem: String::new(),
            core: usize::MAX,
        }
    }

    /// A run with a past: flown, shot, mined into, geared up, charted, one spawn destroyed.
    fn lived_in() -> Game {
        let mut game = Game::new(7);
        game.cargo.add(Material::Metal, 31.5);
        game.cargo.add(Material::Crystal, 4.0);
        game.loadout.acquire(part());
        game.loadout.arsenal.acquire(Profile::Spread, 2);
        game.refresh_stats();
        game.score = 1234;
        game.lives = 2;
        game.run.kills = 5;
        game.run.dashes = 3;
        game.fallen.entry(SectorId::ORIGIN).or_default().insert(3);
        game.fallen
            .entry(SectorId { x: 1, y: 0 })
            .or_default()
            .insert(11);
        game.mined.insert((SectorId::ORIGIN, 2), 12.25);
        game.relics_taken.insert(SectorId { x: -1, y: 2 });
        game.pad.kits = 2;
        for _ in 0..600 {
            game.step(
                DT,
                Input {
                    thrust: 1.0,
                    turn: 0.3,
                    fire: true,
                    ..Input::default()
                },
            );
        }
        game
    }

    #[test]
    fn text_round_trip_is_a_fixed_point() {
        let game = lived_in();
        let text = game.save_state().to_text();
        let (state, generator) = SaveState::from_text(&text).unwrap();
        assert_eq!(generator, GENERATOR_VERSION);
        let (loaded, report) = Game::from_save(state, generator);
        assert!(report.world_deltas_kept);
        // A pad's reload count is bumped by the sector reload that loading is, by design.
        let strip = |t: &str| {
            let mut out = String::new();
            for part in t.split("reloads: ") {
                let rest = part.trim_start_matches(|c: char| c.is_ascii_digit());
                out.push_str(rest);
            }
            out
        };
        assert_eq!(strip(&loaded.save_state().to_text()), strip(&text));
    }

    #[test]
    fn text_is_deterministic() {
        // Two independently built identical runs write the same bytes (no hash-order leaks).
        assert_eq!(
            lived_in().save_state().to_text(),
            lived_in().save_state().to_text()
        );
    }

    #[test]
    fn what_the_player_earned_comes_back() {
        let game = lived_in();
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.seed(), 7);
        assert_eq!(loaded.cargo, game.cargo);
        assert_eq!(loaded.loadout, game.loadout);
        assert_eq!(loaded.score, 1234);
        assert_eq!(loaded.lives, 2);
        assert_eq!(loaded.run.kills, 5);
        assert_eq!(loaded.pad.kits, 2);
        assert_eq!(loaded.fallen, game.fallen);
        assert_eq!(loaded.relics_taken, game.relics_taken);
        assert_eq!(loaded.mined.get(&(SectorId::ORIGIN, 2)), Some(&12.25));
        assert_eq!(loaded.time, game.time);
        let (a, b) = (game.player().unwrap(), loaded.player().unwrap());
        assert_eq!(a.position, b.position);
        assert_eq!(a.velocity, b.velocity);
        assert_eq!(a.angle, b.angle);
        assert!(loaded.loadout.parts.iter().any(|p| p.name == "Saved Drive"));
    }

    #[test]
    fn destroyed_spawns_stay_destroyed() {
        let mut game = Game::new(7);
        game.teleport(Vec2::new(crate::world::SECTOR_SIZE * 2.0, 0.0));
        game.step(DT, Input::default());
        let victim = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature)
            .find_map(|b| b.origin)
            .expect("a creature near sector (2, 0)");
        game.fallen.entry(victim.0).or_default().insert(victim.1);
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert!(loaded.bodies.iter().any(|b| b.kind == BodyKind::Creature));
        assert!(!loaded.bodies.iter().any(|b| b.origin == Some(victim)));
        // And the control: the same place without the delta still has it.
        let mut fresh = Game::new(7);
        fresh.teleport(Vec2::new(crate::world::SECTOR_SIZE * 2.0, 0.0));
        fresh.step(DT, Input::default());
        assert!(fresh.bodies.iter().any(|b| b.origin == Some(victim)));
    }

    #[test]
    fn two_loads_of_one_save_play_identically() {
        let text = lived_in().save_state().to_text();
        let play = || {
            let (state, generator) = SaveState::from_text(&text).unwrap();
            let (mut game, _) = Game::from_save(state, generator);
            for i in 0..600 {
                game.step(
                    DT,
                    Input {
                        thrust: 1.0,
                        turn: if i % 90 < 45 { 0.5 } else { -0.5 },
                        fire: i % 7 < 3,
                        ..Input::default()
                    },
                );
            }
            let ship = game.player().map(|s| (s.position, s.health));
            (game.save_state().to_text(), ship, game.bodies.len())
        };
        assert_eq!(play(), play());
    }

    #[test]
    fn a_newer_or_broken_save_is_refused_not_guessed() {
        let text = lived_in().save_state().to_text();
        let newer = text.replacen(
            &format!("version: {SAVE_VERSION}"),
            &format!("version: {}", SAVE_VERSION + 1),
            1,
        );
        assert!(matches!(
            SaveState::from_text(&newer),
            Err(SaveError::TooNew { .. })
        ));
        assert!(matches!(
            SaveState::from_text("not a save"),
            Err(SaveError::Parse(_))
        ));
        assert!(matches!(
            SaveState::from_text(&text[..text.len() / 2]),
            Err(SaveError::Parse(_))
        ));
    }

    #[test]
    fn another_generator_keeps_progress_and_drops_spawn_keyed_deltas() {
        let game = lived_in();
        let (state, _) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, report) = Game::from_save(state, GENERATOR_VERSION + 1);
        assert!(!report.world_deltas_kept);
        assert!(loaded.fallen.is_empty());
        assert!(loaded.mined.is_empty());
        assert!(loaded.relics_taken.is_empty());
        assert_eq!(loaded.loadout, game.loadout);
        assert_eq!(loaded.cargo, game.cargo);
        assert_eq!(loaded.run.kills, 5);
        assert_eq!(loaded.pad.kits, 2);
        // The home pad is planted again.
        assert!(loaded.pad.pads.values().any(|p| p.home));
    }

    #[test]
    fn a_dying_ship_does_not_load_dead() {
        let mut game = Game::new(7);
        if let Some(ship) = game.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.health = 0.0;
        }
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert!(loaded.player().unwrap().health >= 1.0);
    }

    #[test]
    fn exhaustion_saves_the_same_game_at_home_with_one_life() {
        let mut game = Game::new(7);
        game.score = 4500;
        game.lives = 1;
        game.bodies
            .iter_mut()
            .find(|body| body.kind == BodyKind::Player)
            .unwrap()
            .health = 0.0;
        game.step(1.0 / 60.0, Input::default());
        let (state, generator) = SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.lives, 1);
        assert_eq!(loaded.score, 4500);
        assert_eq!(loaded.run.deaths, 1);
        assert_eq!(loaded.legacy.generation, 0);
        assert!(loaded.pending_bequest().is_none());
        let home = loaded.pads().find(|pad| pad.home).unwrap();
        assert!(loaded.player().unwrap().position.distance(home.center) < home.radius + 150.0);
    }
}
