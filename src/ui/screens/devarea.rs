//! The AREA OVERLAY developer toggle (`docs/LEGIBILITY.md` 7.1): sector, ring, realm and band,
//! and the way to the realm's keeper, drawn as one small block of text. It is the home of the
//! numbers the flight HUD no longer shows; the old numeric area tags (`Tag::Area`,
//! `Tag::AreaDetail` in `hud.rs`) share the same switch, `Game::dev.area_overlay`.
//!
//! The switch is the console's TOGGLES row AREA OVERLAY (`DevRow::AreaOverlay`), or the hook
//! `SSC_AREA_OVERLAY=1` for captures. Everything here is a view over the headless game: the
//! lines are a pure function (`overlay_lines`) and nothing off the toggle is computed. The
//! lead and tip lines of the spec join the block when those systems exist.

use crate::Session;
use crate::ui::theme::{self, Tone};
use bevy::prelude::*;
use ssc::sectormap::{KeeperHit, bearing_of};
use ssc::simulation::Game;

/// Whether the capture hook asks for the overlay (`SSC_AREA_OVERLAY=1`).
pub fn env_wanted() -> bool {
    std::env::var("SSC_AREA_OVERLAY").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Whether the overlay is on: the one switch the HUD tags and this block both read.
pub fn enabled(game: &Game) -> bool {
    game.dev.area_overlay
}

/// The realm band word for a realm intensity in [0, 1] (five equal steps, whisper to core).
pub fn band_word(intensity: f32) -> &'static str {
    const BANDS: [&str; 5] = ["WHISPER", "FRINGE", "RIM", "DEEP", "CORE"];
    BANDS[((intensity.clamp(0.0, 1.0) * 5.0) as usize).min(4)]
}

/// The keeper line: archetype and power, where it stands and how far, as sectors and a compass
/// word (always shown, discovery aside). None in the Cradle and in realms without a keeper.
pub fn keeper_line(game: &Game) -> Option<String> {
    let here = game.sector();
    let keeper = ssc::realm::keeper_of(game.seed(), here)?;
    let hit = KeeperHit::of(game.seed(), keeper);
    let (dx, dy) = (keeper.sector.x - here.x, keeper.sector.y - here.y);
    let place = if dx == 0 && dy == 0 {
        "in this sector".to_string()
    } else {
        format!(
            "{} sectors {}",
            hit.distance(here),
            bearing_of(dx, dy).label()
        )
    };
    Some(format!(
        "KEEPER {}+{:?} at ({},{}) {place}",
        keeper.archetype.label().to_uppercase(),
        keeper.power,
        keeper.sector.x,
        keeper.sector.y
    ))
}

/// The overlay's lines for the ship's sector.
pub fn overlay_lines(game: &Game) -> Vec<String> {
    let here = game.sector();
    let realm = game.realm_of(here);
    let mut lines = vec![format!(
        "SECTOR {},{}  RING {}  REALM {} {}  i={:.2} BAND {}",
        here.x,
        here.y,
        ssc::range::ring(here),
        realm.spec().title,
        realm.name.to_uppercase(),
        realm.intensity,
        band_word(realm.intensity)
    )];
    lines.extend(keeper_line(game));
    lines
}

/// The block's text node.
#[derive(Component)]
pub struct AreaOverlayText;

pub fn setup(mut commands: Commands) {
    commands.spawn((
        AreaOverlayText,
        Text::new(""),
        TextFont::from_font_size(theme::FONT_SMALL),
        TextColor(Tone::Warn.color()),
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(12),
            display: Display::None,
            ..default()
        },
    ));
}

pub fn update(
    mut session: ResMut<Session>,
    mut seen_env: Local<bool>,
    mut block: Query<(&mut Text, &mut Node), With<AreaOverlayText>>,
) {
    if !*seen_env {
        *seen_env = true;
        if env_wanted() {
            session.game.dev.area_overlay = true;
        }
    }
    let Ok((mut text, mut node)) = block.single_mut() else {
        return;
    };
    let covered = session.help || session.game.bench_open() || session.details_open();
    if !enabled(&session.game) || covered {
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    }
    if node.display != Display::Flex {
        node.display = Display::Flex;
    }
    let lines = overlay_lines(&session.game).join("\n");
    if text.0 != lines {
        text.0 = lines;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_run_whisper_to_core() {
        assert_eq!(band_word(0.0), "WHISPER");
        assert_eq!(band_word(0.5), "RIM");
        assert_eq!(band_word(1.0), "CORE");
    }

    #[test]
    fn home_has_a_realm_line_and_no_keeper() {
        let game = Game::new(ssc::config::MASTER_SEED);
        assert!(!enabled(&game));
        let lines = overlay_lines(&game);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("SECTOR 0,0  RING 0  REALM THE CRADLE"));
        assert!(keeper_line(&game).is_none());
    }
}
