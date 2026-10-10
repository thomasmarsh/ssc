//! HUD panel systems: text updates, summary, chart and scrolling.
use super::*;

pub(super) type BenchNodeOnly = (
    With<BenchPanelNode>,
    Without<DetailsPanel>,
    Without<HelpPanel>,
    Without<HelpBody>,
);
pub(super) type HelpBodyOnly = (With<HelpBody>, Without<HelpPanel>, Without<DetailsPanel>);
pub(super) type BenchSpan = (
    &'static mut TextSpan,
    &'static mut TextColor,
    &'static BenchLine,
);
pub(super) type BenchOnly = (Without<RigLine>, Without<FeedLine>);
pub(super) type BannerOnly = (
    With<PadBanner>,
    Without<BenchLine>,
    Without<RigLine>,
    Without<FeedLine>,
    Without<Hud>,
    Without<Overlay>,
);

/// Scrolls the open details or help panel with the mouse wheel.
pub fn scroll_panels(
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
pub fn update_hud(
    session: Res<Session>,
    mut feed: Query<(&mut TextSpan, &mut TextColor, &FeedLine), Without<RigLine>>,
    mut rig: Query<(&mut TextSpan, &mut TextColor, &RigLine), Without<FeedLine>>,
    mut pad_text: Single<(&mut Text, &mut TextColor), BannerOnly>,
    mut bench: Query<BenchSpan, BenchOnly>,
    mut bench_node: Single<&mut Node, BenchNodeOnly>,
    mut hud: Single<&mut Text, (With<Hud>, Without<Overlay>)>,
    mut overlay: Single<&mut Text, (With<Overlay>, Without<Hud>)>,
    mut details_node: Single<&mut Node, (With<DetailsPanel>, Without<HelpPanel>)>,
    mut help_node: Single<&mut Node, (With<HelpPanel>, Without<DetailsPanel>)>,
    mut help_body: Single<&mut Node, HelpBodyOnly>,
    camera: Single<&Camera, With<Camera2d>>,
    ui_scale: Res<UiScale>,
) {
    let game = &session.game;
    let (text, tint) = if game.bench_open() || session.help {
        (String::new(), CYAN)
    } else {
        pad_banner(game)
    };
    if pad_text.0.0 != text {
        pad_text.0.0 = text;
    }
    pad_text.1.0 = tint;
    let viewport = camera
        .logical_viewport_size()
        .unwrap_or(Vec2::new(1280.0, 800.0))
        / ui_scale.0;
    let bench_width = (viewport.x - 32.0).min(680.0);
    let bench_height = viewport.y - DETAILS_TOP - DETAILS_BOTTOM;
    bench_node.width = px(bench_width);
    let panel = bench_lines(game, bench_width, bench_height);
    let bench_display = if panel.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if bench_node.display != bench_display {
        bench_node.display = bench_display;
    }
    for (mut span, mut color, line) in &mut bench {
        match panel.get(line.0) {
            Some((text, tint)) => {
                if span.0 != *text {
                    span.0 = text.clone();
                }
                color.0 = *tint;
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
    let details = session.details_open();
    // Between the top row and the bottom cluster, whatever the window: the panels scroll.
    let room = camera
        .logical_viewport_size()
        .map_or(600.0, |size| {
            size.y / ui_scale.0 - DETAILS_TOP - DETAILS_BOTTOM
        })
        .max(120.0);
    if details_node.max_height != px(room) {
        details_node.max_height = px(room);
        help_body.max_height = px(room);
    }
    let want = |on: bool| if on { Display::Flex } else { Display::None };
    if details_node.display != want(details) {
        details_node.display = want(details);
    }
    if help_node.display != want(session.help) {
        help_node.display = want(session.help);
    }
    if details {
        let status = situation_text(&session);
        if hud.0 != status {
            hud.0 = status;
        }
    }
    let now = if details { rig_lines(game) } else { Vec::new() };
    for (mut span, mut color, line) in &mut rig {
        if let Some((text, tint)) = now.get(line.0) {
            if span.0 != *text {
                span.0 = text.clone();
            }
            color.0 = *tint;
        }
    }
    // Toasts give way to the panels: both would claim the middle of the screen.
    let count = if details || session.help {
        0
    } else {
        if game.bench_open() {
            0
        } else {
            game.notices.len()
        }
    };
    for (mut span, mut color, line) in &mut feed {
        // Newest at the bottom; older lines sit above and fade as they expire.
        let shown = (line.0 + count)
            .checked_sub(FEED_LINES)
            .and_then(|i| game.notices.get(i));
        match shown {
            Some(notice) => {
                let text = format!("{}\n", notice.text);
                if span.0 != text {
                    span.0 = text;
                }
                color.0 = rarity_color(notice.rarity).with_alpha(notice.remaining.min(1.0));
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
    let message = if game.game_over {
        String::new()
    } else if session.paused {
        "PAUSED\nPress P to resume".into()
    } else {
        String::new()
    };
    if overlay.0 != message {
        overlay.0 = message;
    }
}

pub fn update_summary(
    session: Res<Session>,
    mut panel: Single<&mut Node, With<SummaryPanel>>,
    mut spans: Query<(&mut TextSpan, &mut TextColor, &mut TextFont, &SummaryLine)>,
) {
    let lines = summary_lines(&session);
    let display = if lines.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if panel.display != display {
        panel.display = display;
    }
    let last = lines.len().saturating_sub(1);
    for (mut span, mut color, mut font, line) in &mut spans {
        match lines.get(line.0) {
            Some((text, tint, size)) => {
                let text = if line.0 == last {
                    text.clone()
                } else {
                    format!("{text}\n")
                };
                if span.0 != text {
                    span.0 = text;
                }
                color.0 = *tint;
                let size = bevy::text::FontSize::Px(*size);
                if font.font_size != size {
                    font.font_size = size;
                }
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
}

pub fn update_chart(
    session: Res<Session>,
    mut spans: Query<(&mut TextSpan, &mut TextColor, &ChartSpan)>,
) {
    let lines = chart_lines(&session);
    for (mut span, mut color, line) in &mut spans {
        match lines.get(line.0) {
            Some((text, tint)) => {
                if span.0 != *text {
                    span.0 = text.clone();
                }
                color.0 = *tint;
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
}
