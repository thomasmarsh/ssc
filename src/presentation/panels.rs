//! HUD panel systems: text updates, summary, chart and scrolling.
use super::banners::{FeedLine, PadBanner};
use super::bench::{BenchLine, BenchPanelNode};
use super::help::{HelpBody, HelpPanel};
use super::{DETAILS_BOTTOM, DETAILS_TOP, Overlay, Scrollable};
use super::{banners, bench, help};
use crate::Session;
use bevy::prelude::*;

pub(super) type BenchNodeOnly = (With<BenchPanelNode>, Without<HelpPanel>, Without<HelpBody>);
pub(super) type HelpBodyOnly = (With<HelpBody>, Without<HelpPanel>);
pub(super) type BenchSpan = (
    &'static mut TextSpan,
    &'static mut TextColor,
    &'static BenchLine,
);
pub(super) type BenchOnly = Without<FeedLine>;
pub(super) type BannerOnly = (
    With<PadBanner>,
    Without<BenchLine>,
    Without<FeedLine>,
    Without<Overlay>,
);

/// Scrolls the open details or help panel with the mouse wheel.
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
    if delta == 0.0 || !(session.details_open() || session.help) {
        return;
    }
    for mut position in &mut panels {
        position.0.y = (position.0.y - delta).max(0.0);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_hud(
    session: Res<Session>,
    mut feed: Query<(&mut TextSpan, &mut TextColor, &FeedLine)>,
    mut pad_text: Single<(&mut Text, &mut TextColor), BannerOnly>,
    mut bench: Query<BenchSpan, BenchOnly>,
    mut bench_node: Single<&mut Node, BenchNodeOnly>,
    mut overlay: Single<&mut Text, With<Overlay>>,
    mut help_node: Single<&mut Node, With<HelpPanel>>,
    mut help_body: Single<&mut Node, HelpBodyOnly>,
    camera: Single<&Camera, With<Camera2d>>,
    ui_scale: Res<UiScale>,
) {
    let (pad_text, pad_color) = &mut *pad_text;
    banners::apply_banner(&session, pad_text, pad_color);
    let viewport = camera
        .logical_viewport_size()
        .unwrap_or(Vec2::new(1280.0, 800.0))
        / ui_scale.0;
    bench::apply(&session.game, viewport, &mut bench_node, &mut bench);
    let details = session.details_open();
    // Between the top row and the bottom cluster, whatever the window: the panels scroll.
    let room = camera
        .logical_viewport_size()
        .map_or(600.0, |size| {
            size.y / ui_scale.0 - DETAILS_TOP - DETAILS_BOTTOM
        })
        .max(120.0);
    help::apply(session.help, room, &mut help_node, &mut help_body);
    banners::apply_feed(&session, details, &mut feed);
    let message = if !session.game.game_over && session.paused {
        "PAUSED\nPress P to resume"
    } else {
        ""
    };
    if overlay.0 != message {
        overlay.0 = message.into();
    }
}
