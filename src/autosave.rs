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
/// reported and left alone on disk (the next autosave moves it to the backup slot).
pub fn load() -> Option<Game> {
    if !enabled() {
        return None;
    }
    let dir = savefile::default_dir();
    let text = match savefile::read(&dir) {
        Ok(Some(text)) => text,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("save: cannot read {}: {e}", dir.display());
            return None;
        }
    };
    match SaveState::from_text(&text) {
        Ok((state, generator)) => {
            let (game, report) = Game::from_save(state, generator);
            eprintln!(
                "save: loaded {} (world deltas {})",
                dir.display(),
                if report.world_deltas_kept {
                    "kept"
                } else {
                    "dropped: generator changed"
                }
            );
            Some(game)
        }
        Err(e) => {
            eprintln!("save: {e}");
            None
        }
    }
}

fn write_state(state: &SaveState) {
    let dir = savefile::default_dir();
    if let Err(e) = savefile::write(&dir, &state.to_text()) {
        eprintln!("save: cannot write {}: {e}", dir.display());
    }
}

/// Saves a living run. A finished run is never saved as it died (see `settle`).
fn write(game: &Game) {
    if game.game_over || game.player().is_none() {
        return;
    }
    write_state(&game.save_state());
}

/// Called right after the ship is lost. Saving now closes the reload-to-undo hole: a lost life
/// is kept lost, and a finished run is replaced in the slot by the run that follows it (the
/// bequest is made here, once, by `next_run`), so no save can bring the dead run back.
pub fn settle(game: &Game) {
    if !enabled() {
        return;
    }
    if game.game_over {
        write_state(&game.next_run().save_state());
    } else {
        write(game);
    }
}

/// Deletes the saved run (the title menu's delete and new run).
pub fn erase() {
    if !enabled() {
        return;
    }
    let dir = savefile::default_dir();
    if let Err(e) = savefile::delete(&dir) {
        eprintln!("save: cannot delete {}: {e}", dir.display());
    }
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
