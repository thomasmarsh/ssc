mod audio;
mod glitchview;
mod hud;
mod juice;
mod nebula;
mod powerview;
mod presentation;
mod settings;
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
}

impl Session {
    /// Whether the details panel (and the radar with it) is showing.
    pub fn details_open(&self) -> bool {
        self.details || self.tab_held
    }
}

impl Default for Session {
    fn default() -> Self {
        Self {
            game: Game::new(0x535343),
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
        .init_resource::<audio::Audio>()
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
                settings::setup,
                nebula::setup,
                audio::setup,
            ),
        )
        .add_systems(FixedUpdate, simulate)
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
                presentation::draw,
                presentation::update_hud,
                presentation::scroll_panels,
                hud::update_texts,
                settings::update,
                presentation::update_summary,
                presentation::update_chart,
                smoke_run,
            )
                .chain(),
        )
        .run();
}

fn simulate(time: Res<Time<Fixed>>, mut session: ResMut<Session>) {
    if session.paused || session.chart.is_some() || session.settings.is_some() {
        return;
    }
    let input = session.input;
    let dt = time.delta_secs() * if session.slow { 0.35 } else { 1.0 };
    session.game.step(dt, input);
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
    mut audio: ResMut<audio::Audio>,
    mut exit: MessageWriter<AppExit>,
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
    if session.help && (escape || keys.just_pressed(KeyCode::F1)) {
        session.help = false;
    } else if session.settings.is_some() {
        if settings_controls(
            &keys,
            &pad,
            &mut session,
            &mut audio,
            &mut window,
            escape || start,
        ) == settings::Outcome::Quit
        {
            exit.write(AppExit::Success);
        }
        session.input = Input::default();
        return;
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
            }),
        };
    }
    if session.chart.is_some() {
        chart_controls(&keys, &gamepads, &mut session, &mut stick_latch);
        session.input = Input::default();
        return;
    }
    // The bench, while landed with it open: arrows pick a row (left and right a tab), enter or
    // space does the thing, Q takes from the stash, 1-7 jump to a tab, E closes. Nothing flies.
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
        // Beacon (locked until bought at the bench's RIG tab): H, or Y on a pad.
        if keys.just_pressed(KeyCode::KeyH) || pad(GamepadButton::North) {
            let _ = session.game.deploy_beacon();
        }
    }
    // Enter starts a new run once the last ship is lost (restarting mid-run is in the settings).
    if keys.just_pressed(KeyCode::Enter) && session.game.game_over {
        restart(&mut session);
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
}

/// Starts a fresh run (a lost one applies its legacy; restarting mid-run earns none).
fn restart(session: &mut Session) {
    session.game.reset();
    session.recorded = false;
    session.new_best = false;
    session.paused = false;
    session.slow = false;
}

/// The settings screen's keys: up and down choose a row, left, right and enter change it, Esc
/// (Start) closes. Returns what a change asked for beyond itself.
fn settings_controls(
    keys: &ButtonInput<KeyCode>,
    pad: &impl Fn(GamepadButton) -> bool,
    session: &mut Session,
    audio: &mut audio::Audio,
    window: &mut Window,
    close: bool,
) -> settings::Outcome {
    let Some(mut row) = session.settings else {
        return settings::Outcome::Stay;
    };
    if close || pad(GamepadButton::East) {
        session.settings = None;
        return settings::Outcome::Close;
    }
    if keys.just_pressed(KeyCode::ArrowDown) || pad(GamepadButton::DPadDown) {
        row = settings::step_index(row, 1);
    }
    if keys.just_pressed(KeyCode::ArrowUp) || pad(GamepadButton::DPadUp) {
        row = settings::step_index(row, -1);
    }
    let dir = i32::from(keys.just_pressed(KeyCode::ArrowRight) || pad(GamepadButton::DPadRight))
        - i32::from(keys.just_pressed(KeyCode::ArrowLeft) || pad(GamepadButton::DPadLeft));
    let confirm = keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || pad(GamepadButton::South);
    session.settings = Some(row);
    if dir == 0 && !confirm {
        return settings::Outcome::Stay;
    }
    let outcome = settings::change(settings::Setting::ALL[row], dir, session, audio, window);
    match outcome {
        settings::Outcome::Close => session.settings = None,
        settings::Outcome::Restart => {
            session.settings = None;
            restart(session);
        }
        _ => {}
    }
    outcome
}

/// The bench's keys; see `controls`.
fn bench_controls(
    keys: &ButtonInput<KeyCode>,
    pad: &impl Fn(GamepadButton) -> bool,
    session: &mut Session,
) {
    let game = &mut session.game;
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
    for (n, key) in DIGITS.into_iter().take(7).enumerate() {
        if keys.just_pressed(key) {
            game.bench_tab(n);
        }
    }
    if keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || pad(GamepadButton::South)
    {
        game.bench_confirm();
    }
    if keys.just_pressed(KeyCode::KeyQ) || pad(GamepadButton::West) {
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
    if keys.just_pressed(KeyCode::KeyZ) {
        cursor.sector = session.game.sector();
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

/// Optional bounded renderer smoke run; no effect in regular play.
#[derive(Resource, Default)]
struct SmokeRun {
    frames: u32,
    requested: bool,
}

fn smoke_run(
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
    // SSC_TIME=<seconds>: start the game clock there (wells and anything else posed by time).
    if run.frames == 0
        && let Some(time) = std::env::var("SSC_TIME")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
    {
        session.game.time = time;
    }
    // SSC_SPECIMEN=skipjack|veilwing|hullpick: three specimens of a rare power, a few hundred
    // units from the ship (use with SSC_TELEPORT somewhere past ring 3).
    if run.frames == 0
        && let Ok(name) = std::env::var("SSC_SPECIMEN")
    {
        use ssc::genome::{Genome, Species};
        let genome = match name.as_str() {
            "skipjack" => Genome::skipjack(),
            "veilwing" => Genome::veilwing(),
            "hullpick" => Genome::hullpick(),
            "stormcap" => Genome::stormcap(),
            "argus" => Genome::argus(),
            "gloomfeeder" => Genome::gloomfeeder(),
            "dizzard" => Genome::dizzard(),
            "pushwhale" => Genome::pushwhale(),
            "tarbloom" => Genome::tarbloom(),
            "lenswyrm" => Genome::lenswyrm(),
            "tidegorger" => Genome::tidegorger(),
            "splitter" => Genome::splitter(),
            "murmur" => Genome::murmur(),
            _ => Genome::default(),
        };
        let near = if matches!(name.as_str(), "stormcap" | "dizzard") {
            200.0
        } else {
            420.0
        };
        if matches!(name.as_str(), "stormcap" | "dizzard" | "argus") {
            // The jammers only work on a ship that is not in grace.
            session.game.player_invulnerability = 0.0;
        }
        let ship = session.game.player().map_or(Vec2::ZERO, |p| p.position);
        for k in 0..3 {
            let at = ship + Vec2::from_angle(0.6 + k as f32 * 2.1) * (near + 90.0 * k as f32);
            session.game.place_creature(&Species::of(genome), at);
        }
    }
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
    // SSC_SPECIMEN_TELL=1: just before the screenshot, run the game until a blink is announced.
    if run.frames + 3 == limit && std::env::var_os("SSC_SPECIMEN_TELL").is_some() {
        for _ in 0..4000 {
            session.game.step(0.02, ssc::simulation::Input::default());
            let told = session.game.bodies.iter().any(|b| {
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
                        .jam
                        .is_some_and(|t| t.progress() > 0.5)
            });
            if told {
                break;
            }
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
        smoke_chart(&mut session, run.frames);
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
    // in hand, a pad down, landed, or landed with the bench open on tab SSC_BENCH=0..5).
    if run.frames == 4
        && let Ok(mode) = std::env::var("SSC_PAD")
    {
        smoke_pads(&mut session.game, &mode);
    }
    // SSC_SUMMARY=over|death: stage a run (with a few extirpations) and show its summary.
    if run.frames == 4
        && let Ok(mode) = std::env::var("SSC_SUMMARY")
    {
        smoke_summary(&mut session, &mode);
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
            game.teleport(Vec2::new(4.0 * SECTOR_SIZE, 2.0 * SECTOR_SIZE));
        }
        20 => {
            game.ping();
        }
        90 => {
            session.chart = Some(ChartCursor {
                sector: ssc::world::SectorId { x: 2, y: 1 },
                label: PinLabel::Camp,
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
        game.collect(Item::Surge(upgrades::roll_surge(&mut rng, &source)));
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
        Item::Surge(upgrades::roll_surge(&mut rng, &source)),
        Item::Surge(upgrades::roll_surge(&mut rng, &source)),
    ];
    for (i, item) in items.into_iter().enumerate() {
        let spot = center + Vec2::from_angle(i as f32 * 0.8) * 240.0;
        game.drop_item(spot, Vec2::ZERO, item);
    }
}

/// Stages the landing-pad states for a screenshot: beside the nearest planetoid with a kit
/// (`kit`), with a pad down (`deploy`), landed on it (`land`), or landed with the bench open
/// on the tab named by SSC_BENCH (`bench`). Hurts the ship a little so mending shows.
fn smoke_pads(game: &mut Game, mode: &str) {
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
