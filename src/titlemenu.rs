//! The title menu: CONTINUE and NEW GAME. Shown at every normal launch. The
//! choices are pure state here (`TitleMenu`, tested); the adapter in `main.rs` does the file
//! and game work for the outcome, and `update` draws the panel in the settings screen's style.

use crate::Session;
use crate::presentation::{AMBER, CYAN, MUTED};
use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Continue,
    NewRun,
}

impl Row {
    fn label(self) -> &'static str {
        match self {
            Self::Continue => "CONTINUE",
            Self::NewRun => "NEW GAME",
        }
    }
}

/// What a confirmed row asks the adapter to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Stay,
    Continue,
    NewRun,
}

#[derive(Clone, Debug)]
pub struct TitleMenu {
    row: usize,
    has_save: bool,
    /// A destructive row waits for a second confirm.
    armed: Option<Row>,
    /// One line about the saved run, shown under the rows.
    summary: String,
}

impl TitleMenu {
    pub fn new(has_save: bool, summary: String) -> Self {
        Self {
            row: 0,
            has_save,
            armed: None,
            summary,
        }
    }

    pub fn rows(&self) -> &'static [Row] {
        if self.has_save {
            &[Row::Continue, Row::NewRun]
        } else {
            &[Row::NewRun]
        }
    }

    pub fn error(&mut self, message: String) {
        self.summary = message;
        self.armed = None;
    }

    pub fn has_save(&self) -> bool {
        self.has_save
    }

    pub fn step(&mut self, delta: i32) {
        let n = self.rows().len() as i32;
        self.row = (self.row as i32 + delta.signum()).rem_euclid(n) as usize;
        self.armed = None;
    }

    /// Enter on the selected row. NEW GAME needs a second press when replacing saved progress.
    pub fn confirm(&mut self) -> Outcome {
        let row = self.rows()[self.row];
        match row {
            Row::Continue => Outcome::Continue,
            Row::NewRun if !self.has_save || self.armed == Some(row) => Outcome::NewRun,
            _ => {
                self.armed = Some(row);
                Outcome::Stay
            }
        }
    }
}

#[derive(Component)]
pub struct TitlePanel;
#[derive(Component)]
pub struct TitleLine(usize);

pub fn setup(mut commands: Commands) {
    commands
        .spawn((
            TitlePanel,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(110),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(45),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(34), px(20)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.012, 0.022, 0.045)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                Text::new(""),
                TextFont::from_font_size(14.0),
            ))
            .with_children(|panel| {
                for n in 0..6 {
                    panel.spawn((
                        TitleLine(n),
                        TextSpan::new(""),
                        TextFont::from_font_size(14.0),
                        TextColor(MUTED),
                    ));
                }
            });
        });
}

pub fn update(
    session: Res<Session>,
    mut panel: Single<&mut Node, With<TitlePanel>>,
    mut lines: Query<(&mut TextSpan, &mut TextColor, &TitleLine)>,
) {
    let want = if session.menu.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if panel.display != want {
        panel.display = want;
    }
    let Some(menu) = &session.menu else {
        return;
    };
    let light = Color::srgb(0.82, 0.88, 0.95);
    let mut out: Vec<(String, Color)> = vec![("SSC / DEEP SPACE\n\n".into(), CYAN)];
    for (i, row) in menu.rows().iter().enumerate() {
        let on = i == menu.row;
        let armed = menu.armed == Some(*row);
        let text = format!(
            "{} {:<14}{}\n",
            if on { ">" } else { " " },
            row.label(),
            if armed { "ENTER AGAIN TO ERASE" } else { "" },
        );
        let color = match (on, armed) {
            (_, true) => AMBER,
            (true, _) => CYAN,
            _ => light,
        };
        out.push((text, color));
    }
    let note = if menu.has_save {
        format!("\n{}\n\n", menu.summary)
    } else {
        "\n\n".to_string()
    };
    out.push((format!("{note}UP / DOWN choose    ENTER confirms"), MUTED));
    for (mut span, mut color, line) in &mut lines {
        match out.get(line.0) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn with_save() -> TitleMenu {
        TitleMenu::new(true, "score 10".into())
    }

    #[test]
    fn continue_is_first_and_needs_no_confirm() {
        assert_eq!(with_save().confirm(), Outcome::Continue);
    }

    #[test]
    fn a_new_run_over_a_save_asks_twice() {
        let mut menu = with_save();
        menu.step(1);
        assert_eq!(menu.confirm(), Outcome::Stay);
        assert_eq!(menu.confirm(), Outcome::NewRun);
    }

    #[test]
    fn moving_disarms() {
        let mut menu = with_save();
        menu.step(1);
        menu.confirm();
        menu.step(1);
        menu.step(-1);
        assert_eq!(menu.confirm(), Outcome::Stay);
    }

    #[test]
    fn without_a_save_a_new_run_is_immediate() {
        assert_eq!(
            TitleMenu::new(false, String::new()).confirm(),
            Outcome::NewRun
        );
    }
}
