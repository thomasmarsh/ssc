//! HUD panel systems: the bench's per-frame apply and wheel scrolling of the details.
use super::bench::BenchRender;
use super::{Scrollable, bench};
use crate::Session;
use bevy::prelude::*;

/// Scrolls the open details panel with the mouse wheel (the help scrolls its own cursor).
pub(crate) fn scroll_panels(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut session: ResMut<Session>,
    mut panels: Query<&mut ScrollPosition, With<Scrollable>>,
) {
    let mut delta = 0.0;
    for event in wheel.read() {
        delta += match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => event.y * 28.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => event.y,
        };
    }
    if delta != 0.0 && session.game.bench_open() {
        session.game.bench_move(-delta.signum() as i32);
        return;
    }
    if delta == 0.0 || !session.details_open() {
        return;
    }
    for mut position in &mut panels {
        position.0.y = (position.0.y - delta).max(0.0);
    }
}

pub(crate) fn update_hud(
    session: Res<Session>,
    mut bench: BenchRender,
    camera: Single<&Camera, With<Camera2d>>,
    ui_scale: Res<UiScale>,
) {
    let viewport = camera
        .logical_viewport_size()
        .unwrap_or(Vec2::new(1280.0, 800.0))
        / ui_scale.0;
    bench::apply(&session.game, viewport, &mut bench);
}
