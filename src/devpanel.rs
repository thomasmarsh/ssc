//! The developer panel (backquote, or the guide button on a pad; only with `SSC_DEV=1`). It is a
//! plain view onto `Game::dev` (see `ssc::simulation::dev`): every value and every change comes
//! from the headless game, this file only lays out the rows and reads keys. It pauses the game,
//! like the settings screen, and is styled like it.

use crate::Session;
use crate::presentation::{AMBER, CYAN, MUTED};
use bevy::prelude::*;
use ssc::simulation::dev::DevRow;

/// The selected row after moving by `delta` (wrapping).
pub fn step_index(index: usize, delta: i32) -> usize {
    (index as i32 + delta.signum()).rem_euclid(DevRow::ALL.len() as i32) as usize
}

/// Whether a row is a switch that is currently on (it reads amber).
fn lit(row: DevRow, session: &Session) -> bool {
    let dev = &session.game.dev;
    match row {
        DevRow::Invulnerable => dev.invulnerable,
        DevRow::InfiniteAmmo => dev.infinite_ammo,
        DevRow::FreePurchases => dev.free_purchases,
        DevRow::NoCooldowns => dev.no_cooldowns,
        DevRow::UnlimitedLives => dev.unlimited_lives,
        DevRow::FreezeEnemies => dev.freeze_enemies,
        DevRow::TimeScale => dev.time_scale() != 1.0,
        _ => false,
    }
}

#[derive(Component)]
pub struct DevPanel;
#[derive(Component)]
pub struct DevLine(usize);

pub fn setup(mut commands: Commands) {
    commands
        .spawn((
            DevPanel,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(64),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(41),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(30), px(14)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.012, 0.022, 0.045)),
                BorderColor::all(Color::srgba(1.0, 0.62, 0.28, 0.55)),
                Text::new(""),
                TextFont::from_font_size(14.0),
            ))
            .with_children(|panel| {
                for n in 0..DevRow::ALL.len() + 2 {
                    panel.spawn((
                        DevLine(n),
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
    mut panel: Single<&mut Node, With<DevPanel>>,
    mut lines: Query<(&mut TextSpan, &mut TextColor, &DevLine)>,
) {
    let want = if session.dev_panel.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if panel.display != want {
        panel.display = want;
    }
    let Some(selected) = session.dev_panel else {
        return;
    };
    let light = Color::srgb(0.82, 0.88, 0.95);
    let mut out: Vec<(String, Color)> = vec![("DEVELOPER\n\n".into(), AMBER)];
    for (i, row) in DevRow::ALL.into_iter().enumerate() {
        let on = i == selected;
        let spacer = if matches!(
            row,
            DevRow::TimeScale | DevRow::GrantPart | DevRow::Teleport
        ) {
            "\n"
        } else {
            ""
        };
        let text = format!(
            "{} {:<22} {}\n{spacer}",
            if on { ">" } else { " " },
            row.label(),
            session.game.dev_value(row),
        );
        let color = match (on, lit(row, &session)) {
            (true, _) => CYAN,
            (false, true) => AMBER,
            (false, false) => light,
        };
        out.push((text, color));
    }
    // The chosen row explains itself on a line of its own, so the panel never changes width.
    out.push((
        format!(
            "\n{}\nUP / DOWN choose    LEFT / RIGHT change    ENTER do    BACKQUOTE or ESC closes",
            DevRow::ALL[selected].hint()
        ),
        MUTED,
    ));
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

    #[test]
    fn the_selection_wraps_both_ways() {
        assert_eq!(step_index(0, -1), DevRow::ALL.len() - 1);
        assert_eq!(step_index(DevRow::ALL.len() - 1, 1), 0);
        assert_eq!(step_index(3, 1), 4);
    }
}
