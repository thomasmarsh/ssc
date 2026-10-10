mod audio;
mod autosave;
mod bestiaryview;
mod chartview;
mod flockview;
mod glitchview;
mod grammarview;
mod hud;
mod juice;
mod nebula;
mod powerview;
mod presentation;
mod settings;
mod shipview;
mod smoke;
mod titlemenu;
mod ui;
mod wellview;

use bevy::{
    app::AppExit,
    camera::ScalingMode,
    gizmos::config::{DefaultGizmoConfigGroup, GizmoConfigStore},
    post_process::bloom::Bloom,
    prelude::*,
    render::{
        RenderPlugin,
        settings::{Backends, PowerPreference, RenderCreation, WgpuSettings},
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};
use smoke::{SmokeRun, smoke_run};
use ssc::simulation::upgrades::{self, Item, Source};
use ssc::simulation::{BodyKind, Cargo, Game, Input, Material, PinLabel};
use ssc::world::RockKind;
use ssc::world::{Rng, SECTOR_SIZE, SectorId};

/// Left stick deadzone for thrust; the right stick aims and fires past a larger push.
const STICK_DEADZONE: f32 = 0.15;
const FIRE_STICK_THRESHOLD: f32 = 0.3;
/// Number keys that pick the 1st to 9th owned weapon profile.
const DIGITS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Camera preferences belong to the desktop adapter, independent of simulation rules.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum CameraView {
    #[default]
    Close,
    Wide,
    Far,
    Sector,
}

impl CameraView {
    fn next(self) -> Self {
        match self {
            Self::Close => Self::Wide,
            Self::Wide => Self::Far,
            Self::Far => Self::Sector,
            Self::Sector => Self::Close,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Close => "CLOSE",
            Self::Wide => "WIDE",
            Self::Far => "FAR",
            Self::Sector => "SECTOR",
        }
    }

    fn scaling_mode(self) -> ScalingMode {
        match self {
            Self::Close | Self::Wide | Self::Far => ScalingMode::FixedVertical {
                viewport_height: presentation::VIEW_HEIGHT
                    * match self {
                        Self::Wide => 1.2,
                        Self::Far => 4.0,
                        _ => 1.0,
                    },
            },
            // Fit the entire square even in portrait windows, with a border margin.
            Self::Sector => ScalingMode::AutoMin {
                min_width: SECTOR_SIZE * 1.1,
                min_height: SECTOR_SIZE * 1.1,
            },
        }
    }
}

/// How the scene is rendered. Gameplay never depends on it, so new looks can be added
/// here (and in `apply_style`) without touching the simulation.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum RenderStyle {
    /// Thin vector lines on a flat background.
    #[default]
    Classic,
    /// HDR with soft bloom and heavier lines: a phosphor-lit vector display.
    Glow,
    /// Wide, hot bloom with a long anamorphic streak: neon gas tubes.
    Neon,
}

impl RenderStyle {
    fn next(self) -> Self {
        match self {
            Self::Classic => Self::Glow,
            Self::Glow => Self::Neon,
            Self::Neon => Self::Classic,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "CLASSIC",
            Self::Glow => "GLOW",
            Self::Neon => "NEON",
        }
    }

    fn line_width(self) -> f32 {
        match self {
            Self::Classic => 2.0,
            Self::Glow => 2.5,
            Self::Neon => 3.5,
        }
    }

    fn bloom(self) -> Option<Bloom> {
        match self {
            Self::Classic => None,
            Self::Glow => Some(Bloom {
                intensity: 0.45,
                ..Bloom::NATURAL
            }),
            Self::Neon => Some(Bloom {
                intensity: 0.7,
                ..Bloom::ANAMORPHIC
            }),
        }
    }
}

/// Where the star map's cursor is and which preset note a new pin would carry.
#[derive(Clone, Copy, Debug)]
pub struct ChartCursor {
    pub sector: SectorId,
    pub label: PinLabel,
    pub center: SectorId,
    pub zoom: usize,
}

#[derive(Resource)]
pub struct Session {
    pub game: Game,
    pub input: Input,
    pub paused: bool,
    pub slow: bool,
    /// The radar stays on (otherwise it shows while the details are open).
    pub radar: bool,
    /// The details panel is latched open (F3); holding Tab opens it too.
    pub details: bool,
    pub tab_held: bool,
    /// The full key list (F1).
    pub help: bool,
    /// Edge arrows toward offscreen threats and minerals.
    pub arrows: bool,
    pub camera_view: CameraView,
    pub style: RenderStyle,
    /// Hides the nebula backdrop and plain-colours the stars (key U, or SSC_REDUCE_EFFECTS=1).
    pub reduce_effects: bool,
    /// The settings screen (Esc), with the selected row, and the options it holds that the
    /// game itself keeps: auto repair and the boosts.
    pub settings: Option<usize>,
    pub save_feedback: String,
    /// The developer console (SSC_DEV=1); the game waits while it is open. See `ui`.
    pub console: Option<ui::screens::console::Console>,
    /// The console took this frame's input (it was open, or opened or closed this frame), so
    /// `controls` skips it.
    pub ui_consumed: bool,
    /// Screen shake, floating scores and rings; see `juice`.
    pub juice: juice::Juice,
    pub auto_repair: bool,
    pub boosts: bool,
    /// The star map, while it is open (the simulation waits), with its cursor and note preset.
    pub chart: Option<ChartCursor>,
    /// Best score this session (kept in memory only), whether the run just ended beat it,
    /// and whether the ended run has been entered yet.
    pub best: Option<u64>,
    pub new_best: bool,
    recorded: bool,
    /// Ship losses already saved, so each loss is written once.
    settled_deaths: u32,
    /// The title menu, up at every normal launch (the game waits).
    pub menu: Option<titlemenu::TitleMenu>,
}

impl Session {
    /// Whether the details panel (and the radar with it) is showing.
    pub fn details_open(&self) -> bool {
        self.details || self.tab_held
    }
}

/// `SSC_TUNING=<path>`, honoured only with `SSC_DEV=1` (like the other developer hooks): a RON
/// map of tunable name to number, for example `{ "adapt_max": 0.4 }` (see docs/DEVTOOLS.md).
fn dev_tuning_file() -> Option<String> {
    if !ssc::simulation::dev::enabled() {
        return None;
    }
    let path = std::env::var_os("SSC_TUNING")?;
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) => {
            eprintln!("tuning: cannot read {}: {e}", path.to_string_lossy());
            None
        }
    }
}

/// Prints what an overrides file did: what applied, and each entry that was refused.
fn report_tuning(report: &ssc::simulation::tunables::OverrideReport) {
    eprintln!(
        "tuning: applied {} override(s), {} problem(s)",
        report.applied.len(),
        report.problems.len()
    );
    for problem in &report.problems {
        eprintln!("tuning: {problem}");
    }
}

impl Default for Session {
    fn default() -> Self {
        let saved = autosave::load();
        // A scripted run continues straight into its save (SSC_MENU shows the menu).
        let scripted = std::env::var_os("SSC_SMOKE_FRAMES").is_some();
        let menu = (!scripted).then(|| {
            let summary = saved
                .as_ref()
                .map(|game| {
                    let minutes = (game.time / 60.0) as u32;
                    format!(
                        "saved game: score {}, {} lives, {minutes} min",
                        game.score, game.lives
                    )
                })
                .unwrap_or_default();
            titlemenu::TitleMenu::new(saved.is_some(), summary)
        });
        let overrides = dev_tuning_file();
        let game = match (saved, overrides) {
            // A continued save keeps its own tuning; the file is applied on top.
            (Some(mut game), Some(text)) => {
                report_tuning(&game.tune_load_overrides(&text));
                game
            }
            (Some(game), None) => game,
            // A fresh world is generated under the file, so generation entries apply.
            (None, Some(text)) => {
                let (tune, report) = ssc::simulation::Tunables::from_overrides(&text);
                report_tuning(&report);
                Game::with_tuning(ssc::config::MASTER_SEED, tune)
            }
            (None, None) => Game::new(ssc::config::MASTER_SEED),
        };
        Self {
            settled_deaths: game.run.deaths,
            game,
            menu,
            input: Input::default(),
            paused: false,
            slow: false,
            radar: false,
            details: false,
            tab_held: false,
            help: false,
            arrows: true,
            camera_view: CameraView::default(),
            style: RenderStyle::default(),
            reduce_effects: std::env::var_os("SSC_REDUCE_EFFECTS").is_some(),
            settings: None,
            save_feedback: String::new(),
            console: None,
            ui_consumed: false,
            juice: juice::Juice::default(),
            auto_repair: true,
            boosts: true,
            chart: None,
            best: None,
            new_best: false,
            recorded: false,
        }
    }
}

/// SDL mapping for the Switch 2 bridge's virtual gamepad (see switch2mac VirtualHID.swift for the
/// report layout). gilrs has no built-in entry for it, so without this the sticks stay unmapped.
/// Axes sort by usage (X, Y, Z, Rx, Ry, Rz) and buttons by usage, as in the report.
const SWITCH2_BRIDGE_MAPPING: &str = "030000007e0500006920000000000000,Pro Controller 2 (Finally),\
platform:Mac OS X,a:b0,b:b1,x:b2,y:b3,back:b4,guide:b5,start:b6,leftstick:b7,rightstick:b8,\
leftshoulder:b9,rightshoulder:b10,misc1:b15,leftx:a0,lefty:a1,rightx:a2,righty:a5,\
lefttrigger:a3,righttrigger:a4,dpup:h0.1,dpright:h0.2,dpdown:h0.4,dpleft:h0.8,";

fn main() {
    if std::env::var_os("SDL_GAMECONTROLLERCONFIG").is_none() {
        // SAFETY: set before any threads are spawned.
        unsafe { std::env::set_var("SDL_GAMECONTROLLERCONFIG", SWITCH2_BRIDGE_MAPPING) };
    }
    let mut settings = WgpuSettings {
        power_preference: PowerPreference::LowPower,
        ..default()
    };
    if cfg!(target_os = "macos") {
        settings.backends = Some(Backends::METAL);
    }
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.012, 0.022, 0.045)))
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .init_resource::<Session>()
        .init_resource::<SmokeRun>()
        .insert_resource(grammarview::Gallery::from_env())
        .insert_resource(bestiaryview::Bestiary::from_env())
        .init_resource::<audio::Audio>()
        .add_plugins(ui::UiPlugin)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "SSC / deep space".into(),
                        resolution: (1200, 800).into(),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(Box::new(settings)),
                    ..default()
                }),
        )
        .add_systems(
            Startup,
            (
                presentation::setup,
                hud::setup,
                nebula::setup,
                chartview::setup,
                bestiaryview::setup,
                audio::setup,
            ),
        )
        .add_systems(FixedUpdate, simulate)
        .add_systems(Last, autosave::autosave)
        .add_systems(
            Update,
            (
                controls,
                hud::apply_ui_scale,
                camera,
                apply_style,
                nebula::update,
                juice::update,
                audio::apply_mute,
                audio::play_cues,
                presentation::draw
                    .run_if(not(grammarview::gallery_active))
                    .run_if(not(bestiaryview::gallery_active)),
                grammarview::draw,
                bestiaryview::draw,
                presentation::update_hud.run_if(not(bestiaryview::gallery_active)),
                presentation::scroll_panels,
                hud::update_texts.run_if(not(bestiaryview::gallery_active)),
                (
                    chartview::update,
                    chartview::update_geometry,
                    presentation::update_chart,
                )
                    .chain(),
                smoke_run,
            )
                .chain(),
        )
        .run();
}

fn simulate(time: Res<Time<Fixed>>, mut session: ResMut<Session>, smoke: Res<SmokeRun>) {
    if smoke.hold
        || session.paused
        || session.chart.is_some()
        || session.settings.is_some()
        || session.console.is_some()
        || session.menu.is_some()
    {
        return;
    }
    let input = session.input;
    let dt = time.delta_secs() * if session.slow { 0.35 } else { 1.0 };
    session.game.step_scaled(dt, input);
    // Save the recovered ship immediately after a lost life.
    if session.game.run.deaths != session.settled_deaths {
        session.settled_deaths = session.game.run.deaths;
        autosave::settle(&session.game);
    }
    if session.game.game_over && !session.recorded {
        session.recorded = true;
        let score = session.game.score;
        session.new_best = session.best.is_none_or(|best| score > best);
        session.best = session.best.max(Some(score));
    }
}

/// Every physical input device the player can use.
#[derive(bevy::ecs::system::SystemParam)]
struct Devices<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
}

#[allow(clippy::too_many_arguments)]
fn controls(
    devices: Devices,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    view: Single<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut session: ResMut<Session>,
    mut stick_latch: Local<bool>,
) {
    let Devices {
        keys,
        mouse,
        gamepads,
    } = devices;
    let pad = |button| gamepads.iter().any(|pad| pad.just_pressed(button));
    // Settings that live in the game are pushed in every frame, so a restart keeps them.
    let (auto_repair, boosts) = (session.auto_repair, session.boosts);
    session.game.set_auto_repair(auto_repair);
    session.game.set_boosts(boosts);
    // Esc (Start on a pad) opens the settings, which pause the game; Esc closes the key list
    // first if that is what is showing.
    let escape = keys.just_pressed(KeyCode::Escape);
    let start = pad(GamepadButton::Start);
    // The developer console (SSC_DEV=1, `ui`) takes every key while it is open, and the frame
    // it opens or closes on.
    if std::mem::take(&mut session.ui_consumed) {
        session.input = Input::default();
        return;
    }
    // The title menu and the settings screen (`ui::screens::{title, settings}`) take the same.
    if session.menu.is_some() || session.settings.is_some() {
        session.input = Input::default();
        return;
    }
    if session.help && (escape || keys.just_pressed(KeyCode::F1)) {
        session.help = false;
    } else if escape || start {
        session.settings = Some(0);
        session.input = Input::default();
        return;
    }
    if keys.just_pressed(KeyCode::KeyP) || keys.just_pressed(KeyCode::Pause) {
        session.paused = !session.paused;
    }
    // Tab held shows the details (and the radar); F3 latches them; F1 is the full key list.
    session.tab_held = keys.pressed(KeyCode::Tab);
    if keys.just_pressed(KeyCode::F3) {
        session.details = !session.details;
    }
    if keys.just_pressed(KeyCode::F1) {
        session.help = !session.help;
    }
    if keys.just_pressed(KeyCode::F11) {
        window.mode = if window.mode == WindowMode::Windowed {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        };
    }
    // The star map: G (d-pad left), while flying. It pauses the simulation; see `chart_controls`.
    if !session.game.game_over
        && !session.game.bench_open()
        && (keys.just_pressed(KeyCode::KeyG) || pad(GamepadButton::DPadLeft))
    {
        session.chart = match session.chart {
            Some(_) => None,
            None => Some(ChartCursor {
                sector: session.game.sector(),
                label: PinLabel::Danger,
                center: session.game.sector(),
                zoom: 3,
            }),
        };
    }
    if session.chart.is_some() {
        chart_controls(&keys, &gamepads, &mut session, &mut stick_latch);
        session.input = Input::default();
        return;
    }
    // The bench, while landed with it open: arrows pick a row (left and right a tab), enter or
    // space does the thing, Q takes from the stash, 1-3 jump to a tab, E closes. Nothing flies.
    if !session.paused && session.game.bench_open() {
        bench_controls(&keys, &pad, &mut session);
        session.input = Input::default();
        return;
    }
    // Weapon profiles: ] next, [ previous, 1-9 pick directly; the shoulders on a pad.
    // Switching is instant and ignored while paused.
    if !session.paused {
        let next = keys.just_pressed(KeyCode::BracketRight) || pad(GamepadButton::RightTrigger);
        let previous = keys.just_pressed(KeyCode::BracketLeft) || pad(GamepadButton::LeftTrigger);
        if next {
            session.game.switch_weapon(1);
        }
        if previous {
            session.game.switch_weapon(-1);
        }
        for (n, key) in DIGITS.into_iter().enumerate() {
            if keys.just_pressed(key) {
                session.game.select_weapon(n);
            }
        }
        // Parry (a later upgrade, refused while locked): D, or d-pad right on a pad.
        if keys.just_pressed(KeyCode::KeyD) || pad(GamepadButton::DPadRight) {
            session.game.parry();
        }
        // Sonar ping: X (right stick click on a pad).
        if keys.just_pressed(KeyCode::KeyX) || pad(GamepadButton::RightThumb) {
            session.game.ping();
        }
        // The one context key: E (B or Select on a pad) lands, builds and deploys a pad, opens
        // the bench, or tithes, whichever the prompt over the ship says.
        if keys.just_pressed(KeyCode::KeyE)
            || pad(GamepadButton::East)
            || pad(GamepadButton::Select)
        {
            session.game.interact();
        }
        // Seed picker: C (d-pad up) cycles the species planted next.
        if keys.just_pressed(KeyCode::KeyC) || pad(GamepadButton::DPadUp) {
            session.game.cycle_seed();
        }
        // Beacon (locked until bought at the bench's SKILLS tab): H, or Y on a pad.
        if keys.just_pressed(KeyCode::KeyH) || pad(GamepadButton::North) {
            let _ = session.game.deploy_beacon();
        }
    }
    // Enter or the south face button (Xbox A) starts a new run once the last ship is lost.
    // Restarting mid-run is in the settings.
    if session.game.game_over && (keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South)) {
        restart(&mut session);
        session.input = Input::default();
        return;
    }
    // Cursor steering is active while firing; keyboard-only play keeps its heading.
    let aim_direction = if mouse.pressed(MouseButton::Left) {
        window.cursor_position().and_then(|cursor| {
            view.0
                .viewport_to_world_2d(view.1, cursor)
                .ok()
                .and_then(|position| session.game.player().map(|ship| position - ship.position))
        })
    } else {
        None
    };
    // Twin-stick: left stick thrusts in any direction, right stick aims and fires past a larger push.
    let mut stick_move = None;
    let mut stick_aim = None;
    let mut pad_brake = false;
    let mut pad_mine = false;
    for pad in &gamepads {
        let left = pad.left_stick();
        if left.length() > STICK_DEADZONE {
            stick_move = Some(left);
        }
        let right = pad.right_stick();
        if right.length() > FIRE_STICK_THRESHOLD {
            stick_aim = Some(right);
        }
        pad_brake |= pad.pressed(GamepadButton::LeftTrigger2) || pad.pressed(GamepadButton::South);
        // The right trigger (R2) holds the mining beam (no aiming needed; the guns go quiet).
        // The bumpers are the weapon switch.
        pad_mine |= pad.pressed(GamepadButton::RightTrigger2);
    }
    let pad_fire = stick_aim.is_some();
    // Dash (a later upgrade, refused while locked): Shift or L3, toward the left stick, else
    // where the ship faces.
    let dash_pressed = !session.paused
        && (keys.just_pressed(KeyCode::ShiftLeft)
            || keys.just_pressed(KeyCode::ShiftRight)
            || pad(GamepadButton::LeftThumb));
    if dash_pressed {
        session.game.dash(stick_move);
    }
    session.input = Input {
        thrust: if keys.pressed(KeyCode::ArrowUp) {
            1.0
        } else {
            0.0
        },
        turn: keys.pressed(KeyCode::ArrowLeft) as u8 as f32
            - keys.pressed(KeyCode::ArrowRight) as u8 as f32,
        brake: keys.pressed(KeyCode::ArrowDown) || pad_brake,
        fire: keys.pressed(KeyCode::KeyA)
            || keys.pressed(KeyCode::Space)
            || mouse.pressed(MouseButton::Left)
            || pad_fire,
        mine: keys.pressed(KeyCode::KeyM) || pad_mine,
        aim_direction: stick_aim.or(aim_direction),
        move_direction: stick_move,
    };
    // Bounded visual checks of independent aim, turning RCS, braking and coasting.
    if std::env::var_os("SSC_SMOKE_FRAMES").is_some()
        && let Ok(mode) = std::env::var("SSC_SHIP_VIEW")
    {
        let time = session.game.time;
        session.input = Input {
            aim_direction: Some(Vec2::X),
            move_direction: match mode.as_str() {
                "cross" => Some(Vec2::Y * 0.8),
                "reverse" => Some(-Vec2::X),
                "turn" => Some(Vec2::from_angle((time * 2.0).floor() * 1.5)),
                "brake" | "coast" if time >= 1.0 => None,
                "brake" | "coast" => Some(Vec2::Y),
                _ => Some(Vec2::X),
            },
            brake: mode == "brake" && time >= 1.0,
            ..default()
        };
    }
}

/// Starts a fresh game.
fn restart(session: &mut Session) {
    // A developer's tuning overrides carry into the new game (and shape its generation).
    session.game = Game::with_tuning(ssc::config::MASTER_SEED, session.game.tune);
    session.settled_deaths = 0;
    session.recorded = false;
    session.new_best = false;
    session.paused = false;
    session.slow = false;
    session.chart = None;
    session.console = None;
    session.save_feedback.clear();
}

/// The bench's keys; see `controls`.
fn bench_controls(
    keys: &ButtonInput<KeyCode>,
    pad: &impl Fn(GamepadButton) -> bool,
    session: &mut Session,
) {
    let game = &mut session.game;
    if game.drone_name_editing() {
        let cursor =
            i32::from(keys.just_pressed(KeyCode::ArrowRight) || pad(GamepadButton::DPadRight))
                - i32::from(keys.just_pressed(KeyCode::ArrowLeft) || pad(GamepadButton::DPadLeft));
        let character =
            i32::from(keys.just_pressed(KeyCode::ArrowUp) || pad(GamepadButton::DPadUp))
                - i32::from(keys.just_pressed(KeyCode::ArrowDown) || pad(GamepadButton::DPadDown));
        game.drone_name_step(cursor, character);
        if keys.just_pressed(KeyCode::KeyE)
            || pad(GamepadButton::East)
            || pad(GamepadButton::Select)
        {
            game.finish_drone_name(false);
        } else if keys.just_pressed(KeyCode::Enter)
            || keys.just_pressed(KeyCode::Space)
            || pad(GamepadButton::South)
        {
            game.finish_drone_name(true);
        } else if keys.just_pressed(KeyCode::KeyQ) || pad(GamepadButton::West) {
            game.drone_name_clear();
        }
        return;
    }
    if keys.just_pressed(KeyCode::ArrowDown)
        || pad(GamepadButton::DPadDown)
        || pad(GamepadButton::RightTrigger)
    {
        game.bench_move(1);
    }
    if keys.just_pressed(KeyCode::ArrowUp)
        || pad(GamepadButton::DPadUp)
        || pad(GamepadButton::LeftTrigger)
    {
        game.bench_move(-1);
    }
    if keys.just_pressed(KeyCode::ArrowRight) || pad(GamepadButton::DPadRight) {
        game.bench_tab_step(1);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) || pad(GamepadButton::DPadLeft) {
        game.bench_tab_step(-1);
    }
    for (n, key) in DIGITS.into_iter().take(3).enumerate() {
        if keys.just_pressed(key) {
            game.bench_tab(n);
        }
    }
    if keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || pad(GamepadButton::South)
    {
        game.bench_confirm();
    } else if keys.just_pressed(KeyCode::KeyQ) || pad(GamepadButton::West) {
        game.bench_alt();
    }
    if keys.just_pressed(KeyCode::KeyE) || pad(GamepadButton::East) || pad(GamepadButton::Select) {
        game.interact();
    }
}

/// The star map's keys. Arrows (or the left stick, d-pad up, down and right) move the cursor,
/// [ ] (triggers) pick the note preset, F (A) pins the cursor's sector with it, Backspace (X)
/// removes the pin, H (Y) deploys a beacon at the ship, J (B) starts a jump to the beacon in
/// the cursor's sector, R recalls it, Z returns the cursor to the ship. G (d-pad left) closes.
fn chart_controls(
    keys: &ButtonInput<KeyCode>,
    gamepads: &Query<&Gamepad>,
    session: &mut Session,
    stick_latch: &mut bool,
) {
    let Some(mut cursor) = session.chart else {
        return;
    };
    let pad = |button| gamepads.iter().any(|pad| pad.just_pressed(button));
    let (mut dx, mut dy) = (0, 0);
    if keys.just_pressed(KeyCode::ArrowLeft) {
        dx -= 1;
    }
    if keys.just_pressed(KeyCode::ArrowRight) || pad(GamepadButton::DPadRight) {
        dx += 1;
    }
    if keys.just_pressed(KeyCode::ArrowUp) || pad(GamepadButton::DPadUp) {
        dy += 1;
    }
    if keys.just_pressed(KeyCode::ArrowDown) || pad(GamepadButton::DPadDown) {
        dy -= 1;
    }
    let stick = gamepads
        .iter()
        .map(|pad| pad.left_stick())
        .find(|s| s.length() > 0.6);
    match (stick, *stick_latch) {
        (Some(s), false) => {
            *stick_latch = true;
            if s.x.abs() >= s.y.abs() {
                dx += s.x.signum() as i32;
            } else {
                dy += s.y.signum() as i32;
            }
        }
        (None, true) => *stick_latch = false,
        _ => {}
    }
    cursor.sector.x += dx;
    cursor.sector.y += dy;
    if dx != 0 || dy != 0 {
        cursor.center = cursor.sector;
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        cursor.sector = session.game.sector();
        cursor.center = cursor.sector;
    }
    if keys.just_pressed(KeyCode::BracketRight) || pad(GamepadButton::RightTrigger) {
        cursor.label = cursor.label.step(1);
    }
    if keys.just_pressed(KeyCode::BracketLeft) || pad(GamepadButton::LeftTrigger) {
        cursor.label = cursor.label.step(-1);
    }
    if keys.just_pressed(KeyCode::KeyF) || pad(GamepadButton::South) {
        session.game.chart_pin(cursor.sector, cursor.label);
    }
    if keys.just_pressed(KeyCode::Backspace)
        || keys.just_pressed(KeyCode::Delete)
        || pad(GamepadButton::West)
    {
        session.game.chart_unpin(cursor.sector);
    }
    if keys.just_pressed(KeyCode::KeyH) || pad(GamepadButton::North) {
        let _ = session.game.deploy_beacon();
    }
    if keys.just_pressed(KeyCode::KeyR)
        && let Some(id) = session.game.beacon_in(cursor.sector)
    {
        session.game.recall_beacon(id);
    }
    let jump = keys.just_pressed(KeyCode::KeyJ) || pad(GamepadButton::East);
    session.chart = Some(cursor);
    if jump {
        match session.game.beacon_in(cursor.sector) {
            Some(id) => {
                // A started charge-up needs the world to run: leave the map.
                if session.game.begin_travel(id).is_ok() {
                    session.chart = None;
                }
            }
            None => session.game.chart_note("NO BEACON IN THIS SECTOR"),
        }
    }
}

/// The camera trails the ship with a little look-ahead so fast flight shows what is coming,
/// and shakes with the trauma budget (never when effects are reduced, never by rotating).
fn camera(
    session: Res<Session>,
    time: Res<Time>,
    mut view: Single<(&Camera, &mut Transform, &mut Projection), With<Camera2d>>,
    mut previous_view: Local<CameraView>,
    mut base: Local<Option<Vec2>>,
) {
    let game = &session.game;
    let target = if session.camera_view == CameraView::Sector {
        game.sector().center()
    } else {
        game.player()
            .map_or(game.focus, |ship| ship.position + ship.velocity * 0.3)
    };
    // Bounded smoke runs render far faster than real time: snap instead of trailing.
    let smoothing = if std::env::var_os("SSC_SMOKE_FRAMES").is_some() {
        1.0
    } else {
        1.0 - (-6.0 * time.delta_secs()).exp()
    };
    let (camera, transform, projection) = &mut *view;
    let current = base.unwrap_or(transform.translation.truncate());
    // Snap sector framing and the return to close view, keeping the ship visible.
    let next = if session.camera_view == CameraView::Sector || *previous_view == CameraView::Sector
    {
        target
    } else {
        current.lerp(target, smoothing)
    };
    *base = Some(next);
    let mut world_per_pixel = 1.0;
    if let Projection::Orthographic(projection) = &mut **projection {
        projection.scaling_mode = session.camera_view.scaling_mode();
        if let Some(size) = camera.logical_viewport_size() {
            world_per_pixel = projection.area.height() / size.y.max(1.0);
        }
    }
    let shake = if session.reduce_effects {
        Vec2::ZERO
    } else {
        session.juice.trauma.offset(session.juice.clock) * world_per_pixel
    };
    transform.translation = (next + shake).extend(0.0);
    *previous_view = session.camera_view;
}

/// Reconfigures the camera and gizmos when the render style changes.
fn apply_style(
    session: Res<Session>,
    mut commands: Commands,
    camera: Single<Entity, With<Camera2d>>,
    mut gizmos: ResMut<GizmoConfigStore>,
    mut applied: Local<Option<RenderStyle>>,
) {
    if *applied == Some(session.style) {
        return;
    }
    *applied = Some(session.style);
    gizmos.config_mut::<DefaultGizmoConfigGroup>().0.line.width = session.style.line_width();
    // Hdr and tonemapping stay on the camera for good: toggling them at runtime breaks the
    // 2D render graph (InvalidViewQuery). Only the bloom pass comes and goes.
    let mut camera = commands.entity(*camera);
    match session.style.bloom() {
        Some(bloom) => {
            camera.insert(bloom);
        }
        None => {
            camera.remove::<Bloom>();
        }
    }
}

#[cfg(test)]
mod bench_input_tests {
    use super::*;
    use ssc::simulation::{BenchAction, skills::Skill};
    fn session() -> Session {
        let mut session = Session::default();
        smoke::smoke_pads(&mut session.game, "bench");
        session.game.cargo = Cargo {
            metal: 1000.0,
            volatiles: 1000.0,
            crystal: 1000.0,
            ..default()
        };
        session
            .game
            .bench_select(BenchAction::Skill(Skill::BeamPower));
        session
    }
    #[test]
    fn simultaneous_keyboard_and_controller_confirms_buy_once_and_holding_does_not_repeat() {
        let mut session = session();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Enter);
        keys.press(KeyCode::Space);
        bench_controls(
            &keys,
            &|button| button == GamepadButton::South || button == GamepadButton::West,
            &mut session,
        );
        assert_eq!(session.game.loadout.skills.level(Skill::BeamPower), 1);
        let receipt = session.game.bench_feedback.as_ref().unwrap().text.clone();
        let cargo = session.game.cargo;
        let cues = session.game.cues.len();
        keys.clear();
        for _ in 0..10 {
            bench_controls(&keys, &|_| false, &mut session);
        }
        assert_eq!(session.game.loadout.skills.level(Skill::BeamPower), 1);
        assert_eq!(session.game.bench_feedback.as_ref().unwrap().text, receipt);
        assert_eq!(session.game.cargo, cargo);
        assert_eq!(session.game.cues.len(), cues);
        keys.release(KeyCode::Enter);
        keys.clear();
        keys.press(KeyCode::Enter);
        bench_controls(&keys, &|_| false, &mut session);
        assert_eq!(session.game.loadout.skills.level(Skill::BeamPower), 2);
        assert!(
            session
                .game
                .bench_feedback
                .as_ref()
                .unwrap()
                .text
                .contains("LEVEL 1 -> 2")
        );
    }
    #[test]
    fn keyboard_and_controller_reach_all_tabs_and_groups_with_existing_bindings() {
        let mut session = session();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Digit1);
        bench_controls(&keys, &|_| false, &mut session);
        assert_eq!(
            session.game.bench_panel().unwrap().tab,
            ssc::simulation::BenchTab::Parts
        );
        keys.clear();
        for expected in [
            ssc::simulation::BenchTab::Weapons,
            ssc::simulation::BenchTab::Skills,
            ssc::simulation::BenchTab::Parts,
        ] {
            bench_controls(&keys, &|b| b == GamepadButton::DPadRight, &mut session);
            assert_eq!(session.game.bench_panel().unwrap().tab, expected);
        }
        keys.press(KeyCode::Digit3);
        bench_controls(&keys, &|_| false, &mut session);
        keys.clear();
        let mut groups = vec![];
        for _ in 0..session.game.bench_panel().unwrap().rows.len() {
            let panel = session.game.bench_panel().unwrap();
            let row = panel.rows.iter().find(|r| r.selected).unwrap();
            if groups.last() != Some(&row.group) {
                groups.push(row.group);
            }
            bench_controls(&keys, &|b| b == GamepadButton::RightTrigger, &mut session);
        }
        assert_eq!(
            groups,
            ["MINING", "FLIGHT / UTILITY", "SONAR", "ORGANS", "RESEARCH"]
        );
        assert_eq!(
            session
                .game
                .bench_panel()
                .unwrap()
                .rows
                .iter()
                .position(|r| r.selected)
                .unwrap(),
            0
        );
        bench_controls(&keys, &|b| b == GamepadButton::LeftTrigger, &mut session);
        assert_eq!(
            session
                .game
                .bench_panel()
                .unwrap()
                .rows
                .iter()
                .position(|r| r.selected)
                .unwrap(),
            session.game.bench_panel().unwrap().rows.len() - 1
        );
    }
    #[test]
    fn confirm_and_stash_take_together_do_one_transfer() {
        let mut session = session();
        session
            .game
            .bench_select(BenchAction::Stash(Material::Metal));
        session.game.cargo.metal = 100.0;
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Enter);
        keys.press(KeyCode::KeyQ);
        bench_controls(&keys, &|_| false, &mut session);
        assert_eq!(session.game.cargo.metal, 75.0);
        keys.clear();
        bench_controls(&keys, &|_| false, &mut session);
        assert_eq!(session.game.cargo.metal, 75.0);
        keys.release(KeyCode::KeyQ);
        keys.clear();
        keys.press(KeyCode::KeyQ);
        bench_controls(&keys, &|_| false, &mut session);
        assert_eq!(session.game.cargo.metal, 100.0);
    }
}
