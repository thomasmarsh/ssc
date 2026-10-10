//! The run summary on the widget layer: the full panel at game over, a short recap after losing
//! a ship. `Game::run_report` and `legacy_report` are the view-model; this file groups their
//! lines (stat rows become label and value chips) and lays them out. Launching again is the
//! existing Enter or A in `controls`; the panel shows it as a focused button, so a pad alone
//! reads it, and takes no input of its own.

use super::title::{line, panel, wrapped};
use crate::Session;
use crate::presentation::{AMBER, CYAN, MUTED};
use crate::ui::focus::ItemId;
use crate::ui::glyphs::{Device, Glyph};
use crate::ui::icons::Icon;
use crate::ui::theme::{self, Tone};
use crate::ui::widgets::{self, ButtonView, Hint};
use bevy::prelude::*;

const LIGHT: Color = theme::TEXT;
/// Below this many logical pixels of height the panel runs compact.
const COMPACT_HEIGHT: f32 = 620.0;

/// One figure of the run: a muted label over its value.
#[derive(Clone, PartialEq, Debug)]
pub struct Stat {
    pub label: String,
    pub value: String,
}

/// A titled list (EXTIRPATED, LEGACY).
#[derive(Clone, PartialEq, Debug)]
pub struct Section {
    pub title: &'static str,
    pub lines: Vec<String>,
    pub note: String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SummaryView {
    /// The full game-over panel (false for the short recap).
    pub over: bool,
    pub title: String,
    pub run_title: String,
    pub stats: Vec<Vec<Stat>>,
    pub sections: Vec<Section>,
    pub footer: String,
    pub best: String,
    pub device: Device,
    /// A short window: figures run inline and the launch button comes first, so nothing the
    /// player needs is cut off at the bottom.
    pub compact: bool,
}

/// Splits a report line into figures at runs of two or more spaces; the last word of each
/// piece is the value ("SECTORS EXPLORED 17" is SECTORS EXPLORED and 17). A piece with no
/// value to split off stays a label alone.
pub fn stats(text: &str) -> Vec<Stat> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let (piece, tail) = match rest.find("  ") {
            Some(at) => (&rest[..at], rest[at..].trim_start()),
            None => (rest, ""),
        };
        let piece = piece.trim();
        match piece.rsplit_once(' ') {
            Some((label, value)) => out.push(Stat {
                label: label.trim().to_string(),
                value: value.to_string(),
            }),
            None => out.push(Stat {
                label: piece.to_string(),
                value: String::new(),
            }),
        }
        rest = tail;
    }
    out
}

/// The view, or none when the panel should be hidden: the full run at game over, a few lines
/// for the recap after losing a ship.
pub fn view(session: &Session, device: Device, compact: bool) -> Option<SummaryView> {
    let game = &session.game;
    let report = game.run_report();
    if game.game_over {
        let mut sections = Vec::new();
        if !report.extirpated.is_empty() {
            sections.push(Section {
                title: "EXTIRPATED",
                lines: report.extirpated.clone(),
                note: report.quip.to_string(),
            });
        }
        let legacy = game.legacy_report();
        if !legacy.is_empty() {
            sections.push(Section {
                title: "LEGACY",
                lines: legacy,
                note: String::new(),
            });
        }
        let best = match (session.new_best, session.best) {
            (true, _) => "NEW BEST RUN".to_string(),
            (false, Some(best)) => format!("BEST RUN THIS SESSION {best}"),
            _ => String::new(),
        };
        Some(SummaryView {
            over: true,
            title: "SHIP LOST".into(),
            run_title: report.title.to_uppercase(),
            stats: report.lines.iter().map(|l| stats(l)).collect(),
            sections,
            // With no extirpations the quip closes the report instead.
            footer: if report.extirpated.is_empty() {
                report.quip.to_string()
            } else {
                String::new()
            },
            best,
            device,
            compact,
        })
    } else if game.run.recap > 0.0 {
        let r = &game.run;
        let figure = |label: &str, value: String| Stat {
            label: label.into(),
            value,
        };
        let mut sections = Vec::new();
        if !r.extirpated.is_empty() {
            sections.push(Section {
                title: "EXTIRPATED",
                lines: vec![
                    r.extirpated
                        .iter()
                        .map(|e| e.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ],
                note: String::new(),
            });
        }
        Some(SummaryView {
            over: false,
            title: format!(
                "SHIP LOST   {} {} LEFT",
                game.lives,
                if game.lives == 1 { "LIFE" } else { "LIVES" }
            ),
            run_title: report.title.to_uppercase(),
            stats: vec![vec![
                figure("SCORE", game.score.to_string()),
                figure("DESTROYED", r.kills.to_string()),
                figure("SECTORS EXPLORED", r.sectors.len().to_string()),
                figure("REGIONS", r.regions.len().to_string()),
                figure("REALMS", r.realms.len().to_string()),
                figure("MINED", format!("{:.0}", r.total_mined())),
            ]],
            sections,
            footer: String::new(),
            best: String::new(),
            device,
            compact,
        })
    } else {
        None
    }
}

// ---- Bevy -----------------------------------------------------------------------------------

#[derive(Component)]
pub struct SummaryRoot;

pub fn setup(mut commands: Commands) {
    commands.spawn((
        SummaryRoot,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(60),
            bottom: px(60),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            display: Display::None,
            ..default()
        },
    ));
}

pub fn render(
    session: Res<Session>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    camera: Query<&Camera, With<Camera2d>>,
    ui_scale: Res<UiScale>,
    mut commands: Commands,
    mut root: Query<(Entity, &mut Node), With<SummaryRoot>>,
    mut last: Local<(Option<SummaryView>, Device)>,
) {
    let Ok((entity, mut node)) = root.single_mut() else {
        return;
    };
    if pads.iter().any(|p| p.get_pressed().next().is_some()) {
        last.1 = Device::Pad;
    } else if keys.get_pressed().next().is_some() {
        last.1 = Device::Keys;
    }
    let height = camera
        .iter()
        .find_map(|c| c.logical_viewport_size())
        .map_or(800.0, |size| size.y / ui_scale.0.max(0.1));
    let now = view(&session, last.1, height < COMPACT_HEIGHT);
    let want = if now.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != want {
        node.display = want;
    }
    if last.0 == now {
        return;
    }
    commands.entity(entity).despawn_children();
    if let Some(view) = &now {
        commands
            .entity(entity)
            .with_children(|root| build(root, view));
    }
    last.0 = now;
}

fn build(root: &mut ChildSpawnerCommands, view: &SummaryView) {
    let width = if view.over { 700.0 } else { 820.0 };
    panel(root, width, |panel| {
        let compact = view.compact;
        centered(panel, |c| {
            line(
                c,
                view.title.clone(),
                if view.over && !compact { 30.0 } else { 22.0 },
                CYAN,
            );
            if view.over && !compact {
                line(c, "RUN SUMMARY", theme::FONT_SMALL, MUTED);
            }
            line(
                c,
                view.run_title.clone(),
                if view.over && !compact { 20.0 } else { 15.0 },
                AMBER,
            );
        });
        // The way back in comes first when room is short.
        if view.over && compact {
            launch(panel, view);
        }
        if compact {
            let all: Vec<Stat> = view.stats.iter().flatten().cloned().collect();
            inline_stats(panel, &all);
        } else {
            for row in &view.stats {
                stat_row(panel, row);
            }
        }
        let size = if compact { 12.0 } else { 14.0 };
        for section in &view.sections {
            centered(panel, |c| {
                line(
                    c,
                    section.title,
                    if compact { 13.0 } else { theme::FONT_TITLE },
                    AMBER,
                );
                for l in &section.lines {
                    wrapped(c, l.clone(), size, AMBER);
                }
                if !section.note.is_empty() && !compact {
                    line(c, section.note.clone(), theme::FONT_SMALL, MUTED);
                }
            });
        }
        if !view.footer.is_empty() && !compact {
            centered(panel, |c| {
                line(c, view.footer.clone(), theme::FONT_SMALL, MUTED)
            });
        }
        if !view.over {
            return;
        }
        if !view.best.is_empty() {
            centered(panel, |c| line(c, view.best.clone(), 14.0, CYAN));
        }
        if !compact {
            launch(panel, view);
        }
    });
}

/// The focused LAUNCH AGAIN button and its prompts.
fn launch(panel: &mut ChildSpawnerCommands, view: &SummaryView) {
    widgets::button_row(
        panel,
        &[ButtonView {
            id: ItemId(0),
            label: "LAUNCH AGAIN".into(),
            icon: Some(Icon::Regen),
            focused: true,
            tone: Tone::Accent,
        }],
    );
    widgets::hint_bar(
        panel,
        &[
            Hint {
                glyph: Glyph::Confirm,
                text: "launch again",
            },
            Hint {
                glyph: Glyph::Back,
                text: "settings",
            },
        ],
        view.device,
    );
}

/// Every figure on a wrapped run of "value label" pieces (the compact layout).
fn inline_stats(parent: &mut ChildSpawnerCommands, stats: &[Stat]) {
    parent
        .spawn(Node {
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::Center,
            column_gap: px(12),
            row_gap: px(1),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|r| {
            for stat in stats {
                r.spawn(Node {
                    column_gap: px(4),
                    align_items: AlignItems::Baseline,
                    ..default()
                })
                .with_children(|cell| {
                    if stat.value.is_empty() {
                        line(cell, stat.label.clone(), 12.0, LIGHT);
                    } else {
                        line(cell, stat.value.clone(), 13.0, LIGHT);
                        line(cell, stat.label.clone(), 10.0, MUTED);
                    }
                });
            }
        });
}

/// A centered column of lines.
fn centered(parent: &mut ChildSpawnerCommands, content: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(content);
}

/// A row of figures, each a small label over its value.
fn stat_row(parent: &mut ChildSpawnerCommands, row: &[Stat]) {
    parent
        .spawn(Node {
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::Center,
            column_gap: px(18),
            row_gap: px(4),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|r| {
            for stat in row {
                r.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|cell| {
                    if stat.value.is_empty() {
                        line(cell, stat.label.clone(), 14.0, LIGHT);
                    } else {
                        line(cell, stat.value.clone(), 16.0, LIGHT);
                        line(cell, stat.label.clone(), 11.0, MUTED);
                    }
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_line_splits_into_figures() {
        let s = stats("SECTORS EXPLORED 17   REGIONS 1  THREAT FACED x2.8");
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].label, "SECTORS EXPLORED");
        assert_eq!(s[0].value, "17");
        assert_eq!(s[2].value, "x2.8");
    }

    #[test]
    fn a_line_without_a_value_stays_a_label() {
        let s = stats("PERFECT");
        assert_eq!(
            s,
            vec![Stat {
                label: "PERFECT".into(),
                value: String::new()
            }]
        );
        assert!(stats("   ").is_empty());
    }
}
