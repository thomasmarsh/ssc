//! The SSC_* env-hook smoke and screenshot harness; no effect in regular play. Hook
//! semantics are documented in docs/HOOKS.md.
use crate::*;

/// Optional bounded renderer smoke run; no effect in regular play.
#[derive(Resource, Default)]
pub(crate) struct SmokeRun {
    pub(crate) frames: u32,
    pub(crate) requested: bool,
    /// Explicit specimen captures advance through their bounded hook, independent of GPU speed.
    pub(crate) hold: bool,
    /// `SSC_FARM_CIV` has posed the ship in the greenhouse.
    pub(crate) civ_staged: bool,
}

pub(crate) fn smoke_run(
    mut run: ResMut<SmokeRun>,
    mut session: ResMut<Session>,
    mut commands: Commands,
    offscreen: Option<Res<presentation::Offscreen>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(limit) = std::env::var("SSC_SMOKE_FRAMES")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    else {
        return;
    };
    smoke_view_hooks(&mut run, &mut session);
    smoke_placement_hooks(&mut run, &mut session);
    smoke_specimen_hooks(&mut run, &mut session);
    smoke_kit_hooks(&mut run, &mut session);
    smoke_capture_hooks(&mut run, &mut session, limit);
    smoke_bench_hooks(&mut run, &mut session, limit);
    smoke_run_hooks(&mut run, &mut session);
    run.frames += 1;
    if run.frames < limit || run.requested {
        return;
    }
    run.requested = true;
    if let Ok(path) = std::env::var("SSC_SCREENSHOT") {
        let shot = match &offscreen {
            Some(target) => Screenshot::image(target.0.clone()),
            None => Screenshot::primary_window(),
        };
        commands.spawn(shot).observe(save_to_disk(path)).observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
    } else {
        exit.write(AppExit::Success);
    }
}

/// Camera, style, panel and menu hooks applied at frame 0, plus SSC_DIE.
fn smoke_view_hooks(run: &mut SmokeRun, session: &mut Session) {
    // Exercise camera views in bounded runs without synthetic keyboard input.
    if run.frames == 0 {
        session.camera_view = match std::env::var("SSC_CAMERA").as_deref() {
            Ok("wide") => CameraView::Wide,
            Ok("far") => CameraView::Far,
            Ok("sector" | "quadrant") => CameraView::Sector,
            _ => session.camera_view,
        };
    }
    if run.frames == 0 {
        session.style = match std::env::var("SSC_STYLE").as_deref() {
            Ok("glow") => RenderStyle::Glow,
            Ok("neon") => RenderStyle::Neon,
            _ => session.style,
        };
    }
    // SSC_DETAILS=1, SSC_HELP=1, SSC_RADAR=1 and SSC_SETTINGS=1 open the details, the key list,
    // the radar and the settings screen.
    if run.frames == 0 {
        session.details |= std::env::var_os("SSC_DETAILS").is_some();
        session.help |= std::env::var_os("SSC_HELP").is_some();
        session.radar |= std::env::var_os("SSC_RADAR").is_some();
        if std::env::var_os("SSC_SETTINGS").is_some() {
            session.settings = Some(2);
            if std::env::var("SSC_SETTINGS").as_deref() == Ok("save") {
                session.settings = settings::Setting::ALL
                    .iter()
                    .position(|row| *row == settings::Setting::Save);
                settings::save_game(session);
            }
        }
    }
    // SSC_MENU=save|armed|new: show the title menu with a save, with its erase armed, or none.
    if run.frames == 0
        && let Ok(mode) = std::env::var("SSC_MENU")
    {
        let mut menu = titlemenu::TitleMenu::new(
            mode != "new",
            "saved game: score 4500, 3 lives, 12 min".into(),
        );
        if mode == "armed" {
            menu.step(1);
            menu.confirm();
        }
        session.menu = Some(menu);
    }
    // SSC_DIE=1: exhaust lives at frame 6 (checks pad recovery and its save).
    if run.frames == 6 && std::env::var_os("SSC_DIE").is_some() {
        session.game.lives = 1;
        if let Some(ship) = session
            .game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
        {
            ship.health = 0.0;
            ship.shield = 0.0;
        }
    }
}

/// Where the ship starts: developer panel, teleport, farm, civ greenhouse and the clock.
fn smoke_placement_hooks(run: &mut SmokeRun, session: &mut Session) {
    // SSC_DEV=1 SSC_DEV_CONSOLE=tuning|toggles opens the developer console on a tab, with
    // SSC_DEV_GROUP=<group>, SSC_DEV_SEARCH=<text> and SSC_DEV_MODIFIED=1 staging the filters and
    // SSC_DEV_DIALOG=reset|regen|search opening a dialog over it;
    // SSC_DEV_PANEL=<row> opens the toggles tab on that row (the older hook). SSC_DEV_ON=1 first
    // turns on the six switches and doubles the time scale (to check the console and DEV tag).
    if run.frames == 0 && ssc::simulation::dev::enabled() {
        use crate::ui::screens::console::{Console, Tab};
        use ssc::simulation::dev::DevRow;
        let staged_row = std::env::var("SSC_DEV_PANEL")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok());
        let tab = std::env::var("SSC_DEV_CONSOLE").ok();
        if staged_row.is_some() || tab.is_some() {
            if std::env::var_os("SSC_DEV_ON").is_some() {
                for row in DevRow::ALL.into_iter().take(6) {
                    session.game.dev_change(row, 0);
                }
                session.game.dev_change(DevRow::TimeScale, 1);
            }
            session.console = Some(match (tab.as_deref(), staged_row) {
                (Some("toggles"), row) | (None, row @ Some(_)) => {
                    Console::on_toggle_row(row.unwrap_or(0).min(DevRow::ALL.len() - 1))
                }
                _ => {
                    let console = Console::staged(
                        Tab::Tuning,
                        std::env::var("SSC_DEV_GROUP").ok().as_deref(),
                        &std::env::var("SSC_DEV_SEARCH").unwrap_or_default(),
                        std::env::var_os("SSC_DEV_MODIFIED").is_some(),
                    );
                    match std::env::var("SSC_DEV_DIALOG") {
                        Ok(name) => console.with_dialog(&name),
                        Err(_) => console,
                    }
                }
            });
        }
    }
    // Smoke runs can start somewhere interesting: SSC_TELEPORT="x,y" (invulnerable).
    if run.frames == 0
        && let Some((x, y)) = std::env::var("SSC_TELEPORT")
            .ok()
            .and_then(|v| v.split_once(',').map(|(x, y)| (x.to_owned(), y.to_owned())))
        && let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>())
    {
        session.game.teleport(Vec2::new(x, y));
        session.game.player_invulnerability = 1e9;
    }
    // SSC_FARM=stage|plant: pose the ship beside HOME's crop with seeds and biomass in hand
    // (`plant` also plants one); SSC_FARM_AGE=<seconds> then ages the clock (a ripe crop).
    if run.frames == 0
        && let Ok(mode) = std::env::var("SSC_FARM")
    {
        let age = std::env::var("SSC_FARM_AGE")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(0.0);
        // SSC_FARM_GENES="yield,vigor,hardy,hue" (each -100 to 100) gives the staged seeds genes.
        let genes = std::env::var("SSC_FARM_GENES")
            .ok()
            .map(|v| {
                let mut g = v
                    .split(',')
                    .map(|n| n.trim().parse::<i8>().unwrap_or(0).clamp(-100, 100));
                let mut next = || g.next().unwrap_or(0);
                ssc::flora::CropGenes {
                    yield_: next(),
                    vigor: next(),
                    hardy: next(),
                    hue: next(),
                }
            })
            .unwrap_or_default();
        let at = session.game.stage_farm(mode == "plant", age, genes);
        eprintln!("SSC_FARM staged at {at:?}");
        // SSC_FARM_BLIGHT=1: every plant is sick (drawn dull and mottled; HOME never drains).
        if std::env::var_os("SSC_FARM_BLIGHT").is_some() {
            session.game.blight_all();
        }
        session.game.player_invulnerability = 1e9;
    }
    // SSC_FARM_CIV=1: fly to the early outpost (a farming settlement) and stand inside its
    // greenhouse with seeds, a friendly regard and a full-ish granary. The world loads first,
    // so it tries each frame until the glass exists.
    if !run.civ_staged
        && run.frames < 240
        && std::env::var_os("SSC_FARM_CIV").is_some()
        && let Some(at) = session.game.stage_civ_farm()
    {
        eprintln!("SSC_FARM_CIV staged at {at:?}");
        run.civ_staged = true;
    }
    // SSC_TIME=<seconds>: start the game clock there (wells and anything else posed by time).
    if run.frames == 0
        && let Some(time) = std::env::var("SSC_TIME")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
    {
        session.game.time = time;
    }
}

/// Specimen, payload gallery and doorway gallery staging.
fn smoke_specimen_hooks(run: &mut SmokeRun, session: &mut Session) {
    // SSC_SPECIMEN=skipjack|veilwing|hullpick: three specimens of a rare power, a few hundred
    // units from the ship (use with SSC_TELEPORT somewhere past ring 3).
    if run.frames == 0
        && let Ok(name) = std::env::var("SSC_SPECIMEN")
    {
        use ssc::genome::Species;
        let genome = ssc::simulation::dev::specimen_genome(&name);
        let near = if matches!(name.as_str(), "stormcap" | "dizzard" | "multijammer") {
            200.0
        } else if name == "hullworm" {
            30.0
        } else if name == "oozer" {
            170.0
        } else if name == "remora" {
            100.0
        } else {
            420.0
        };
        let near = std::env::var("SSC_SPECIMEN_NEAR")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(near);
        if matches!(
            name.as_str(),
            "stormcap"
                | "dizzard"
                | "argus"
                | "dirgewhale"
                | "hullworm"
                | "slinger"
                | "oozer"
                | "longslinger"
                | "softslinger"
                | "wildslinger"
                | "wildsong"
                | "wildmulti"
                | "multijammer"
                | "multioozer"
        ) {
            // The jammers only work on a ship that is not in grace.
            session.game.player_invulnerability = 0.0;
        }
        if matches!(
            name.as_str(),
            "multijammer"
                | "multioozer"
                | "longslinger"
                | "softslinger"
                | "wildslinger"
                | "wildsong"
                | "wildmulti"
        ) {
            // A bounded authored encounter: load first, then remove wild threats so the
            // warning capture cannot lose the observer or relocate on death recovery.
            session.game.player_invulnerability = 1e9;
            session.game.step(0.02, ssc::simulation::Input::default());
            session.game.player_invulnerability = 0.0;
            session
                .game
                .bodies
                .retain(|b| b.kind == ssc::simulation::BodyKind::Player);
            session.game.tethers.clear();
            run.hold = true;
        }
        if name == "slinger" {
            // Load the destination, then isolate a readable authored encounter from wild threats.
            session.game.player_invulnerability = 1e9;
            session.game.step(0.02, ssc::simulation::Input::default());
            session.game.player_invulnerability = 0.0;
            let rocks: Vec<_> = session
                .game
                .bodies
                .iter()
                .filter(|b| b.kind == ssc::simulation::BodyKind::Asteroid && !b.pinned)
                .take(3)
                .map(|b| b.id)
                .collect();
            session
                .game
                .bodies
                .retain(|b| b.kind == ssc::simulation::BodyKind::Player || rocks.contains(&b.id));
            session.game.tethers.clear();
        }
        if name == "oozer" && std::env::var_os("SSC_OOZER_ISOLATE").is_some() {
            // Only the ship and two free stones remain, so nothing else interferes.
            session.game.player_invulnerability = 1e9;
            session.game.step(0.02, ssc::simulation::Input::default());
            // A gate run is about the squeeze, so the ship is safe from the blob.
            session.game.player_invulnerability = if std::env::var_os("SSC_OOZER_GATE").is_some() {
                1e9
            } else {
                0.0
            };
            let rocks: Vec<_> = session
                .game
                .bodies
                .iter()
                .filter(|b| b.kind == ssc::simulation::BodyKind::Asteroid && !b.pinned)
                .take(2)
                .map(|b| b.id)
                .collect();
            session
                .game
                .bodies
                .retain(|b| b.kind == ssc::simulation::BodyKind::Player || rocks.contains(&b.id));
        }
        if name == "slinger"
            && (std::env::var_os("SSC_SPECIMEN_TELL").is_some()
                || std::env::var_os("SSC_SPECIMEN_THROW").is_some())
        {
            run.hold = true;
        }
        let ship = session.game.player().map_or(Vec2::ZERO, |p| p.position);
        let count = if matches!(
            name.as_str(),
            "weaver"
                | "slinger"
                | "runekeeper"
                | "seamer"
                | "oozer"
                | "multijammer"
                | "multioozer"
                | "longslinger"
                | "softslinger"
                | "wildslinger"
                | "wildsong"
                | "wildmulti"
        ) {
            1
        } else {
            3
        };
        for k in 0..count {
            let at = ship + Vec2::from_angle(0.6 + k as f32 * 2.1) * (near + 90.0 * k as f32);
            let id = session.game.place_creature(&Species::of(genome), at);
            if matches!(
                name.as_str(),
                "multijammer"
                    | "multioozer"
                    | "longslinger"
                    | "softslinger"
                    | "wildslinger"
                    | "wildsong"
                    | "wildmulti"
            ) && let Some(body) = session.game.bodies.iter_mut().find(|b| b.id == id)
            {
                body.pinned = true;
                body.alert = true;
            }
            if name == "oozer" {
                // SSC_OOZER_FED=<0..1> starts it grown; SSC_OOZER_GATE=<gap> walls the way.
                let env = |k: &str| {
                    std::env::var(k)
                        .ok()
                        .and_then(|v| v.trim().parse::<f32>().ok())
                };
                session
                    .game
                    .dev_stage_ooze(id, env("SSC_OOZER_FED"), env("SSC_OOZER_GATE"));
                // Two free stones beside it, so the skin and the digesting contents show.
                let rocks: Vec<u64> = session
                    .game
                    .bodies
                    .iter()
                    .filter(|b| {
                        b.kind == ssc::simulation::BodyKind::Asteroid
                            && !b.pinned
                            && b.rock != ssc::world::RockKind::Planetoid
                    })
                    .take(2)
                    .map(|b| b.id)
                    .collect();
                for (i, rock) in session
                    .game
                    .bodies
                    .iter_mut()
                    .filter(|b| rocks.contains(&b.id))
                    .enumerate()
                {
                    // On its way to the ship, so it eats them as it crawls.
                    rock.position = ship + (at - ship) * (0.8 - 0.1 * i as f32);
                    rock.velocity = Vec2::ZERO;
                    rock.radius = 9.0;
                }
            }
            if matches!(name.as_str(), "weaver" | "slinger") {
                // Stage a stationary web with free stones, so bounded screenshots do not
                // depend on the sector happening to put rocks beside the specimen.
                for body in session.game.bodies.iter_mut().filter(|b| b.id == id) {
                    body.pinned = true;
                    if name == "slinger" {
                        body.alert = true;
                    }
                }
                for (i, rock) in session
                    .game
                    .bodies
                    .iter_mut()
                    .filter(|b| {
                        b.kind == ssc::simulation::BodyKind::Asteroid
                            && !b.pinned
                            && b.rock != ssc::world::RockKind::Planetoid
                    })
                    .take(3)
                    .enumerate()
                {
                    rock.position = at + Vec2::from_angle(-1.2 + i as f32 * 1.2) * 300.0;
                    rock.velocity = Vec2::ZERO;
                    if name == "slinger" {
                        rock.radius = 22.0;
                        rock.mass = 25.0;
                        rock.rock = ssc::world::RockKind::Plain;
                    }
                }
            }
        }
    }
    // Bounded four-payload gallery. Freeze the staged instant for screenshot comparison.
    if run.frames == 0
        && let Ok(mode) = std::env::var("SSC_RUNE")
        && matches!(mode.as_str(), "arming" | "activation")
    {
        use ssc::genome::{Genome, Species};
        use ssc::simulation::{BodyKind, Mine, Payload, Sigil};
        let game = &mut session.game;
        game.player_invulnerability = 1e9;
        game.step(0.02, ssc::simulation::Input::default());
        game.bodies.retain(|b| b.kind == BodyKind::Player);
        game.mines.clear();
        game.tethers.clear();
        game.player_invulnerability = 0.0;
        let ship = game.player().map_or(Vec2::ZERO, |p| p.position);
        for (i, payload) in [Payload::Blast, Payload::Slow, Payload::Push, Payload::Jam]
            .into_iter()
            .enumerate()
        {
            let position = ship
                + Vec2::new(
                    if i % 2 == 0 { -220.0 } else { 220.0 },
                    if i < 2 { 155.0 } else { -155.0 },
                );
            let mut genome = Genome::runekeeper();
            genome.rune = (91.125 + i as f32 * 0.25) / 131.0;
            let owner = game.place_creature(&Species::of(genome), position + Vec2::new(0.0, 140.0));
            for body in game.bodies.iter_mut().filter(|b| b.id == owner) {
                body.pinned = true;
            }
            game.mines.push(Mine {
                sigil: Some(Sigil {
                    owner,
                    payload,
                    shot: mode == "activation",
                    fresh: false,
                }),
                position,
                velocity: Vec2::ZERO,
                friendly: false,
                age: 0.0,
                fuse: Some(1.2),
                damage: 40.0,
                blast: 90.0,
            });
        }
        let steps = if mode == "activation" { 86 } else { 36 };
        for _ in 0..steps {
            game.step(1.0 / 60.0, ssc::simulation::Input::default());
        }
        assert_eq!(
            game.mines.len(),
            if mode == "activation" { 0 } else { 4 },
            "Rune gallery failed to stage"
        );
        if mode == "activation" {
            assert_eq!(game.rune_fields.len(), 4);
        }
        run.hold = true;
    }
    // Bounded doorway galleries hold simulation after a precise warning/open/transit pose.
    if run.frames == 0
        && let Ok(mode) = std::env::var("SSC_RIFT")
        && matches!(mode.as_str(), "warning" | "active" | "transit")
    {
        use ssc::genome::{Genome, Species};
        use ssc::simulation::{BodyKind, Bullet, Rift};
        let game = &mut session.game;
        game.player_invulnerability = 1e9;
        game.step(0.02, ssc::simulation::Input::default());
        game.bodies.retain(|b| b.kind == BodyKind::Player);
        game.bullets.clear();
        game.mines.clear();
        game.tethers.clear();
        let ship = game.player().unwrap().position;
        let owner =
            game.place_creature(&Species::of(Genome::seamer()), ship + Vec2::new(0.0, 200.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == owner)
            .unwrap()
            .pinned = true;
        let a = ship + Vec2::new(-475.0, 0.0);
        let b = ship + Vec2::new(475.0, 0.0);
        game.rifts.push(Rift {
            owner,
            a,
            b,
            warning: 1.2,
            left: 8.0,
        });
        let ticks = if mode == "warning" { 36 } else { 78 };
        for _ in 0..ticks {
            game.step(1.0 / 60.0, ssc::simulation::Input::default());
        }
        if mode == "transit" {
            let player = game
                .bodies
                .iter_mut()
                .find(|b| b.kind == BodyKind::Player)
                .unwrap();
            player.position = a - Vec2::X * 76.0;
            player.velocity = Vec2::X * 460.0;
            game.bullets.push(Bullet::hostile(
                a - Vec2::X * 100.0 + Vec2::Y * 32.0,
                Vec2::X * 6000.0,
                2.0,
                10.0,
            ));
            game.step(1.0 / 60.0, ssc::simulation::Input::default());
            assert!(!game.rift_traces.is_empty(), "Rift transit gallery failed");
        }
        // Shots across the connection demonstrate that its faint thread does not hide fire.
        for k in 0..8 {
            game.bullets.push(Bullet::hostile(
                ship + Vec2::new(-240.0 + k as f32 * 65.0, -30.0),
                Vec2::Y * 300.0,
                2.0,
                10.0,
            ));
        }
        assert_eq!(game.rifts.len(), 1, "Rift gallery failed");
        run.hold = true;
    }
    if run.frames == 0
        && let Ok(mode) = std::env::var("SSC_DISCOVERY")
    {
        assert!(
            session.game.stage_discovery_smoke(&mode),
            "unknown discovery gallery"
        );
        run.hold = true;
    }
}

/// Organs and the other kit and state staging before the capture window.
fn smoke_kit_hooks(run: &mut SmokeRun, session: &mut Session) {
    // SSC_ORGANS=1: own the four organs with two slots fitted, a bond running, and a hold to
    // pay the upkeep, to check the HUD icons, the details and the SKILLS tab.
    if run.frames == 0 && std::env::var_os("SSC_ORGANS").is_some() {
        use ssc::genome::Genome;
        use ssc::simulation::organs::{Organ, Strain};
        use ssc::simulation::skills::Skill;
        let game = &mut session.game;
        game.loadout.skills.raise(Skill::Symbiosis);
        game.loadout.skills.raise(Skill::Symbiosis);
        for (organ, genome) in [
            (Organ::Remora, Genome::remora()),
            (Organ::Faraday, Genome::stormcap()),
            (Organ::Veil, Genome::veilwing()),
            (Organ::Skipjack, Genome::skipjack()),
        ] {
            game.loadout
                .organs
                .acquire(Strain::from_donor(organ, &genome), &game.tune);
        }
        game.loadout.organs.acquire(
            Strain::from_donor(Organ::Veil, &Genome::veilwing()),
            &game.tune,
        );
        game.cargo = Cargo {
            metal: 120.0,
            volatiles: 150.0,
            crystal: 90.0,
            ..Default::default()
        };
        let _ = game.bench_organ(Organ::Veil);
        let _ = game.bench_organ(Organ::Faraday);
    }
    if run.frames == 0
        && let Ok(mode) = std::env::var("SSC_BENCH_VIEW")
    {
        let game = &mut session.game;
        let at = game
            .pads()
            .find(|pad| pad.home)
            .map(|pad| game.pad_position(pad))
            .expect("HOME pad for bench gallery");
        game.teleport(at);
        game.pad_action();
        game.bench_toggle();
        if let Some(ship) = game.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.health = ship.max_health * 0.6;
        }
        smoke_bench(game, &mode);
        run.hold = true;
    }
    if run.frames == 0
        && let Ok(seconds) = std::env::var("SSC_FLEET_FLIGHT")
    {
        let game = &mut session.game;
        let at = game
            .pads()
            .find(|pad| pad.home)
            .map(|pad| game.pad_position(pad))
            .unwrap();
        game.teleport(at);
        game.pad_action();
        game.bench_toggle();
        smoke_bench(game, "mining-fleet-status");
        let designated = std::env::var("SSC_FLEET_DEPOSIT").as_deref() == Ok("1");
        if designated && let Some(action) = game.stage_drone_deposit_smoke() {
            game.bench_select(action);
            game.bench_confirm();
        }
        game.cargo.fuel = 4.0;
        game.bench_select(ssc::simulation::BenchAction::Stash(
            ssc::simulation::Material::Fuel,
        ));
        game.bench_confirm();
        game.bench_toggle();
        let seconds = seconds
            .parse::<f32>()
            .unwrap_or(5.0)
            .clamp(0.0, if designated { 60.0 } else { 15.0 });
        for _ in 0..(seconds * 60.0) as usize {
            game.step(1.0 / 60.0, Input::default());
        }
        // Settle the endpoint despite accumulated fixed-step rounding.
        if !designated && seconds >= 15.0 {
            game.step(1.0 / 60.0, Input::default());
        }
        if std::env::var("SSC_FLEET_RECALL").as_deref() == Ok("1") {
            game.bench_toggle();
            game.bench_select(ssc::simulation::BenchAction::RecallDroneFleet);
            game.bench_confirm();
            game.bench_toggle();
        }
        match std::env::var("SSC_FLEET_LOSS").as_deref() {
            Ok("1") => game.stage_drone_loss_smoke(),
            Ok("blast") => game.stage_drone_blast_smoke(),
            Ok("impact") => game.stage_drone_impact_smoke(),
            _ => {}
        }
        run.hold = true;
    }
}

/// Hooks timed just before the screenshot: jam, steps, specimen tells, hits.
fn smoke_capture_hooks(run: &mut SmokeRun, session: &mut Session, limit: u32) {
    // SSC_JAM=emp|confuse|glitch|hud: just before the screenshot, jam the ship (with the dash
    // and parry owned, so their rings show).
    if run.frames + 36 == limit
        && let Ok(kind) = std::env::var("SSC_JAM")
    {
        use ssc::simulation::JamSystem;
        use ssc::simulation::skills::Skill;
        session.game.loadout.skills.raise(Skill::Dash);
        session.game.loadout.skills.raise(Skill::Parry);
        session.game.player_invulnerability = 0.0;
        match kind.as_str() {
            "emp" => {
                session
                    .game
                    .apply_jam(&[JamSystem::Weapons, JamSystem::Dash], 1.5);
            }
            "hud" => {
                session
                    .game
                    .apply_jam(&[JamSystem::Parry, JamSystem::Hud], 1.5);
            }
            "confuse" => {
                session.game.apply_confuse(0.6, false, 1.5, 1.0);
            }
            _ => {
                let ok = session.game.apply_glitch(2.0, 5);
                let _ = ok;
            }
        }
        session.game.player_invulnerability = 1e9;
    }
    // SSC_SPECIMEN_HURT=<0..1>: hurt every jointed head by that share of its hull, so a pooled
    // body shows it shedding pieces (see `breakup`); use with SSC_SPECIMEN=serpent.
    if run.frames + 3 == limit
        && let Some(share) = std::env::var("SSC_SPECIMEN_HURT")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
    {
        for body in session
            .game
            .bodies
            .iter_mut()
            .filter(|b| b.chain.is_some() && !b.follower)
        {
            body.health -= body.max_health * share;
        }
    }
    // SSC_STEPS=<seconds>: just before the screenshot, run the game that many seconds ahead (a
    // remora needs a few calm seconds before its grooming ring shows).
    if run.frames + 3 == limit
        && let Some(seconds) = std::env::var("SSC_STEPS")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
    {
        for _ in 0..(seconds / 0.02) as usize {
            session.game.step(0.02, ssc::simulation::Input::default());
        }
    }
    // SSC_AT_STRUCTURE=1: after SSC_STEPS, put the ship beside the biggest structure raised.
    if run.frames + 3 == limit
        && std::env::var_os("SSC_AT_STRUCTURE").is_some()
        && let Some(at) = session.game.structure_focus()
    {
        session.game.teleport(at);
    }
    // SSC_SPECIMEN_TELL=1: advance to the specimen warning before the screenshot.
    if run.frames + 3 == limit && std::env::var_os("SSC_SPECIMEN_TELL").is_some() {
        for _ in 0..4000 {
            session.game.step(0.02, ssc::simulation::Input::default());
            let told = if std::env::var("SSC_SPECIMEN").as_deref() == Ok("slinger") {
                session
                    .game
                    .bodies
                    .iter()
                    .any(|b| b.pinned && session.game.power_view(b).sling.is_some())
            } else {
                session
                    .game
                    .bodies
                    .iter()
                    .any(|b| session.game.power_view(b).sling.is_some())
                    || session.game.song_rings().iter().any(|r| r.radius > 200.0)
                    || session
                        .game
                        .tethers
                        .iter()
                        .any(|t| t.kind == ssc::simulation::TetherKind::Web && t.warning > 0.0)
                    || session.game.bodies.iter().any(|b| {
                        session
                            .game
                            .power_view(b)
                            .blink
                            .is_some_and(|t| t.progress() > 0.5)
                            || session.game.power_view(b).shove_age.clamp(0.2, 0.3)
                                == session.game.power_view(b).shove_age
                            || session
                                .game
                                .power_view(b)
                                .jams
                                .into_iter()
                                .flatten()
                                .any(|t| t.progress() > 0.5)
                    })
            };
            if told {
                break;
            }
        }
        if std::env::var("SSC_SPECIMEN").as_deref() == Ok("slinger") {
            assert!(
                session
                    .game
                    .bodies
                    .iter()
                    .any(|b| b.pinned && session.game.power_view(b).sling.is_some()),
                "Slinger smoke capture did not reach a warning"
            );
        }
    }
    // SSC_SPECIMEN_THROW=1: capture a launched rock after a short visible flight.
    if run.frames + 3 == limit && std::env::var_os("SSC_SPECIMEN_THROW").is_some() {
        for _ in 0..4000 {
            session.game.step(0.02, ssc::simulation::Input::default());
            if session.game.bodies.iter().any(|b| b.sling_thrown > 0.0) {
                for _ in 0..6 {
                    session.game.step(0.02, ssc::simulation::Input::default());
                }
                break;
            }
        }
        assert!(
            session.game.bodies.iter().any(|b| b.sling_thrown > 0.0),
            "Slinger smoke capture did not reach a throw"
        );
    }

    if run.frames == 0 && std::env::var_os("SSC_FRONTIER_CONTACT").is_some() {
        session.game.pose_frontier_contact();
    }
    if run.frames == 0
        && let Ok(view) = std::env::var("SSC_CONTACT_JOB")
    {
        if view == "pest" || view == "pest-target" {
            session.game = Game::new(42);
        }
        session.game.pose_frontier_contact();
        if view == "culture" {
            session.game.pose_contact_culture();
        } else if view == "agreement" {
            session.game.pose_contact_agreement();
        } else if view == "partnership" {
            session.game.pose_contact_partnership();
        } else if view == "pest-target" {
            session.game.pose_pest_target();
        } else {
            session.game.pose_contact_job(match view.as_str() {
                "survey" => ssc::simulation::jobs::JobKind::Survey,
                "pest" => ssc::simulation::jobs::JobKind::Pest,
                _ => ssc::simulation::jobs::JobKind::Fuel,
            });
        }
    }
    // SSC_OUTPOST=1: start at the early outpost's capital (standing meter, tithe seat).
    if run.frames == 0 && std::env::var_os("SSC_OUTPOST").is_some() {
        let capital = ssc::territory::outpost(session.game.seed()).capital;
        session.game.teleport(capital.center());
        session.game.player_invulnerability = 1e9;
    }
    // SSC_HIT=<degrees>: a hostile shot from that direction lands just before the screenshot,
    // to show the damage direction mark and the hit feel.
    if run.frames + 20 == limit
        && let Some(degrees) = std::env::var("SSC_HIT")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
        && let Some(ship) = session.game.player().map(|p| p.position)
    {
        let from = Vec2::from_angle(degrees.to_radians()) * 34.0;
        session.game.player_invulnerability = 0.0;
        session.game.bullets.push(ssc::simulation::Bullet::hostile(
            ship + from,
            -from.normalize() * 900.0,
            2.0,
            30.0,
        ));
    }
}

/// Bench galleries, receipts, purchases and kills near capture time.
pub(crate) fn smoke_bench_hooks(run: &mut SmokeRun, session: &mut Session, limit: u32) {
    // Physical controller input can reopen the landed bench during held flight captures.
    if std::env::var_os("SSC_FLEET_FLIGHT").is_some() && session.game.bench_open() {
        session.game.bench_toggle();
    }
    // A bounded receipt gallery confirms real actions near capture time.
    // Keep fleet galleries on their requested row despite physical input arriving during staging.
    if let Ok(mode) = std::env::var("SSC_BENCH_VIEW") {
        match mode.as_str() {
            "mining-fleet" => session
                .game
                .bench_select(ssc::simulation::BenchAction::MiningDrone),
            "mining-fleet-template" => {
                session
                    .game
                    .bench_select(ssc::simulation::BenchAction::DroneTemplate(
                        ssc::simulation::fleet::DroneUpgrade::Cargo,
                    ));
            }
            "mining-fleet-deposit" | "mining-fleet-rock" => {
                let action = if mode == "mining-fleet-rock" {
                    session.game.stage_drone_rock_smoke()
                } else {
                    session.game.stage_drone_deposit_smoke()
                };
                if let Some(action) = action {
                    session.game.bench_select(action);
                }
            }
            "mining-fleet-repair" => session
                .game
                .bench_select(ssc::simulation::BenchAction::RepairDrone(1)),
            "mining-fleet-recall" => session
                .game
                .bench_select(ssc::simulation::BenchAction::RecallDroneFleet),
            "mining-fleet-pause" => session
                .game
                .bench_select(ssc::simulation::BenchAction::PauseDroneFleet),
            "mining-fleet-role" => session
                .game
                .bench_select(ssc::simulation::BenchAction::CycleDroneRole),
            "mining-fleet-blueprint" => session
                .game
                .bench_select(ssc::simulation::BenchAction::ApplyDroneBlueprint),
            "mining-fleet-retrofit" => {
                let last = session.game.tune.fleet_max_drones - 1;
                session
                    .game
                    .bench_select(ssc::simulation::BenchAction::DroneUpgrade(
                        last,
                        ssc::simulation::fleet::DroneUpgrade::Cargo,
                    ))
            }
            "mining-fleet-status" => {
                let last = session.game.tune.fleet_max_drones - 1;
                session
                    .game
                    .bench_select(ssc::simulation::BenchAction::MiningDroneStatus(last))
            }
            _ => {}
        }
    }
    if run.frames + 20 == limit && std::env::var_os("SSC_BENCH_RESULT").is_some() {
        let mode = std::env::var("SSC_BENCH_VIEW").unwrap_or_default();
        if !matches!(mode.as_str(), "gate" | "parts" | "raw-input") {
            session.game.cargo = Cargo {
                metal: 200.0,
                volatiles: 200.0,
                crystal: 200.0,
                ..default()
            };
        }
        session.game.bench_confirm();
        if mode == "repeated" {
            session.game.bench_confirm();
            session.game.bench_confirm();
        }
    }
    // SSC_BUY=1: buy the bench's selected row just before the screenshot (a purchase ring).
    // SSC_KILL=1: destroy the nearest creatures just before it (floating scores, kill rings).
    if run.frames + 20 == limit && std::env::var_os("SSC_BUY").is_some() {
        session.game.bench_confirm();
    }
    if run.frames + 20 == limit && std::env::var_os("SSC_KILL").is_some() {
        let at = session.game.player().map_or(Vec2::ZERO, |p| p.position);
        let mut near: Vec<_> = session
            .game
            .bodies
            .iter_mut()
            .filter(|b| b.kind == BodyKind::Creature && !b.follower)
            .collect();
        near.sort_by(|a, b| a.position.distance(at).total_cmp(&b.position.distance(at)));
        for (n, body) in near.into_iter().take(3).enumerate() {
            body.position = at + Vec2::from_angle(0.9 + n as f32 * 2.1) * (180.0 + 60.0 * n as f32);
            body.health = 0.0;
        }
    }
}

/// Hurt, abilities, ping, chart, arm, pad, summary and held-input hooks.
fn smoke_run_hooks(run: &mut SmokeRun, session: &mut Session) {
    // SSC_HURT=<fraction>: set hull and shield to that fraction (to check the rings).
    // SSC_ABILITIES=1: unlock parry and dash, then use them so their rings are cooling.
    if run.frames == 6
        && let Some(fraction) = std::env::var("SSC_HURT")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
        && let Some(ship) = session
            .game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
    {
        ship.health = ship.max_health * fraction;
        ship.shield = ship.max_shield * fraction * 0.6;
    }
    if std::env::var_os("SSC_ABILITIES").is_some() {
        use ssc::simulation::skills::Skill;
        match run.frames {
            2 => {
                session.game.loadout.skills.raise(Skill::Parry);
                session.game.loadout.skills.raise(Skill::Dash);
            }
            8 => {
                session.game.dash(None);
            }
            _ => {}
        }
    }
    // SSC_PING=1: ping once the world has settled, to check the ring and echo markers.
    if run.frames == 20 && std::env::var_os("SSC_PING").is_some() {
        session.game.ping();
    }
    // SSC_CHART=1: buy the sonar tiers and a beacon, drop a beacon and a pin, ping, and open
    // the star map once the echoes are in (frame 90), to check the chart panel.
    if std::env::var_os("SSC_CHART").is_some() {
        smoke_chart(session, run.frames);
    }
    // SSC_ARM=<threat>: kit the ship out and scatter samples of every kind of drop, so the
    // equipment art and pickups can be checked without playing for them.
    if run.frames == 0
        && let Some(grade) = std::env::var("SSC_ARM")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
    {
        arm_for_smoke(&mut session.game, grade);
    }
    // SSC_PAD=kit|deploy|land|bench: stage the pad states beside the nearest planetoid (a kit
    // in hand, a pad down, landed, or landed with the bench open on tab SSC_BENCH=0..2).
    if run.frames == 4
        && let Ok(mode) = std::env::var("SSC_PAD")
    {
        smoke_pads(&mut session.game, &mode);
    }
    // SSC_SUMMARY=over|death: stage a run (with a few extirpations) and show its summary.
    if run.frames == 4
        && let Ok(mode) = std::env::var("SSC_SUMMARY")
    {
        smoke_summary(session, &mode);
    }
    // SSC_ELECTROLYSIS=1: use the real brake + mine input with water and an empty fuel tank.
    if std::env::var_os("SSC_ELECTROLYSIS").is_some() {
        if run.frames == 2 {
            session.game.cargo.water = 20.0;
            session.game.cargo.fuel = 0.0;
        }
        if run.frames > 2 {
            session.input.brake = true;
            session.input.mine = true;
            session.input.fire = false;
        }
    }
    // SSC_MINE=1: hold the mining beam on the nearest free rock (aimed at it each frame).
    if std::env::var_os("SSC_MINE").is_some() && run.frames > 2 {
        let ship = session.game.player().map(|p| p.position);
        let rock = session
            .game
            .bodies
            .iter()
            .filter(|b| b.kind == ssc::simulation::BodyKind::Asteroid && !b.pinned)
            .min_by(|a, b| {
                let at = ship.unwrap_or(Vec2::ZERO);
                at.distance(a.position).total_cmp(&at.distance(b.position))
            })
            .map(|b| (b.position, b.radius));
        if let (Some(at), Some((rock, radius))) = (ship, rock) {
            let toward = (rock - at).normalize_or_zero();
            if at.distance(rock) > radius + 150.0 {
                session.game.teleport(rock - toward * (radius + 150.0));
            }
            session.input.mine = true;
            session.input.fire = false;
        }
    }
    // SSC_FARM_BEAM=1 (with SSC_FARM=stage): hold the beam over the crop.
    if std::env::var_os("SSC_FARM_BEAM").is_some() && run.frames > 2 {
        session.input.mine = true;
        session.input.fire = false;
    }
    if std::env::var_os("SSC_SHIP_VIEW").is_some() {
        session.game.player_invulnerability = 0.0;
    }
}

fn smoke_chart(session: &mut Session, frame: u32) {
    use ssc::simulation::skills::Skill;
    let game = &mut session.game;
    match frame {
        0 => {
            for skill in [
                Skill::EchoLodes,
                Skill::EchoNests,
                Skill::EchoPredators,
                Skill::EchoPads,
                Skill::PingReach,
                Skill::PingReach,
                Skill::PingTargets,
                Skill::PingTargets,
                Skill::Beacon,
                Skill::Beacon,
            ] {
                game.loadout.skills.raise(skill);
            }
            game.cargo = Cargo {
                metal: 200.0,
                volatiles: 200.0,
                crystal: 200.0,
                ..Default::default()
            };
        }
        4 => {
            let _ = game.deploy_beacon();
            game.chart_pin(ssc::world::SectorId { x: 1, y: 1 }, PinLabel::Camp);
            game.chart_pin(ssc::world::SectorId { x: -2, y: 0 }, PinLabel::Danger);
            let focus = std::env::var("SSC_CHART_FOCUS").unwrap_or_default();
            let target = if focus == "civ" {
                (-30..30)
                    .flat_map(|x| (-30..30).map(move |y| SectorId { x, y }))
                    .filter_map(|id| {
                        ssc::world::territory(game.seed(), id).filter(|t| t.capital == id)
                    })
                    .find(|t| t.capital != ssc::territory::outpost(game.seed()).capital)
                    .map_or(Vec2::ZERO, |t| t.capital.center())
            } else {
                Vec2::new(4.0 * SECTOR_SIZE, 2.0 * SECTOR_SIZE)
            };
            game.teleport(target);
        }
        20 => {
            game.ping();
        }
        90 => {
            let focus = match std::env::var("SSC_CHART_FOCUS").as_deref() {
                Ok("home") => SectorId::ORIGIN,
                Ok("civ") => game.sector(),
                _ => SectorId { x: 2, y: 1 },
            };
            session.chart = Some(ChartCursor {
                sector: focus,
                label: PinLabel::Camp,
                center: focus,
                zoom: std::env::var("SSC_CHART_ZOOM")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(3)
                    .min(5),
            });
        }
        _ => {}
    }
}

/// Fills in a plausible run and ends it (`over`) or shows the per-life recap (`death`).
fn smoke_summary(session: &mut Session, mode: &str) {
    let game = &mut session.game;
    let run = &mut game.run;
    for n in 0..17 {
        run.visit(
            ssc::world::SectorId { x: n % 5, y: n / 5 },
            (n % 5 + n / 5) as f32,
        );
    }
    run.kills = 41;
    run.by_species = vec![(1, "BOGEY".into(), 25), (2, "KRAZOX".into(), 9)];
    run.elders = 1;
    run.bases = 2;
    run.eggs = 3;
    run.juveniles = 2;
    run.lost_to_nature = 4;
    run.mined = [80.0, 31.0, 9.0];
    run.rocks_depleted = 5;
    run.shots = 450;
    run.damage_dealt = 3200.0;
    run.damage_taken = 410.0;
    run.perfect_parries = 6;
    run.dashes = 12;
    run.deaths = if mode == "over" { 3 } else { 1 };
    run.weapons = 3;
    run.parts = 6;
    run.pads = 1;
    run.distance = 38_200.0;
    run.extirpated = vec![
        ssc::simulation::run::Extirpation {
            name: "BOGEY".into(),
            lineage: 1,
            sectors: 7,
            at: ssc::world::SectorId { x: 0, y: 0 },
            depth: 0.0,
        },
        ssc::simulation::run::Extirpation {
            name: "KRAZOX".into(),
            lineage: 2,
            sectors: 2,
            at: ssc::world::SectorId { x: 3, y: 1 },
            depth: 3.0,
        },
    ];
    game.score = 4500;
    game.time = 192.0;
    if mode == "over" {
        game.lives = 0;
        game.game_over = true;
        game.cargo = Cargo {
            metal: 90.0,
            volatiles: 40.0,
            crystal: 12.0,
            ..Default::default()
        };
        game.seal_bequest(Vec2::new(5.0 * SECTOR_SIZE, 2.0 * SECTOR_SIZE), None);
    } else {
        game.lives = 2;
        game.run.recap = 1e6;
    }
}

fn arm_for_smoke(game: &mut Game, grade: f32) {
    let mut rng = Rng::new(0xA2D);
    let mut source = Source::plain(grade, game.params());
    source.bias = 1.0;
    for _ in 0..60 {
        game.collect(Item::Part(upgrades::roll_part(&mut rng, &source)));
    }
    for _ in 0..3 {
        game.collect(Item::Surge(upgrades::roll_surge(
            &mut rng, &source, &game.tune,
        )));
    }
    let center = game.player().map_or(Vec2::ZERO, |ship| ship.position);
    let items = [
        Item::Repair(30.0),
        Item::Recharge(30.0),
        Item::Life,
        Item::Material(Material::Metal, 50.0),
        Item::Material(Material::Volatiles, 30.0),
        Item::Material(Material::Crystal, 20.0),
        Item::Part(upgrades::roll_part(&mut rng, &source)),
        Item::Part(upgrades::roll_part(&mut rng, &source)),
        Item::Surge(upgrades::roll_surge(&mut rng, &source, &game.tune)),
        Item::Surge(upgrades::roll_surge(&mut rng, &source, &game.tune)),
    ];
    for (i, item) in items.into_iter().enumerate() {
        let spot = center + Vec2::from_angle(i as f32 * 0.8) * 240.0;
        game.drop_item(spot, Vec2::ZERO, item);
    }
}

/// Stages the landing-pad states for a screenshot: beside the nearest planetoid with a kit
/// (`kit`), with a pad down (`deploy`), landed on it (`land`), or landed with the bench open
/// on the tab named by SSC_BENCH (`bench`). Hurts the ship a little so mending shows.
pub(crate) fn smoke_pads(game: &mut Game, mode: &str) {
    game.cargo = Cargo {
        metal: 150.0,
        volatiles: 80.0,
        crystal: 60.0,
        ..Default::default()
    };
    let Some(ship) = game.player().map(|p| p.position) else {
        return;
    };
    let Some((center, radius)) = game
        .bodies
        .iter()
        .filter(|b| b.kind == BodyKind::Asteroid && b.rock == RockKind::Planetoid)
        .min_by(|a, b| {
            ship.distance(a.position)
                .total_cmp(&ship.distance(b.position))
        })
        .map(|b| (b.position, b.radius))
    else {
        return;
    };
    let away = (ship - center).try_normalize().unwrap_or(Vec2::Y);
    game.craft_kit();
    game.teleport(center + away * (radius + 70.0));
    if mode == "kit" {
        return;
    }
    game.pad_action();
    if let Some(ship) = game.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
        ship.health = ship.max_health * 0.6;
    }
    if mode == "deploy" {
        return;
    }
    let pad_at = game.pads().next().map(|pad| game.pad_position(pad));
    if let Some(at) = pad_at {
        // Clear the neighborhood so the staged landing is not refused.
        game.bodies
            .retain(|b| b.kind != BodyKind::Creature || b.position.distance(at) > 400.0);
        game.teleport(at);
        // The first press may already have landed on the home pad: a second would lift off.
        if !game.is_landed() {
            game.pad_action();
        }
    }
    if mode == "bench" {
        game.bench_toggle();
        let tab = std::env::var("SSC_BENCH")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        game.bench_tab(tab);
    }
}

/// Existing progress staged only by the bounded smoke runner.
pub(crate) fn smoke_bench(game: &mut Game, mode: &str) {
    use ssc::genome::Genome;
    use ssc::simulation::{
        BenchAction,
        arsenal::Profile,
        organs::{Organ, Strain},
        skills::Skill,
        upgrades::{Rarity, Slot},
    };
    game.cargo = Cargo {
        metal: 80.0,
        volatiles: 55.0,
        crystal: 25.0,
        fuel: 80.0,
        biomass: 30.0,
        water: 10.0,
        ..default()
    };
    match mode {
        "parts" | "upgrade" => {
            let source = upgrades::Source::plain(2.0, game.params());
            let mut rng = Rng::new(90);
            game.loadout.parts.clear();
            for (slot, rarity) in [
                (Slot::Plating, Rarity::Rare),
                (Slot::Engine, Rarity::Common),
            ] {
                let part = (0..500)
                    .map(|_| upgrades::roll_part(&mut rng, &source))
                    .find(|part| part.slot == slot && part.rarity == rarity)
                    .expect("bounded fitted-part specimen");
                game.loadout.parts.push(part);
            }
            game.bench_select(if mode == "parts" {
                BenchAction::Upgrade(0)
            } else {
                BenchAction::Upgrade(1)
            });
        }
        "weapons" => {
            game.loadout.arsenal.acquire(Profile::Spread, 1);
            game.loadout
                .arsenal
                .acquire(Profile::Needles, Profile::Needles.max_level());
            game.bench_select(BenchAction::Weapon(Profile::Spread));
        }
        "mining-fleet-name" => {
            game.stage_drone_name_smoke();
        }
        "power" => {
            game.loadout
                .research
                .known
                .insert(ssc::simulation::research::Tech::Fabrication);
            game.bench_select(BenchAction::Power);
        }
        "mining-drone"
        | "mining-fleet"
        | "mining-fleet-pause"
        | "mining-fleet-recall"
        | "mining-fleet-repair"
        | "mining-fleet-status"
        | "mining-fleet-retrofit"
        | "mining-fleet-template"
        | "mining-fleet-role"
        | "mining-fleet-blueprint"
        | "mining-fleet-deposit"
        | "mining-fleet-rock" => {
            game.loadout.research.known.extend([
                ssc::simulation::research::Tech::Fabrication,
                ssc::simulation::research::Tech::Automation,
            ]);
            game.bench_select(BenchAction::Power);
            game.bench_confirm();
            game.cargo.metal = 100.0;
            game.cargo.crystal = 30.0;
            game.bench_select(BenchAction::Warehouse);
            game.bench_confirm();
            if matches!(mode, "mining-fleet-blueprint" | "mining-fleet-role") {
                game.stage_drone_blueprint_smoke();
            }
            game.bench_select(BenchAction::MiningDrone);
            if mode.starts_with("mining-fleet") {
                for _ in 0..game.tune.fleet_max_drones - 1 {
                    game.cargo.metal = 40.0;
                    game.cargo.crystal = 10.0;
                    game.bench_confirm();
                }
                game.cargo.metal = 40.0;
                game.cargo.crystal = 10.0;
                if matches!(
                    mode,
                    "mining-fleet-pause"
                        | "mining-fleet-recall"
                        | "mining-fleet-repair"
                        | "mining-fleet-status"
                        | "mining-fleet-retrofit"
                        | "mining-fleet-template"
                        | "mining-fleet-role"
                        | "mining-fleet-blueprint"
                        | "mining-fleet-deposit"
                        | "mining-fleet-rock"
                ) {
                    game.bench_confirm();
                    game.bench_select(BenchAction::MiningDroneStatus(
                        game.tune.fleet_max_drones - 1,
                    ));
                    if matches!(mode, "mining-fleet-retrofit" | "mining-fleet-template") {
                        game.cargo.fuel = 4.0;
                        game.bench_select(BenchAction::Stash(Material::Fuel));
                        game.bench_confirm();
                        game.step(1.0 / 60.0, Input::default());
                        let template = mode == "mining-fleet-template";
                        game.cargo.metal = if template { 80.0 } else { 20.0 };
                        game.cargo.crystal = if template { 20.0 } else { 5.0 };
                        game.bench_select(BenchAction::DroneUpgrade(
                            game.tune.fleet_max_drones - 1,
                            ssc::simulation::fleet::DroneUpgrade::Cargo,
                        ));
                    }
                    game.bench_feedback = None;
                    if mode == "mining-fleet-repair" {
                        game.stage_drone_repair_smoke();
                    }
                    if mode == "mining-fleet-recall" {
                        game.cargo.fuel = 20.0;
                        game.bench_select(BenchAction::Stash(Material::Fuel));
                        game.bench_confirm();
                        game.step(1.0, Input::default());
                        game.bench_select(BenchAction::RecallDroneFleet);
                    }
                    if mode == "mining-fleet-pause" {
                        game.bench_select(BenchAction::PauseDroneFleet);
                    }
                    if matches!(mode, "mining-fleet-deposit" | "mining-fleet-rock")
                        && let Some(action) = if mode == "mining-fleet-rock" {
                            game.stage_drone_rock_smoke()
                        } else {
                            game.stage_drone_deposit_smoke()
                        }
                    {
                        game.bench_select(action);
                    }
                    if matches!(mode, "mining-fleet-blueprint" | "mining-fleet-role") {
                        game.cargo.metal = 80.0;
                        game.cargo.crystal = 20.0;
                        game.bench_select(if mode == "mining-fleet-role" {
                            BenchAction::CycleDroneRole
                        } else {
                            BenchAction::ApplyDroneBlueprint
                        });
                    }
                }
            }
        }
        "water-extractor" => {
            game.loadout
                .research
                .known
                .insert(ssc::simulation::research::Tech::Fabrication);
            game.bench_select(BenchAction::WaterTank);
            game.bench_confirm();
            game.bench_select(BenchAction::WaterExtractor);
        }
        "raw-input" => {
            game.cargo.volatiles = 0.0;
            game.bench_select(BenchAction::RawInput);
        }
        "warehouse" => game.bench_select(BenchAction::Warehouse),
        "water-tank" => game.bench_select(BenchAction::WaterTank),
        "refinery" => {
            game.loadout
                .research
                .known
                .insert(ssc::simulation::research::Tech::Fabrication);
            game.bench_select(BenchAction::Refinery);
        }
        "research" => game.bench_select(BenchAction::Research(
            ssc::simulation::research::Tech::Frontier,
        )),
        "skills" => game.bench_select(BenchAction::Skill(Skill::EchoLodes)),
        "gate" => game.bench_select(BenchAction::Skill(Skill::Parry)),
        "organs" => {
            game.loadout.skills.raise(Skill::Symbiosis);
            for (organ, donor) in [
                (Organ::Remora, Genome::remora()),
                (Organ::Faraday, Genome::stormcap()),
                (Organ::Veil, Genome::veilwing()),
                (Organ::Skipjack, Genome::skipjack()),
            ] {
                game.loadout
                    .organs
                    .acquire(Strain::from_donor(organ, &donor), &game.tune);
            }
            game.bench_organ(Organ::Remora).unwrap();
            game.bench_select(BenchAction::Organ(Organ::Skipjack));
        }
        "stash" => game.bench_select(BenchAction::Stash(Material::Crystal)),
        "reforge-good" | "reforge-kept" | "unlock" => {
            let slot = if mode == "unlock" {
                Slot::Plating
            } else {
                Slot::Engine
            };
            let rarity = match mode {
                "reforge-good" => Rarity::Rare,
                "unlock" => Rarity::Uncommon,
                _ => Rarity::Rare,
            };
            game.loadout.parts.push(upgrades::Part {
                name: "Test drive".into(),
                stem: "Drive".into(),
                slot,
                rarity,
                grade: 1.0,
                effects: vec![upgrades::Effect::Stat(upgrades::Stat::Thrust, 0.2)],
                core: 1,
            });
            if mode == "reforge-kept" {
                game.loadout
                    .parts
                    .last_mut()
                    .unwrap()
                    .effects
                    .push(upgrades::Effect::Stat(upgrades::Stat::Hull, 5.0));
            }
            game.bench_select(if mode == "unlock" {
                BenchAction::Upgrade(0)
            } else {
                BenchAction::Reforge(0)
            });
        }
        "repeated" => game.bench_select(BenchAction::Skill(Skill::BeamPower)),
        _ => panic!("unknown bench gallery"),
    }
}
