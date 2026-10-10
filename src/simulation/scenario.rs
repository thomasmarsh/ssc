//! Headless scenarios: a fixed seed, a start sector, a scripted input and a tick count, run at
//! the fixed 1/60 s step with digests (`digest.rs`) taken at checkpoints. They are the safety net
//! for refactors: the pinned digests in `goldens.rs` fail when behavior changes, and the
//! `simperf` binary times the same scenarios (see `docs/PERF.md`).
//!
//! This module never reads the clock or the environment; the caller times and compares.

use super::*;
use crate::config::MASTER_SEED;
use crate::simulation::goldens;
use crate::world::SectorId;

/// The fixed step every scenario runs at.
pub const DT: f32 = 1.0 / 60.0;

/// What the pilot does at a tick, given the centre of the start sector. It may call the same API
/// the adapter calls (dash and so on) and read the game, so a script can stay near its start.
pub type Script = fn(tick: u32, game: &mut Game, anchor: Vec2) -> Input;

/// What the ship carries when the scenario starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kit {
    /// A new run's ship.
    Stock,
    /// Every skill, weapon and organ at its top level and a full hold (the headless `dev_grant_*`
    /// API), so dashes, parries, missiles, mines and organs are exercised.
    Maxed,
}

/// One reproducible run.
pub struct Scenario {
    /// Stable name, used by the goldens and by `simperf --only`.
    pub name: &'static str,
    /// What it exercises, one line.
    pub about: &'static str,
    pub seed: u64,
    /// The ship starts at the centre of this sector (HOME is the origin and is not teleported to).
    pub sector: SectorId,
    /// Like `SSC_TELEPORT` smoke runs: the hull cannot be lost, so a busy sector stays busy.
    pub invulnerable: bool,
    pub kit: Kit,
    pub script: Script,
    pub ticks: u32,
    /// Ticks at which a digest is taken (ascending; the last should equal `ticks`).
    pub checkpoints: &'static [u32],
}

/// The digest of a game after `tick` steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub tick: u32,
    pub digest: digest::StateDigest,
}

/// A scenario being run one tick at a time, so a caller can time or inspect each step.
pub struct Run {
    scenario: &'static Scenario,
    pub(crate) game: Game,
    tick: u32,
}

impl Run {
    pub fn new(scenario: &'static Scenario) -> Self {
        let mut game = Game::new(scenario.seed);
        if scenario.sector != SectorId::ORIGIN {
            game.teleport(scenario.sector.center());
        }
        if scenario.kit == Kit::Maxed {
            game.dev_grant_skills();
            game.dev_grant_weapons();
            game.dev_grant_organs();
            game.dev_fill_hold();
        }
        if scenario.invulnerable {
            game.player_invulnerability = 1e9;
        }
        Self {
            scenario,
            game,
            tick: 0,
        }
    }

    /// Advances one tick. Returns false once the scenario's tick count is spent.
    pub fn tick(&mut self) -> bool {
        if self.tick >= self.scenario.ticks {
            return false;
        }
        let anchor = self.scenario.sector.center();
        let input = (self.scenario.script)(self.tick, &mut self.game, anchor);
        self.game.step(DT, input);
        self.tick += 1;
        true
    }

    pub fn ticks_done(&self) -> u32 {
        self.tick
    }

    /// Sectors currently loaded around the ship.
    pub fn loaded_sectors(&self) -> usize {
        self.game.loaded.len()
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Continues `scenario`'s script with an existing game (for example one loaded from a save)
    /// as though `tick` steps had already run.
    pub fn resume(scenario: &'static Scenario, game: Game, tick: u32) -> Self {
        Self {
            scenario,
            game,
            tick,
        }
    }

    pub fn scenario(&self) -> &'static Scenario {
        self.scenario
    }
}

/// Runs a scenario to its end and returns its digest at every checkpoint.
pub fn run(scenario: &'static Scenario) -> Vec<Checkpoint> {
    let mut run = Run::new(scenario);
    let mut out = Vec::with_capacity(scenario.checkpoints.len());
    let mut next = scenario.checkpoints.iter().copied().peekable();
    while run.tick() {
        while next.peek() == Some(&run.ticks_done()) {
            next.next();
            out.push(Checkpoint {
                tick: run.ticks_done(),
                digest: run.game().state_digest(),
            });
        }
    }
    out
}

/// A pinned digest: `parts` are `StateDigest::parts()` values in order.
#[derive(Clone, Copy, Debug)]
pub struct Golden {
    pub scenario: &'static str,
    pub tick: u32,
    pub parts: [u64; 7],
}

/// How to regenerate pinned digests, appended to every golden failure.
pub const BLESS_HELP: &str = "If this change of behavior is deliberate, regenerate the table with\n  \
    SSC_BLESS=1 cargo test --no-default-features golden   (new lines are in each failure message)\n  \
    cargo run --release --no-default-features --bin simperf -- --goldens   (the whole table)\n\
    paste into src/simulation/goldens.rs and state the reason in the commit message.\n\
    Never bless to make a refactor pass: a refactor must not change behavior.";

/// The `Golden` source lines for a scenario's checkpoints, ready to paste into `goldens.rs`.
pub fn golden_lines(name: &str, checkpoints: &[Checkpoint]) -> String {
    let mut out = String::new();
    for c in checkpoints {
        let parts: Vec<String> = c
            .digest
            .parts()
            .iter()
            .map(|(_, v)| format!("{v:#018x}"))
            .collect();
        out.push_str(&format!(
            "    Golden {{ scenario: \"{name}\", tick: {}, parts: [{}] }},\n",
            c.tick,
            parts.join(", ")
        ));
    }
    out
}

/// Compares checkpoints with the pinned table. `Err` carries a message naming the scenario, the
/// first diverging checkpoint tick and the diverging sub-digests.
pub fn verify(name: &str, checkpoints: &[Checkpoint]) -> Result<(), String> {
    for c in checkpoints {
        let Some(golden) = goldens::GOLDENS
            .iter()
            .find(|g| g.scenario == name && g.tick == c.tick)
        else {
            return Err(format!(
                "scenario `{name}` has no pinned digest at tick {}.\n{BLESS_HELP}",
                c.tick
            ));
        };
        let actual = c.digest.parts();
        let wrong: Vec<String> = actual
            .iter()
            .zip(golden.parts)
            .filter(|((_, got), want)| *got != *want)
            .map(|((part, got), want)| {
                format!("  sub-digest `{part}`: pinned {want:#018x}, got {got:#018x}")
            })
            .collect();
        if !wrong.is_empty() {
            return Err(format!(
                "GOLDEN MISMATCH: scenario `{name}` first diverged at tick {}\n{}\n{BLESS_HELP}",
                c.tick,
                wrong.join("\n")
            ));
        }
    }
    Ok(())
}

pub fn find(name: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().find(|s| s.name == name)
}

// ---- scripts -------------------------------------------------------------------------------

fn idle(_tick: u32, _game: &mut Game, _anchor: Vec2) -> Input {
    Input::default()
}

/// Thrusts, sweeps the nose, fires in bursts, holds the mining beam in a window, brakes and
/// dashes on a schedule: every pilot verb the headless game takes, on a 10 second cycle. A leash
/// steers back toward the start sector's centre so the run stays among its own population.
fn patrol(tick: u32, game: &mut Game, anchor: Vec2) -> Input {
    let phase = tick % 600;
    let sweep = match (tick / 45) % 4 {
        0 => 0.6,
        2 => -0.6,
        _ => 0.0,
    };
    if phase == 200 || phase == 440 {
        game.dash(None);
    }
    let mining = (300..420).contains(&phase);
    let away = game
        .player()
        .map_or(Vec2::ZERO, |ship| ship.position - anchor);
    let leashed = away.length() > LEASH;
    Input {
        thrust: if phase < 480 || leashed { 1.0 } else { 0.0 },
        turn: if leashed { 0.0 } else { sweep },
        aim_direction: leashed.then(|| -away.normalize_or_zero()),
        brake: phase >= 540 && !leashed,
        fire: !mining && tick % 90 < 30,
        mine: mining,
        ..Input::default()
    }
}

/// How far the patrol strays from the start sector's centre before it turns back.
const LEASH: f32 = 2200.0;

/// The patrol with a fully kitted ship: weapon changes, parries and dashes on a schedule.
fn ace(tick: u32, game: &mut Game, anchor: Vec2) -> Input {
    if tick % 150 == 149 {
        game.switch_weapon(1);
    }
    if tick % 240 == 100 {
        game.parry();
    }
    patrol(tick, game, anchor)
}

// ---- the table -----------------------------------------------------------------------------

/// Every scenario, in a fixed order. Sectors were picked from the generated map of the master seed
/// (`sectormap`) for what they hold; names state the ring and the point of the run.
pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "home_idle",
        about: "HOME sanctuary, no input: rocks, plankton, farm and pad clocks only",
        seed: MASTER_SEED,
        sector: SectorId::ORIGIN,
        invulnerable: false,
        kit: Kit::Stock,
        script: idle,
        ticks: 1800,
        checkpoints: &[60, 600, 1200, 1800],
    },
    Scenario {
        name: "ring3_busy",
        about: "ring 3 (35 creatures, Lunatics debut), fully kitted ship: dash, parry, every weapon",
        seed: MASTER_SEED,
        sector: SectorId { x: -3, y: -3 },
        invulnerable: true,
        kit: Kit::Maxed,
        script: ace,
        ticks: 3600,
        checkpoints: &[60, 600, 1800, 3600],
    },
    Scenario {
        name: "civ_city",
        about: "fortified civilization city (stations, walls, turrets, societies), kitted ship",
        seed: MASTER_SEED,
        sector: SectorId { x: 4, y: -5 },
        invulnerable: true,
        kit: Kit::Maxed,
        script: ace,
        ticks: 3600,
        checkpoints: &[60, 600, 1800, 3600],
    },
    Scenario {
        name: "long_busy",
        about: "five simulated minutes in a ring 4 sector with 33 creatures, kitted ship",
        seed: MASTER_SEED,
        sector: SectorId { x: 4, y: 1 },
        invulnerable: true,
        kit: Kit::Maxed,
        script: ace,
        ticks: 18_000,
        checkpoints: &[600, 3000, 9000, 18_000],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::save::SaveState;
    use crate::world;

    /// Runs a scenario against its pinned digests, or prints new lines under `SSC_BLESS=1`.
    fn golden(name: &str) {
        let scenario = find(name).unwrap_or_else(|| panic!("no scenario named {name}"));
        let checkpoints = run(scenario);
        if std::env::var_os("SSC_BLESS").is_some() {
            panic!(
                "SSC_BLESS is set; not verifying. New lines for {name}:\n{}",
                golden_lines(name, &checkpoints)
            );
        }
        if let Err(message) = verify(name, &checkpoints) {
            panic!("{message}");
        }
    }

    macro_rules! goldens {
        ($($test:ident => $name:literal),* $(,)?) => {
            $(
                /// The pinned digests of this scenario hold at every checkpoint.
                #[test]
                fn $test() {
                    golden($name);
                }
            )*
        };
    }

    goldens! {
        golden_home_idle => "home_idle",
        golden_ring3_busy => "ring3_busy",
        golden_civ_city => "civ_city",
        golden_long_busy => "long_busy",
    }

    #[test]
    fn the_scenario_table_is_consistent() {
        let mut names: Vec<_> = SCENARIOS.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SCENARIOS.len(), "scenario names are unique");
        for s in SCENARIOS {
            assert!(!s.checkpoints.is_empty(), "{}", s.name);
            assert!(s.checkpoints.windows(2).all(|w| w[0] < w[1]), "{}", s.name);
            assert_eq!(s.checkpoints.last(), Some(&s.ticks), "{}", s.name);
            for tick in s.checkpoints {
                assert!(
                    goldens::GOLDENS
                        .iter()
                        .any(|g| g.scenario == s.name && g.tick == *tick),
                    "scenario {} has no pinned digest at tick {tick}",
                    s.name
                );
            }
        }
        for g in goldens::GOLDENS {
            let s = find(g.scenario).expect("a golden names a scenario that exists");
            assert!(s.checkpoints.contains(&g.tick), "stale golden {g:?}");
        }
    }

    // ---- guarantee 1: determinism --------------------------------------------------------------

    /// The same seed and the same inputs give identical digests at every checkpoint, run after
    /// run in one process, and whatever other games ran in between (no hidden global state).
    #[test]
    fn same_seed_and_inputs_give_identical_digests_whatever_ran_between() {
        let names = ["ring3_busy", "civ_city", "home_idle"];
        let first: Vec<_> = names.iter().map(|n| run(find(n).unwrap())).collect();
        // Different order, interleaved with another scenario.
        let mut second = Vec::new();
        for n in names.iter().rev() {
            run(find("home_idle").unwrap());
            second.push(run(find(n).unwrap()));
        }
        second.reverse();
        assert_eq!(first, second);
    }

    // ---- guarantee 2: saving ------------------------------------------------------------------

    fn save_midway(name: &str, midway: u32) -> (Run, String) {
        let scenario = find(name).unwrap();
        let mut run = Run::new(scenario);
        while run.ticks_done() < midway && run.tick() {}
        let text = run.game().save_state().to_text();
        (run, text)
    }

    fn load(text: &str) -> Game {
        let (state, generator) = SaveState::from_text(text).unwrap();
        Game::from_save(state, generator).0
    }

    /// The pad's reload counter is bumped by the load itself; nothing else may differ.
    fn without_reload_counter(text: &str) -> String {
        text.replace("reloads: 1)", "reloads: 0)")
    }

    /// A save taken mid-run and loaded writes the same save text again (a fixed point, apart from
    /// the pad reload counter), and two loads of one save continue to identical digests at every
    /// checkpoint. An uninterrupted run and a loaded one are NOT digest-identical afterwards, by
    /// design: bodies, bullets and pursuits are ephemeral and regenerated from the seed (see
    /// `docs/PERSISTENCE.md`), ids restart and the ship gets its post-load shield time.
    #[test]
    fn save_load_is_a_fixed_point_and_loads_continue_identically() {
        for (name, midway) in [("ring3_busy", 900), ("civ_city", 900)] {
            let (original, text) = save_midway(name, midway);
            let scenario = original.scenario();
            let continue_from = |game: Game| {
                let mut run = Run::resume(scenario, game, midway);
                let mut digests = Vec::new();
                while run.tick() {
                    if run.ticks_done().is_multiple_of(300) {
                        digests.push(run.game().state_digest());
                    }
                }
                digests
            };
            let loaded = load(&text);
            assert_eq!(
                without_reload_counter(&loaded.save_state().to_text()),
                without_reload_counter(&text),
                "{name}: save text is a fixed point"
            );
            let a = continue_from(loaded);
            let b = continue_from(load(&text));
            assert_eq!(a, b, "{name}: two loads of one save diverged");
            // The documented limitation, asserted so a future change to it is noticed.
            let uninterrupted = continue_from(original.game);
            let last = uninterrupted.len() - 1;
            assert!(
                uninterrupted[last].differing(&a[last]).contains(&"bodies"),
                "{name}: an uninterrupted run unexpectedly matches a reloaded one; \
                 update docs/PERSISTENCE.md and docs/PERF.md"
            );
        }
    }

    // ---- guarantee 3: generation does not depend on visit order --------------------------------

    fn spawn_text(seed: u64, ids: &[SectorId]) -> Vec<String> {
        ids.iter()
            .map(|id| format!("{:?}", world::generate(seed, *id)))
            .collect()
    }

    /// Generating sectors in any order, on a cold thread (no warm caches), gives the same spawns.
    #[test]
    fn generation_is_independent_of_visit_order() {
        let ids: Vec<SectorId> = [(0, 0), (1, 0), (3, 1), (-3, -3), (0, -5), (4, -5), (-5, 4)]
            .iter()
            .map(|&(x, y)| SectorId { x, y })
            .collect();
        let seed = MASTER_SEED;
        let forward = {
            let ids = ids.clone();
            std::thread::spawn(move || spawn_text(seed, &ids))
                .join()
                .unwrap()
        };
        let reversed = {
            let mut rev = ids.clone();
            rev.reverse();
            let mut out = std::thread::spawn(move || spawn_text(seed, &rev))
                .join()
                .unwrap();
            out.reverse();
            out
        };
        assert_eq!(forward, reversed, "seed {seed:#x}");
    }

    /// Arriving at a sector by different routes loads the same population there: the same spawns
    /// with the same genomes, sizes, velocities and (for free bodies) positions. Known limitation,
    /// not asserted equal: `populate` draws each body's initial heading, wander and fire cooldown
    /// from the shared gameplay stream, so those (and the pose of life rooted on a rotated host)
    /// depend on how much the stream has been used when the sector loads.
    #[test]
    fn a_sector_loads_the_same_population_whatever_route_reached_it() {
        fn arrive(route: &[SectorId]) -> Vec<String> {
            let mut game = Game::new(MASTER_SEED);
            for id in route {
                game.teleport(id.center());
                game.stream_sectors();
            }
            let target = *route.last().unwrap();
            let mut bodies: Vec<_> = game
                .bodies
                .iter()
                .filter(|b| b.origin.is_some_and(|(sector, _)| sector == target))
                .map(|b| {
                    let position = if b.root.is_none() {
                        Some(b.position)
                    } else {
                        None
                    };
                    (
                        b.origin,
                        format!(
                            "{:?} {:?} {:?} {:?} {:?} {} {} {} {:?} {:?} {:?}",
                            b.kind,
                            position,
                            b.velocity,
                            b.genome,
                            b.genes,
                            b.radius,
                            b.health,
                            b.mass,
                            b.species,
                            b.rock,
                            b.pinned
                        ),
                    )
                })
                .collect();
            bodies.sort_by_key(|(origin, _)| origin.map(|(s, i)| (s.x, s.y, i)));
            assert!(!bodies.is_empty());
            bodies.into_iter().map(|(_, text)| text).collect()
        }
        let target = SectorId { x: 3, y: 1 };
        let a = SectorId { x: -4, y: 2 };
        let b = SectorId { x: 5, y: -3 };
        let direct = arrive(&[target]);
        assert_eq!(direct, arrive(&[a, target]));
        assert_eq!(direct, arrive(&[b, a, b, target]));
    }

    // ---- time steps ---------------------------------------------------------------------------

    fn stepped(name: &str, ticks: u32, dt: f32, per_tick: u32) -> Game {
        let scenario = find(name).unwrap();
        let mut run = Run::new(scenario);
        let mut game = std::mem::replace(&mut run.game, Game::new(1));
        let anchor = scenario.sector.center();
        for tick in 0..ticks {
            let input = (scenario.script)(tick, &mut game, anchor);
            for _ in 0..per_tick {
                game.step(dt, input);
            }
        }
        game
    }

    /// A step longer than 50 ms is clamped to exactly 50 ms; zero, negative and non-finite steps
    /// do nothing at all.
    #[test]
    fn long_steps_clamp_and_invalid_steps_are_ignored() {
        let long = stepped("ring3_busy", 120, 0.2, 1);
        let clamped = stepped("ring3_busy", 120, 0.05, 1);
        assert_eq!(long.state_digest(), clamped.state_digest());
        let mut game = stepped("ring3_busy", 30, DT, 1);
        let before = game.state_digest();
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            game.step(dt, Input::default());
        }
        assert_eq!(game.state_digest(), before);
    }
}
