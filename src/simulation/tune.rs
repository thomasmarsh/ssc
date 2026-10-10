//! Headless control of the tunables registry (docs/DEVTOOLS.md, Phase B): the API a console, a
//! dev panel or an overrides file sits on. `Game::tune` is the resolved struct the rules read;
//! everything here is the slow, by-name path.
//!
//! What a tweak can and cannot do:
//! - values are clamped into each entry's range (or refused, never NaN or infinite) and a change
//!   that breaks a cross-field rule is refused whole (`tuning::validate`);
//! - a changed value marks the run as dev-touched (the DEV tag), is part of `state_digest`, and
//!   rides in the save, so two runs with different tuning never compare equal by accident;
//! - `Live` entries apply on the next tick; `Regen` entries also set `tuning_needs_regen`
//!   (regeneration of already-loaded sectors is not built; a restart or reload applies them);
//! - a default tuning is bit-identical to a game without the registry.

use super::tunables::{self, Applied, Effect, OverrideReport, TuneError, Unit};
use super::tuning::{TUNABLES, Tunables};
use super::*;

/// One row of the tunables listing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TuneRow {
    pub name: &'static str,
    pub group: &'static str,
    pub doc: &'static str,
    pub value: f32,
    pub default: f32,
    pub min: f32,
    pub max: f32,
    pub unit: Unit,
    pub effect: Effect,
    /// The value differs from the default.
    pub modified: bool,
}

impl Game {
    /// A new game whose world is generated under `tune` (so `Regen` entries apply from the
    /// start). `Game::new(seed)` is this with the defaults.
    pub fn with_tuning(seed: u64, tune: Tunables) -> Self {
        let mut game = Self::blank(seed);
        game.tune = tune;
        game.apply_culture_tuning();
        game.stream_sectors();
        game.spawn_player(Vec2::ZERO);
        game.seed_home_pad();
        game
    }

    /// The resolved tunables the rules read.
    pub fn tunables(&self) -> &Tunables {
        &self.tune
    }

    /// An entry's current value by name.
    pub fn tune_get(&self, name: &str) -> Option<f32> {
        self.tune.get(name)
    }

    /// Sets an entry by name, clamped into its range; see `tunables::set` and the module docs.
    pub fn tune_set(&mut self, name: &str, value: f32) -> Result<Applied, TuneError> {
        let applied = self.tune.set(name, value)?;
        self.tune_changed(applied.regen);
        Ok(applied)
    }

    /// Puts one entry back to its default (through the same rules as `tune_set`).
    pub fn tune_reset(&mut self, name: &str) -> Result<Applied, TuneError> {
        let applied = self.tune.reset(name)?;
        self.tune_changed(applied.regen);
        Ok(applied)
    }

    /// Puts every entry back to its default. A reset of `Regen` entries that changed leaves a
    /// regeneration pending, as any change does.
    pub fn tune_reset_all(&mut self) {
        let regen = TUNABLES.iter().any(|info| {
            info.effect == Effect::Regen && self.tune.get(info.name) != Some(info.default)
        });
        self.tune = Tunables::DEFAULT;
        self.tune_changed(regen);
    }

    /// The rows of one group (or all), in declaration order.
    pub fn tune_list(&self, group: Option<&str>) -> Vec<TuneRow> {
        let values = tunables::Registry::read_all(&self.tune);
        TUNABLES
            .iter()
            .zip(values)
            .filter(|(info, _)| group.is_none_or(|g| info.group == g))
            .map(|(info, value)| TuneRow {
                name: info.name,
                group: info.group,
                doc: info.doc,
                value,
                default: info.default,
                min: info.min,
                max: info.max,
                unit: info.unit,
                effect: info.effect,
                modified: value != info.default,
            })
            .collect()
    }

    /// The group names in declaration order.
    pub fn tune_groups() -> Vec<&'static str> {
        Tunables::groups()
    }

    /// Whether any entry differs from its default (the run is dev-touched; the HUD wears DEV).
    pub fn tuning_modified(&self) -> bool {
        self.tune != Tunables::DEFAULT
    }

    /// Whether a `Regen` entry changed since the world was generated, so what is already loaded
    /// no longer matches the tuning. Cleared by a reload or restart (see `with_tuning`).
    pub fn tuning_needs_regen(&self) -> bool {
        self.tune_regen
    }

    /// Applies an overrides file (a RON map of name to number; see `Tunables::from_overrides`).
    /// Valid entries apply, the rest are reported.
    pub fn tune_load_overrides(&mut self, text: &str) -> OverrideReport {
        let report = tunables::apply_overrides(&mut self.tune, text);
        self.tune_changed(report.regen());
        report
    }

    /// The overrides file text of the non-default entries.
    pub fn tune_overrides_text(&self) -> String {
        self.tune.overrides_to_string()
    }

    /// Re-derives what is cached from the tunables and records a pending regeneration.
    fn tune_changed(&mut self, regen: bool) {
        self.tune_regen |= regen;
        super::tuning_gen::install(&self.tune);
        self.apply_culture_tuning();
        self.cargo.extra = self.loadout.skills.cargo_bonus(&self.tune);
    }

    /// A one-line console: `list [group|modified]`, `groups`, `get <name>`, `set <name> <value>`,
    /// `reset <name|all>`, `regen`, `help`. A leading `/` is allowed. Always returns a reply (an
    /// error is a reply that starts with `error:`); nothing here panics on any input.
    pub fn tune_command(&mut self, line: &str) -> String {
        let line = line.trim().trim_start_matches('/');
        let mut words = line.split_whitespace();
        let Some(verb) = words.next() else {
            return "error: empty command (try help)".to_string();
        };
        let args: Vec<&str> = words.collect();
        match (verb, args.as_slice()) {
            ("help", _) => "commands: list [group|modified], groups, get <name>, \
                            set <name> <value>, reset <name|all>, regen"
                .to_string(),
            ("groups", _) => Self::tune_groups().join(" "),
            ("list", rest) => {
                let rows = match rest {
                    [] => self.tune_list(None),
                    ["modified"] => self
                        .tune_list(None)
                        .into_iter()
                        .filter(|r| r.modified)
                        .collect(),
                    [group] if Self::tune_groups().contains(group) => self.tune_list(Some(group)),
                    [other] => {
                        return format!(
                            "error: no group {other} (groups: {})",
                            Self::tune_groups().join(" ")
                        );
                    }
                    _ => return "error: usage: list [group|modified]".to_string(),
                };
                if rows.is_empty() {
                    return "nothing".to_string();
                }
                rows.iter()
                    .map(|r| {
                        format!(
                            "{}: {} {}{}{}",
                            r.group,
                            r.name,
                            r.value,
                            if r.modified {
                                format!(" (default {})", r.default)
                            } else {
                                String::new()
                            },
                            if r.effect == Effect::Live {
                                String::new()
                            } else {
                                format!(" [{}]", r.effect.label())
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            ("get", [name]) => match tunables::find::<Tunables>(name) {
                Some(info) => {
                    let value = self.tune.get(name).unwrap_or(info.default);
                    format!(
                        "{name} = {value} {} (default {}, range {} to {}, {}): {}",
                        info.unit.label(),
                        info.default,
                        info.min,
                        info.max,
                        info.effect.label(),
                        info.doc
                    )
                }
                None => format!("error: unknown tunable {name}"),
            },
            ("set", [name, value]) => match value.parse::<f32>() {
                Ok(value) => match self.tune_set(name, value) {
                    Ok(a) => Self::applied_reply(&a),
                    Err(e) => format!("error: {e}"),
                },
                Err(_) => format!("error: {value} is not a number"),
            },
            ("reset", ["all"]) => {
                self.tune_reset_all();
                "all tunables reset to their defaults".to_string()
            }
            ("reset", [name]) => match self.tune_reset(name) {
                Ok(a) => Self::applied_reply(&a),
                Err(e) => format!("error: {e}"),
            },
            ("regen", _) => if self.tune_regen {
                "regeneration pending: reload or restart applies the changed generation entries"
            } else {
                "no regeneration pending"
            }
            .to_string(),
            _ => format!("error: unknown or malformed command {line:?} (try help)"),
        }
    }

    fn applied_reply(a: &Applied) -> String {
        let mut out = format!("{} = {}", a.name, a.applied);
        if a.adjusted {
            out.push_str(&format!(" (adjusted from {})", a.requested));
        }
        if a.regen {
            out.push_str(" [needs regen]");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MASTER_SEED;
    const DT: f32 = 1.0 / 60.0;

    fn fly(game: &mut Game, ticks: usize) {
        for n in 0..ticks {
            let input = Input {
                thrust: 1.0,
                turn: if n % 90 < 30 { 1.0 } else { 0.0 },
                fire: n % 7 < 3,
                ..Input::default()
            };
            game.step(DT, input);
        }
    }

    #[test]
    fn a_default_tuning_changes_nothing() {
        let mut plain = Game::new(MASTER_SEED);
        let mut tuned = Game::with_tuning(MASTER_SEED, Tunables::default());
        // Touch an entry and put it back: the state returns to default exactly.
        tuned.tune_set("adapt_max", 0.3).unwrap();
        assert!(tuned.tuning_modified());
        tuned.tune_reset("adapt_max").unwrap();
        assert!(!tuned.tuning_modified());
        fly(&mut plain, 600);
        fly(&mut tuned, 600);
        assert_eq!(plain.state_digest(), tuned.state_digest());
    }

    #[test]
    fn a_nondefault_tuning_marks_the_run_and_the_digest() {
        let mut plain = Game::new(MASTER_SEED);
        let mut tuned = Game::new(MASTER_SEED);
        assert!(!tuned.tuning_modified());
        tuned.tune_set("dash_cost", 9.0).unwrap();
        assert!(tuned.tuning_modified());
        assert_ne!(plain.state_digest(), tuned.state_digest());
        // Different values digest differently too.
        let mut other = Game::new(MASTER_SEED);
        other.tune_set("dash_cost", 10.0).unwrap();
        assert_ne!(tuned.state_digest(), other.state_digest());
        // And the difference shows up in the sub-digest that owns it.
        assert!(
            plain
                .state_digest()
                .differing(&tuned.state_digest())
                .contains(&"ship")
        );
        fly(&mut plain, 30);
        fly(&mut tuned, 30);
        assert_ne!(plain.state_digest(), tuned.state_digest());
    }

    #[test]
    fn tuning_rides_in_the_save() {
        let mut game = Game::new(MASTER_SEED);
        game.tune_set("parry_cost", 20.0).unwrap();
        game.tune_set("regard_start", 5.0).unwrap();
        let text = game.save_state().to_text();
        let (state, generator) = save::SaveState::from_text(&text).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.tune, game.tune);
        assert!(loaded.tuning_modified());
        // A default run writes no tuning at all.
        let plain = Game::new(MASTER_SEED).save_state().to_text();
        assert!(!plain.contains("tuning"), "{plain}");
    }

    #[test]
    fn culture_drift_entries_route_through_the_saved_clock() {
        let plain = Game::new(MASTER_SEED);
        assert_eq!(plain.tune_get("culture_drift_temperature"), Some(0.0));
        assert_eq!(plain.tune_get("culture_drift_timescale"), Some(86_400.0));
        assert_eq!(plain.culture_clock().temperature(), 0.0);
        assert_eq!(
            plain.culture_clock().timescale(),
            crate::culture::DEFAULT_TIMESCALE
        );
        assert!(!plain.tuning_modified() && !plain.culture_modified());

        let mut game = Game::new(MASTER_SEED);
        game.tune_set("culture_drift_temperature", 0.5).unwrap();
        game.tune_set("culture_drift_timescale", 1000.0).unwrap();
        assert_eq!(game.culture_clock().temperature(), 0.5);
        assert_eq!(game.culture_clock().timescale(), 1000.0);
        assert!(game.tuning_modified() && game.culture_modified());
        // Out of range clamps, nonfinite and a non-positive timescale never reach the clock.
        let a = game.tune_set("culture_drift_temperature", 7.0).unwrap();
        assert_eq!(a.applied, 1.0);
        assert!(game.tune_set("culture_drift_timescale", f32::NAN).is_err());
        assert_eq!(
            game.tune_set("culture_drift_timescale", -5.0)
                .unwrap()
                .applied,
            1.0
        );
        assert_eq!(game.culture_clock().timescale(), 1.0);
        game.tune_set("culture_drift_timescale", 1000.0).unwrap();
        // The direct headless control keeps the entries in step.
        assert!(game.configure_culture_drift(0.25, 2000.0));
        assert_eq!(game.tune_get("culture_drift_temperature"), Some(0.25));
        assert_eq!(game.tune_get("culture_drift_timescale"), Some(2000.0));
        assert!(!game.configure_culture_drift(f64::NAN, 2000.0));
        assert_eq!(game.tune_get("culture_drift_temperature"), Some(0.25));
        // The clock is saved on its own; the overrides never carry a second copy.
        game.civs.societies.advance(500.0, &game.tune);
        let phase = game.culture_clock().phase();
        assert!(phase > 0.0);
        assert!(!game.save_state().to_text().contains("culture_drift_"));
        let (state, generator) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.culture_clock(), game.culture_clock());
        assert_eq!(loaded.tune_get("culture_drift_temperature"), Some(0.25));
        // Reset refreezes the current phase, not the original epoch.
        game.tune_reset("culture_drift_temperature").unwrap();
        assert_eq!(game.culture_clock().temperature(), 0.0);
        assert_eq!(game.culture_clock().phase(), phase);
        game.tune_reset_all();
        assert_eq!(
            game.culture_clock().timescale(),
            crate::culture::DEFAULT_TIMESCALE
        );
        // An overrides file reaches the clock, and so does a game built with the tuning.
        let report = game.tune_load_overrides("{\"culture_drift_temperature\": 0.75}");
        assert!(report.ok());
        assert_eq!(game.culture_clock().temperature(), 0.75);
        let mut tune = Tunables::default();
        tune.set("culture_drift_temperature", 0.1).unwrap();
        let built = Game::with_tuning(MASTER_SEED, tune);
        assert_eq!(built.culture_clock().temperature(), f64::from(0.1_f32));
    }

    #[test]
    fn regen_entries_leave_a_regeneration_pending() {
        let mut game = Game::new(MASTER_SEED);
        assert!(!game.tuning_needs_regen());
        game.tune_set("shield_recharge_delay", 3.0).unwrap();
        assert!(!game.tuning_needs_regen());
        let applied = game.tune_set("relic_one_in", 20.0).unwrap();
        assert!(applied.regen);
        assert!(game.tuning_needs_regen());
        // A world generated under the tuning is not pending.
        let mut tune = Tunables::default();
        tune.set("relic_one_in", 20.0).unwrap();
        assert!(!Game::with_tuning(MASTER_SEED, tune).tuning_needs_regen());
    }

    #[test]
    fn live_entries_take_effect_on_the_next_tick() {
        let mut game = Game::new(MASTER_SEED);
        let before = game.cargo.extra;
        game.loadout.skills.raise(skills::Skill::Cargo);
        game.refresh_stats();
        let one = game.cargo.extra;
        assert!(one > before);
        game.tune_set("cargo_step", 100.0).unwrap();
        assert_eq!(game.cargo.extra, 2.0 * one);
    }

    #[test]
    fn the_api_lists_gets_and_resets() {
        let mut game = Game::new(MASTER_SEED);
        assert_eq!(game.tune_get("adapt_max"), Some(0.55));
        assert_eq!(game.tune_get("nope"), None);
        game.tune_set("adapt_max", 0.4).unwrap();
        let rows = game.tune_list(Some("sniping"));
        assert!(!rows.is_empty() && rows.iter().all(|r| r.group == "sniping"));
        let row = rows.iter().find(|r| r.name == "adapt_max").unwrap();
        assert_eq!((row.value, row.default, row.modified), (0.4, 0.55, true));
        assert_eq!(game.tune_list(None).len(), Tunables::COUNT);
        assert_eq!(
            game.tune_list(None).iter().filter(|r| r.modified).count(),
            1
        );
        game.tune_reset_all();
        assert!(!game.tuning_modified());
        assert!(Game::tune_groups().contains(&"diplomacy"));
    }

    #[test]
    fn overrides_files_apply_what_is_valid_and_report_the_rest() {
        let mut game = Game::new(MASTER_SEED);
        let report = game.tune_load_overrides(
            r#"{ "adapt_max": 0.4, "no_such": 1.0, "dash_cost": -5.0, "rest_cap": 30.0 }"#,
        );
        assert_eq!(report.applied.len(), 2, "{report:?}");
        assert_eq!(report.problems.len(), 2, "{report:?}");
        assert_eq!(game.tune.adapt_max, 0.4);
        assert_eq!(game.tune.rest_cap, 30.0);
        assert_eq!(game.tune.dash_cost, Tunables::DEFAULT.dash_cost);
        // The text of what is set round-trips.
        let text = game.tune_overrides_text();
        let (back, report) = Tunables::from_overrides(&text);
        assert!(report.ok(), "{report:?}");
        assert_eq!(back, game.tune);
    }

    #[test]
    fn the_command_line_answers_everything() {
        let mut game = Game::new(MASTER_SEED);
        assert!(
            game.tune_command("set adapt_max 0.4")
                .starts_with("adapt_max = 0.4")
        );
        assert_eq!(game.tune.adapt_max, 0.4);
        assert!(game.tune_command("/get adapt_max").contains("default 0.55"));
        assert!(game.tune_command("set adapt_max 7").contains("adjusted"));
        assert_eq!(game.tune.adapt_max, 0.95);
        assert!(game.tune_command("set adapt_max nan").starts_with("error:"));
        assert!(game.tune_command("set adapt_max abc").starts_with("error:"));
        assert!(game.tune_command("set nope 1").contains("unknown tunable"));
        assert!(game.tune_command("set").starts_with("error:"));
        assert!(game.tune_command("").starts_with("error:"));
        assert!(game.tune_command("frobnicate").starts_with("error:"));
        assert!(game.tune_command("list modified").contains("adapt_max"));
        assert!(game.tune_command("list nogroup").starts_with("error:"));
        assert!(game.tune_command("list sniping").contains("falloff_span"));
        assert!(game.tune_command("groups").contains("diplomacy"));
        assert!(
            game.tune_command("set relic_one_in 20")
                .contains("needs regen")
        );
        assert!(game.tune_command("regen").contains("pending"));
        assert!(
            game.tune_command("reset adapt_max")
                .starts_with("adapt_max = 0.55")
        );
        assert_eq!(
            game.tune_command("reset all"),
            "all tunables reset to their defaults"
        );
        assert!(!game.tuning_modified());
        assert!(game.tune_command("help").contains("commands"));
        // A rule violation is a refused reply, and nothing changed.
        let reply = game.tune_command("set impact_min_speed 5000");
        assert!(
            reply.starts_with("error:") && reply.contains("impact_speed_cap"),
            "{reply}"
        );
        assert!(!game.tuning_modified());
    }
}
