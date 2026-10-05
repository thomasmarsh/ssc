mod presentation;

use bevy::{
    app::AppExit,
    camera::ScalingMode,
    prelude::*,
    render::{
        RenderPlugin,
        settings::{Backends, PowerPreference, RenderCreation, WgpuSettings},
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};
use ssc::simulation::upgrades::{self, Item, Source};
use ssc::simulation::{Game, Input};
use ssc::world::Rng;

#[derive(Resource)]
pub struct Session {
    pub game: Game,
    pub input: Input,
    pub paused: bool,
    pub slow: bool,
    pub radar: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            game: Game::new(0x535343),
            input: Input::default(),
            paused: false,
            slow: false,
            radar: true,
        }
    }
}

fn main() {
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
        .add_systems(Startup, presentation::setup)
        .add_systems(FixedUpdate, simulate)
        .add_systems(
            Update,
            (
                controls,
                camera,
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

fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    view: Single<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut session: ResMut<Session>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    if keys.just_pressed(KeyCode::KeyP) || keys.just_pressed(KeyCode::Pause) {
        session.paused = !session.paused;
    }
    if keys.just_pressed(KeyCode::KeyS) {
        session.slow = !session.slow;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        session.radar = !session.radar;
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
    session.input = Input {
        thrust: if keys.pressed(KeyCode::ArrowUp) {
            1.0
        } else {
            0.0
        },
        turn: keys.pressed(KeyCode::ArrowLeft) as u8 as f32
            - keys.pressed(KeyCode::ArrowRight) as u8 as f32,
        brake: keys.pressed(KeyCode::ArrowDown),
        fire: keys.pressed(KeyCode::KeyA)
            || keys.pressed(KeyCode::Space)
            || mouse.pressed(MouseButton::Left),
        aim_direction,
    };
}

/// The camera trails the ship with a little look-ahead so fast flight shows what is coming.
fn camera(
    session: Res<Session>,
    time: Res<Time>,
    mut view: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let game = &session.game;
    let target = game
        .player()
        .map_or(game.focus, |ship| ship.position + ship.velocity * 0.3);
    let smoothing = 1.0 - (-6.0 * time.delta_secs()).exp();
    let (transform, projection) = &mut *view;
    let current = transform.translation.truncate();
    transform.translation = current.lerp(target, smoothing).extend(0.0);
    if let Projection::Orthographic(projection) = &mut **projection {
        projection.scaling_mode = ScalingMode::FixedVertical {
            viewport_height: presentation::VIEW_HEIGHT,
        };
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
        Item::Scrap(50),
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
