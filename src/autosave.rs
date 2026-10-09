//! The Bevy side of saving: when to write and load, nothing more. The state, the format and
//! the file are `simulation::save` and `savefile`. On by default; `SSC_NO_SAVE=1` turns it off,
//! and a scripted run (`SSC_SMOKE_FRAMES`) leaves the player's save alone unless it opts in
//! with `SSC_SAVE=1` (use `SSC_SAVE_DIR` for a scratch slot). The title menu is `titlemenu`.

use crate::Session;
use bevy::app::AppExit;
use bevy::prelude::*;
use ssc::savefile;
use ssc::simulation::Game;
use ssc::simulation::save::SaveState;

/// Seconds between autosaves of a living run.
const INTERVAL: f32 = 30.0;

pub fn enabled() -> bool {
    let set = |name| std::env::var_os(name).is_some();
    !set("SSC_NO_SAVE") && (set("SSC_SAVE") || !set("SSC_SMOKE_FRAMES"))
}

/// The saved run, if saving is on and there is a readable one. A save that cannot be read is
/// reported and skipped so earlier valid candidates can still be continued.
pub fn load() -> Option<Game> {
    // Unit tests never read the player's real save.
    if cfg!(test) || !enabled() {
        return None;
    }
    let dir = savefile::default_dir();
    load_from_dir(&dir)
}

fn load_from_dir(dir: &std::path::Path) -> Option<Game> {
    let candidates = match savefile::candidates(dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("save: cannot list {}: {e}", dir.display());
            return None;
        }
    };
    for path in candidates {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("save: cannot read {}: {e}", path.display());
                continue;
            }
        };
        match SaveState::from_text(&text) {
            Ok((state, generator)) => {
                let (game, report) = Game::from_save(state, generator);
                eprintln!(
                    "save: loaded {} (world deltas {})",
                    path.display(),
                    if report.world_deltas_kept {
                        "kept"
                    } else {
                        "dropped: generator changed"
                    }
                );
                return Some(game);
            }
            Err(e) => eprintln!("save: {}: {e}", path.display()),
        }
    }
    None
}

fn write_state(state: &SaveState) {
    let dir = savefile::default_dir();
    if let Err(e) = savefile::write(&dir, &state.to_text()) {
        eprintln!("save: cannot write {}: {e}", dir.display());
    }
}

/// Saves the current living game.
fn write(game: &Game) {
    if game.game_over || game.player().is_none() {
        return;
    }
    write_state(&game.save_state());
}

/// Save the recovered ship immediately after any death.
pub fn settle(game: &Game) {
    if enabled() {
        write(game);
    }
}

/// A separate player-controlled save, with a visible success or failure result.
pub fn manual(game: &Game) -> Result<(), String> {
    if !enabled() {
        return Err("SAVING IS DISABLED".into());
    }
    if game.player().is_none() {
        return Err("NO SHIP TO SAVE".into());
    }
    savefile::write_manual(&savefile::default_dir(), &game.save_state().to_text())
        .map_err(|e| format!("SAVE FAILED: {e}"))
}

/// Clears prior saves before starting a new game. The menu remains open on failure.
pub fn erase() -> Result<(), String> {
    if enabled() {
        savefile::delete(&savefile::default_dir())
            .map_err(|e| format!("CANNOT START NEW GAME: {e}"))?;
    }
    Ok(())
}

/// Autosaves every `INTERVAL` real seconds and once more as the app exits.
pub fn autosave(
    time: Res<Time<Real>>,
    session: Res<Session>,
    mut exits: MessageReader<AppExit>,
    mut since: Local<f32>,
) {
    // The title menu is still choosing which run this is; nothing is saved until it closes.
    if !enabled() || session.menu.is_some() {
        return;
    }
    *since += time.delta_secs();
    if exits.read().next().is_some() || *since >= INTERVAL {
        *since = 0.0;
        write(&session.game);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn continue_uses_the_latest_valid_save_across_manual_and_autosaves() {
        let dir = std::env::temp_dir().join(format!("ssc-continue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut game = Game::new(42);
        game.score = 10;
        savefile::write_manual(&dir, &game.save_state().to_text()).unwrap();
        game.score = 20;
        savefile::write(&dir, &game.save_state().to_text()).unwrap();
        assert_eq!(load_from_dir(&dir).unwrap().score, 20);
        savefile::write(&dir, "corrupt").unwrap();
        assert_eq!(load_from_dir(&dir).unwrap().score, 20);
        game.score = 30;
        savefile::write_manual(&dir, &game.save_state().to_text()).unwrap();
        assert_eq!(load_from_dir(&dir).unwrap().score, 30);
        // Backing up an old manual save must not make it newer than a later autosave.
        std::fs::write(dir.join("manual.ron"), "corrupt manual").unwrap();
        assert_eq!(load_from_dir(&dir).unwrap().score, 20);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
