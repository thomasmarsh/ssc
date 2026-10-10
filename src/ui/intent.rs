//! What a screen asks for, and the one place it is carried out. A screen turns focus events into
//! `UiIntent`s; `dispatch` calls the existing headless `Game` methods and returns the reply to
//! show. Replies of the registry are the console reply verbatim (`Game::tune_command`), so a
//! refusal reads exactly as `tune_set` words it. No price, gate, cap or permission lives here.

use super::theme::Tone;
use crate::Session;
use ssc::simulation::Game;
use ssc::simulation::dev::DevRow;
use std::path::{Path, PathBuf};

/// A request from a screen to the game or the session.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum UiIntent {
    TuneSet {
        name: &'static str,
        value: f32,
    },
    TuneReset {
        name: &'static str,
    },
    TuneResetAll,
    /// A new game under the current tuning (the world generates under it).
    Regenerate,
    OverridesLoad,
    OverridesSave,
    DevChange {
        row: DevRow,
        dir: i32,
    },
    Close,
}

/// A line of feedback with its tone.
#[derive(Clone, PartialEq, Debug)]
pub struct Reply {
    pub text: String,
    pub tone: Tone,
}

impl Reply {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }

    /// A registry reply: refusals start with `error:` and read red.
    pub fn from_command(text: String) -> Self {
        let tone = if text.starts_with("error:") {
            Tone::Bad
        } else if text.contains("adjusted") || text.contains("needs regen") {
            Tone::Warn
        } else {
            Tone::Good
        };
        Self { text, tone }
    }
}

/// Where the overrides file is read and written: `SSC_TUNING` when set (the file the run was
/// launched with), else `tuning.ron` in the save folder.
pub fn overrides_path() -> PathBuf {
    std::env::var_os("SSC_TUNING")
        .map(PathBuf::from)
        .unwrap_or_else(|| ssc::savefile::default_dir().join("tuning.ron"))
}

/// Applies the overrides file at `path` on top of the game.
pub fn overrides_load(game: &mut Game, path: &Path) -> Reply {
    match std::fs::read_to_string(path) {
        Err(e) => Reply::new(
            format!("error: cannot read {}: {e}", path.display()),
            Tone::Bad,
        ),
        Ok(text) => {
            let report = game.tune_load_overrides(&text);
            match report.problems.first() {
                None => Reply::new(
                    format!(
                        "loaded {} override(s) from {}",
                        report.applied.len(),
                        path.display()
                    ),
                    Tone::Good,
                ),
                Some(first) => Reply::new(
                    format!(
                        "error: applied {}, {} problem(s): {first}",
                        report.applied.len(),
                        report.problems.len()
                    ),
                    Tone::Bad,
                ),
            }
        }
    }
}

/// Writes the non-default entries to `path`.
pub fn overrides_save(game: &Game, path: &Path) -> Reply {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return Reply::new(
            format!("error: cannot create {}: {e}", dir.display()),
            Tone::Bad,
        );
    }
    match std::fs::write(path, game.tune_overrides_text()) {
        Ok(()) => Reply::new(format!("saved overrides to {}", path.display()), Tone::Good),
        Err(e) => Reply::new(
            format!("error: cannot write {}: {e}", path.display()),
            Tone::Bad,
        ),
    }
}

/// Carries out the intents that only need the game. `None` for ones that need the session.
pub fn dispatch_game(game: &mut Game, intent: UiIntent, path: &Path) -> Option<Reply> {
    Some(match intent {
        UiIntent::TuneSet { name, value } => {
            Reply::from_command(game.tune_command(&format!("set {name} {value}")))
        }
        UiIntent::TuneReset { name } => {
            Reply::from_command(game.tune_command(&format!("reset {name}")))
        }
        UiIntent::TuneResetAll => Reply::from_command(game.tune_command("reset all")),
        UiIntent::OverridesLoad => overrides_load(game, path),
        UiIntent::OverridesSave => overrides_save(game, path),
        UiIntent::DevChange { row, dir } => {
            game.dev_change(row, dir);
            return None;
        }
        UiIntent::Regenerate | UiIntent::Close => return None,
    })
}

/// Carries out one intent against the session; the reply (if any) is shown on the screen.
/// Returns whether the screen should close.
pub fn dispatch(session: &mut Session, intent: UiIntent) -> (Option<Reply>, bool) {
    match intent {
        UiIntent::Close => (None, true),
        UiIntent::Regenerate => {
            // The console stays open over the new world, so tuning can go on.
            let console = session.console.take();
            crate::restart(session);
            session.console = console;
            (
                Some(Reply::new(
                    "generated a new universe under the current tuning",
                    Tone::Good,
                )),
                false,
            )
        }
        other => (
            dispatch_game(&mut session.game, other, &overrides_path()),
            false,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssc::config::MASTER_SEED;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ssc-ui-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("tuning.ron")
    }

    #[test]
    fn a_set_replies_as_the_registry_words_it() {
        let mut game = Game::new(MASTER_SEED);
        let path = scratch("set");
        let ok = dispatch_game(
            &mut game,
            UiIntent::TuneSet {
                name: "adapt_max",
                value: 0.4,
            },
            &path,
        )
        .unwrap();
        assert_eq!(ok.text, "adapt_max = 0.4");
        assert_eq!(ok.tone, Tone::Good);
        assert_eq!(game.tune_get("adapt_max"), Some(0.4));
        let clamped = dispatch_game(
            &mut game,
            UiIntent::TuneSet {
                name: "adapt_max",
                value: 7.0,
            },
            &path,
        )
        .unwrap();
        assert!(clamped.text.contains("adjusted from 7"), "{}", clamped.text);
        assert_eq!(clamped.tone, Tone::Warn);
    }

    #[test]
    fn a_refusal_shows_the_registry_reply_verbatim() {
        let mut game = Game::new(MASTER_SEED);
        let path = scratch("refuse");
        let expected = Game::new(MASTER_SEED).tune_command("set impact_min_speed 5000");
        assert!(expected.starts_with("error:"), "{expected}");
        let reply = dispatch_game(
            &mut game,
            UiIntent::TuneSet {
                name: "impact_min_speed",
                value: 5000.0,
            },
            &path,
        )
        .unwrap();
        assert_eq!(reply.text, expected);
        assert_eq!(reply.tone, Tone::Bad);
        let unknown =
            dispatch_game(&mut game, UiIntent::TuneReset { name: "no_such" }, &path).unwrap();
        assert_eq!(unknown.tone, Tone::Bad);
    }

    #[test]
    fn reset_one_and_reset_all_return_to_defaults() {
        let mut game = Game::new(MASTER_SEED);
        let path = scratch("reset");
        game.tune_set("adapt_max", 0.3).unwrap();
        game.tune_set("dash_cost", 9.0).unwrap();
        dispatch_game(&mut game, UiIntent::TuneReset { name: "adapt_max" }, &path);
        assert!(game.tune_list(None).iter().filter(|r| r.modified).count() == 1);
        dispatch_game(&mut game, UiIntent::TuneResetAll, &path);
        assert!(!game.tuning_modified());
    }

    #[test]
    fn overrides_save_then_load_round_trips() {
        let path = scratch("round");
        let mut game = Game::new(MASTER_SEED);
        game.tune_set("adapt_max", 0.4).unwrap();
        let saved = overrides_save(&game, &path);
        assert_eq!(saved.tone, Tone::Good, "{}", saved.text);
        let mut fresh = Game::new(MASTER_SEED);
        let loaded = overrides_load(&mut fresh, &path);
        assert_eq!(loaded.tone, Tone::Good, "{}", loaded.text);
        assert_eq!(fresh.tune_get("adapt_max"), Some(0.4));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_bad_or_missing_file_is_reported_not_fatal() {
        let path = scratch("bad");
        let mut game = Game::new(MASTER_SEED);
        let missing = overrides_load(&mut game, &path);
        assert_eq!(missing.tone, Tone::Bad);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{ "adapt_max": 0.4, "no_such_entry": 1.0 }"#).unwrap();
        let partial = overrides_load(&mut game, &path);
        assert_eq!(partial.tone, Tone::Bad);
        assert!(partial.text.contains("no_such_entry"), "{}", partial.text);
        assert_eq!(game.tune_get("adapt_max"), Some(0.4), "valid entries apply");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_dev_change_reaches_the_game() {
        let mut game = Game::new(MASTER_SEED);
        let path = scratch("dev");
        assert!(!game.dev.invulnerable);
        let reply = dispatch_game(
            &mut game,
            UiIntent::DevChange {
                row: DevRow::Invulnerable,
                dir: 0,
            },
            &path,
        );
        assert!(reply.is_none());
        assert!(game.dev.invulnerable);
    }
}
