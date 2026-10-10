//! The run summary: a panel over the middle of the screen at game over, a short one for the
//! recap after losing a ship.
use super::{AMBER, CYAN, MUTED};
use crate::Session;
use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct SummaryPanel;
#[derive(Component)]
pub(crate) struct SummaryLine(usize);
const SUMMARY_LINES: usize = 30;

pub(super) fn spawn(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(250),
                right: px(28),
                top: px(150),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            SummaryPanel,
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(26), px(18)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.9)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                Text::new(""),
                TextFont::from_font_size(14.0),
                TextLayout::justify(Justify::Center),
            ))
            .with_children(|panel| {
                for line in 0..SUMMARY_LINES {
                    panel.spawn((
                        SummaryLine(line),
                        TextSpan::new(""),
                        TextFont::from_font_size(14.0),
                        TextColor(MUTED),
                    ));
                }
            });
        });
}

pub(crate) fn update_summary(
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

/// The lines of the summary panel, or none when it should be hidden: the full run at game
/// over, a few lines for the recap after losing a ship.
pub(super) fn summary_lines(session: &Session) -> Vec<(String, Color, f32)> {
    let game = &session.game;
    let report = game.run_report();
    let mut out: Vec<(String, Color, f32)> = Vec::new();
    let light = Color::srgb(0.82, 0.88, 0.95);
    let blank = || (" ".to_string(), MUTED, 8.0);
    let list = |out: &mut Vec<(String, Color, f32)>| {
        if !report.extirpated.is_empty() {
            out.push(blank());
            out.push(("EXTIRPATED".into(), AMBER, 18.0));
            for entry in &report.extirpated {
                out.push((entry.clone(), AMBER, 15.0));
            }
            out.push((report.quip.into(), MUTED, 13.0));
        }
    };
    if game.game_over {
        out.push(("SHIP LOST".into(), CYAN, 30.0));
        out.push(("RUN SUMMARY".into(), MUTED, 13.0));
        out.push((report.title.to_uppercase(), AMBER, 20.0));
        out.push(blank());
        for line in &report.lines {
            out.push((line.clone(), light, 14.0));
        }
        list(&mut out);
        let legacy = game.legacy_report();
        if !legacy.is_empty() {
            out.push(blank());
            out.push(("LEGACY".into(), AMBER, 18.0));
            for line in legacy {
                out.push((line, AMBER, 14.0));
            }
        }
        if report.extirpated.is_empty() {
            out.push(blank());
            out.push((report.quip.into(), MUTED, 13.0));
        }
        out.push(blank());
        let best = match (session.new_best, session.best) {
            (true, _) => "NEW BEST RUN".to_string(),
            (false, Some(best)) => format!("BEST RUN THIS SESSION {best}"),
            _ => String::new(),
        };
        if !best.is_empty() {
            out.push((best, CYAN, 14.0));
        }
        out.push((
            "Press ENTER or gamepad A to launch again".into(),
            CYAN,
            16.0,
        ));
    } else if game.run.recap > 0.0 {
        let r = &game.run;
        out.push((
            format!(
                "SHIP LOST   {} {} LEFT",
                game.lives,
                if game.lives == 1 { "LIFE" } else { "LIVES" }
            ),
            CYAN,
            22.0,
        ));
        out.push((report.title.to_uppercase(), AMBER, 16.0));
        out.push((
            format!(
                "SCORE {}   DESTROYED {}   SECTORS EXPLORED {}   REGIONS {}   REALMS {}   MINED {:.0}",
                game.score,
                r.kills,
                r.sectors.len(),
                r.regions.len(),
                r.realms.len(),
                r.total_mined()
            ),
            light,
            14.0,
        ));
        if !r.extirpated.is_empty() {
            let names = r
                .extirpated
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            out.push((format!("EXTIRPATED  {names}"), AMBER, 14.0));
        }
    }
    out
}
