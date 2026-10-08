//! The Bevy side of saving: when to write and load, nothing more. The state, the format and
//! the file are `simulation::save` and `savefile`. Opt-in with `SSC_SAVE=1` until the
//! continue / new / delete menu exists (workstream 12 slice 4), so a normal run is unchanged.

use crate::Session;
use bevy::app::AppExit;
use bevy::prelude::*;
use ssc::savefile;
use ssc::simulation::Game;
use ssc::simulation::save::SaveState;

/// Seconds between autosaves of a living run.
const INTERVAL: f32 = 30.0;

pub fn enabled() -> bool {
    std::env::var_os("SSC_SAVE").is_some()
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

fn write(game: &Game) {
    // A finished run has nothing to continue; the last autosave stays as it was.
    if game.game_over || game.player().is_none() {
        return;
    }
    let dir = savefile::default_dir();
    if let Err(e) = savefile::write(&dir, &game.save_state().to_text()) {
        eprintln!("save: cannot write {}: {e}", dir.display());
    }
}

/// Autosaves every `INTERVAL` real seconds and once more as the app exits.
pub fn autosave(
    time: Res<Time<Real>>,
    session: Res<Session>,
    mut exits: MessageReader<AppExit>,
    mut since: Local<f32>,
) {
    if !enabled() {
        return;
    }
    *since += time.delta_secs();
    if exits.read().next().is_some() || *since >= INTERVAL {
        *since = 0.0;
        write(&session.game);
    }
}
