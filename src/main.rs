mod audio;
mod presentation;

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
use ssc::simulation::{BodyKind, Cargo, Game, Input, Material};
use ssc::world::RockKind;
use ssc::world::{QUADRANT_SIZE, Rng};

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
    Quadrant,
}

impl CameraView {
    fn next(self) -> Self {
        match self {
            Self::Close => Self::Wide,
            Self::Wide => Self::Far,
            Self::Far => Self::Quadrant,
            Self::Quadrant => Self::Close,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Close => "CLOSE",
            Self::Wide => "WIDE",
            Self::Far => "FAR",
            Self::Quadrant => "QUADRANT",
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
            Self::Quadrant => ScalingMode::AutoMin {
                min_width: QUADRANT_SIZE * 1.1,
                min_height: QUADRANT_SIZE * 1.1,
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

#[derive(Resource)]
pub struct Session {
    pub game: Game,
    pub input: Input,
    pub paused: bool,
    pub slow: bool,
    pub radar: bool,
    pub camera_view: CameraView,
    pub style: RenderStyle,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            game: Game::new(0x535343),
            input: Input::default(),
            paused: false,
            slow: false,
            radar: true,
            camera_view: CameraView::default(),
            style: RenderStyle::default(),
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
        .add_systems(Startup, (presentation::setup, audio::setup))
        .add_systems(FixedUpdate, simulate)
        .add_systems(
            Update,
            (
                controls,
                camera,
                apply_style,
                audio::apply_mute,
                audio::play_cues,
                presentation::draw,
                presentation::update_hud,
                smoke_run,
            )
                .chain(),
        )
        .run();
}

fn simulate(time: Res<Time<Fixed>>, mut session: ResMut<Session>) {
    if session.paused {
        return;
    }
    let input = session.input;
    let dt = time.delta_secs() * if session.slow { 0.35 } else { 1.0 };
    session.game.step(dt, input);
}

/// Every physical input device the player can use.
#[derive(bevy::ecs::system::SystemParam)]
struct Devices<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
}

fn controls(
    devices: Devices,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    view: Single<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut session: ResMut<Session>,
    mut audio: ResMut<audio::Audio>,
    mut exit: MessageWriter<AppExit>,
) {
    let Devices {
        keys,
        mouse,
        gamepads,
    } = devices;
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    if keys.just_pressed(KeyCode::KeyP) || keys.just_pressed(KeyCode::Pause) {
        session.paused = !session.paused;
    }
    if keys.just_pressed(KeyCode::KeyS) {
        session.slow = !session.slow;
    }
    if keys.just_pressed(KeyCode::Tab) {
        session.radar = !session.radar;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        session.camera_view = session.camera_view.next();
    }
    if keys.just_pressed(KeyCode::KeyN) {
        audio.muted = !audio.muted;
    }
    if keys.just_pressed(KeyCode::KeyV) {
        session.style = session.style.next();
    }
    // Weapon profiles: ] next, [ previous, 1-9 pick directly; L and R shoulders on a pad.
    // B toggles the boosts (Y on a pad). Switching is instant and ignored while paused.
    // With the bench open the same keys drive it instead: 1-5 pick a tab (d-pad left/right on
    // a pad), [ ] (shoulders) pick a target, F (A) does the thing, Q (X) takes from the stash.
    // E (Select) opens and closes the bench while landed.
    if !session.paused {
        let pad = |button| gamepads.iter().any(|pad| pad.just_pressed(button));
        let next = keys.just_pressed(KeyCode::BracketRight) || pad(GamepadButton::RightTrigger);
        let previous = keys.just_pressed(KeyCode::BracketLeft) || pad(GamepadButton::LeftTrigger);
        if session.game.bench_open() {
            if next {
                session.game.bench_move(1);
            }
            if previous {
                session.game.bench_move(-1);
            }
            for (n, key) in DIGITS.into_iter().take(5).enumerate() {
                if keys.just_pressed(key) {
                    session.game.bench_tab(n);
                }
            }
            if pad(GamepadButton::DPadRight) {
                session.game.bench_tab_step(1);
            }
            if pad(GamepadButton::DPadLeft) {
                session.game.bench_tab_step(-1);
            }
            if keys.just_pressed(KeyCode::KeyF) || pad(GamepadButton::South) {
                session.game.bench_confirm();
            }
            if keys.just_pressed(KeyCode::KeyQ) || pad(GamepadButton::West) {
                session.game.bench_alt();
            }
        } else {
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
            if keys.just_pressed(KeyCode::KeyR) || pad(GamepadButton::West) {
                session.game.toggle_repair();
            }
        }
        if keys.just_pressed(KeyCode::KeyE) || pad(GamepadButton::Select) {
            session.game.bench_toggle();
        }
        if keys.just_pressed(KeyCode::KeyB) || pad(GamepadButton::North) {
            session.game.toggle_boosts();
        }
        // Landing pads: K crafts a kit (d-pad up), L lands, lifts off or deploys (B on a
        // pad), I toggles part insurance (d-pad down).
        if keys.just_pressed(KeyCode::KeyK) || pad(GamepadButton::DPadUp) {
            session.game.craft_kit();
        }
        if keys.just_pressed(KeyCode::KeyL) || pad(GamepadButton::East) {
            session.game.pad_action();
        }
        if keys.just_pressed(KeyCode::KeyI) || pad(GamepadButton::DPadDown) {
            session.game.toggle_insurance();
        }
    }
    if keys.just_pressed(KeyCode::Enter) {
        session.game.reset();
        session.paused = false;
        session.slow = false;
    }
    if keys.just_pressed(KeyCode::F1) {
        window.mode = if window.mode == WindowMode::Windowed {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        };
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
    // Twin-stick: left stick thrusts in any direction, right stick aims and fires.
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

/// The camera trails the ship with a little look-ahead so fast flight shows what is coming.
fn camera(
    session: Res<Session>,
    time: Res<Time>,
    mut view: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
    mut previous_view: Local<CameraView>,
) {
    let game = &session.game;
    let target = if session.camera_view == CameraView::Quadrant {
        game.quadrant().center()
    } else {
        game.player()
            .map_or(game.focus, |ship| ship.position + ship.velocity * 0.3)
    };
    let smoothing = 1.0 - (-6.0 * time.delta_secs()).exp();
    let (transform, projection) = &mut *view;
    let current = transform.translation.truncate();
    // Snap quadrant framing and the return to close view, keeping the ship visible.
    transform.translation =
        if session.camera_view == CameraView::Quadrant || *previous_view == CameraView::Quadrant {
            target
        } else {
            current.lerp(target, smoothing)
        }
        .extend(0.0);
    if let Projection::Orthographic(projection) = &mut **projection {
        projection.scaling_mode = session.camera_view.scaling_mode();
    }
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
            Ok("quadrant") => CameraView::Quadrant,
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
    // in hand, a pad down, landed, or landed with the bench open on tab SSC_BENCH=0..4).
    if run.frames == 4
        && let Ok(mode) = std::env::var("SSC_PAD")
    {
        smoke_pads(&mut session.game, &mode);
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
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
    } else {
        exit.write(AppExit::Success);
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
        game.pad_action();
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
