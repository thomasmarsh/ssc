//! Saved cultural authority and contact estimates; relationship history is separate.
use super::*;
use crate::culture::{self, Clock, Origin, Profile};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Actor {
    origin: Origin,
    epoch: u64,
    estimate: Option<Estimate>,
    reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Estimate {
    emphasis: usize,
    phase: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Societies {
    clock: Clock,
    actors: BTreeMap<u64, Actor>,
}

impl Societies {
    pub(super) fn register(&mut self, t: Territory) {
        self.actors.entry(t.id).or_insert(Actor {
            origin: Origin::new(t.id, t.capital),
            epoch: 0,
            estimate: None,
            reason: None,
        });
    }
    pub(super) fn advance(&mut self, dt: f32) {
        self.clock.advance(f64::from(dt));
    }
    pub(super) fn restore(mut self, keep: bool) -> Self {
        if !self.clock.valid() {
            self.clock = Clock::default();
        }
        if !keep {
            self.actors.clear();
        }
        self.actors.retain(|id, a| {
            a.origin.actor == *id
                && a.origin.generator == culture::PROFILE_VERSION
                && a.estimate.is_none_or(|e| {
                    e.emphasis < culture::VALUES.len() && e.phase.is_finite() && e.phase >= 0.0
                })
        });
        self
    }
}

/// An explicitly uncertain contact assessment, frozen until another actual contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CultureReading {
    pub tendency: &'static str,
    pub sampled_phase: f64,
    pub last_response: Option<&'static str>,
}

impl Game {
    pub fn culture_clock(&self) -> &Clock {
        &self.societies.clock
    }

    pub fn culture_modified(&self) -> bool {
        self.societies.clock != Clock::default()
    }

    /// Direct headless playtest controls. Callers see effective clamped values in the clock.
    /// Saves retain these controls; the desktop adapter never applies environment overrides.
    pub fn configure_culture_drift(&mut self, temperature: f64, timescale: f64) -> bool {
        self.societies.clock.configure(temperature, timescale)
    }

    pub fn civilization_profile(&self, actor: u64) -> Option<Profile> {
        let origin = self.societies.actors.get(&actor)?.origin;
        Some(culture::profile(
            self.seed,
            origin,
            self.societies.clock.phase(),
        ))
    }

    pub fn culture_reading(&self, actor: u64) -> Option<CultureReading> {
        let record = self.societies.actors.get(&actor)?;
        let e = record.estimate?;
        Some(CultureReading {
            tendency: culture::VALUES[e.emphasis],
            sampled_phase: e.phase,
            last_response: culture::VALUES
                .into_iter()
                .chain([
                    "cost",
                    "risk",
                    "uncertainty",
                    "known outcomes",
                    "legacy barter",
                ])
                .find(|label| Some(*label) == record.reason.as_deref()),
        })
    }

    pub(super) fn assess_culture(&mut self, actor: u64) {
        let Some(profile) = self.civilization_profile(actor) else {
            return;
        };
        let emphasis = profile
            .values
            .iter()
            .enumerate()
            .max_by(|(ia, a), (ib, b)| a.total_cmp(b).then_with(|| ib.cmp(ia)))
            .map_or(0, |(i, _)| i);
        if let Some(record) = self.societies.actors.get_mut(&actor) {
            record.estimate = Some(Estimate {
                emphasis,
                phase: self.societies.clock.phase(),
            });
        }
    }

    pub(super) fn society_choose(
        &mut self,
        actor: u64,
        candidates: &[culture::Candidate],
    ) -> Option<culture::Decision> {
        let profile = self.civilization_profile(actor)?;
        let record = self.societies.actors.get_mut(&actor)?;
        let decision = culture::choose(self.seed, actor, record.epoch, profile, candidates)?;
        record.epoch = record.epoch.saturating_add(1);
        record.reason = Some(decision.reason.into());
        Some(decision)
    }

    pub(super) fn contact_culture_text(&self, actor: u64) -> String {
        let Some(reading) = self.culture_reading(actor) else {
            return "Culture unknown.".into();
        };
        let mut text = format!("Tends toward {} (contact estimate).", reading.tendency);
        if let Some(reason) = self
            .societies
            .actors
            .get(&actor)
            .and_then(|a| a.reason.as_deref())
        {
            text.push_str(&format!(" Last response: {reason}."));
        }
        text
    }

    pub(super) fn society_legacy_response(&mut self, actor: u64) {
        if let Some(record) = self.societies.actors.get_mut(&actor) {
            record.epoch = record.epoch.saturating_add(1);
            record.reason = Some("legacy barter".into());
        }
    }

    /// Bounded screenshot pose using the actual friendly seat and tithe action.
    pub fn pose_contact_culture(&mut self) {
        let Some(actor) = self.pad.contact else {
            return;
        };
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.health = ship.max_health * 0.4;
        }
        self.farm.granary.insert(actor, farm::GRANARY_CAP);
        self.cargo.biomass = 0.0;
        self.bench_select(BenchAction::Tithe);
        self.bench_confirm();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sectormap::GENERATOR_VERSION;
    use crate::territory::outpost;

    #[test]
    fn culture_survives_contact_unload_save_and_rewarming_without_discovery_order_effects() {
        let mut game = Game::new(42);
        let t = outpost(42);
        game.register_territory(t);
        let original = game.civilization_profile(t.id).unwrap();
        assert!(game.culture_reading(t.id).is_none());
        game.assess_culture(t.id);
        let reading = game.culture_reading(t.id);
        game.societies.advance(1e8);
        game.shift_regard(t.id, -90.0);
        assert_eq!(game.civilization_profile(t.id), Some(original));
        assert_eq!(game.civ_tier(t.id), Tier::Hostile);
        game.configure_culture_drift(0.5, 1000.0);
        game.societies.advance(100.0);
        let warm = game.civilization_profile(t.id).unwrap();
        assert_ne!(original, warm);
        assert_eq!(game.culture_reading(t.id), reading);
        game.configure_culture_drift(0.0, 2000.0);
        game.teleport(Vec2::new(1e6, 1e6));
        game.step(0.01, Input::default());
        let text = game.save_state().to_text();
        let (state, generator) = save::SaveState::from_text(&text).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.culture_clock(), game.culture_clock());
        assert_eq!(loaded.civilization_profile(t.id), Some(warm));
        assert_eq!(loaded.culture_reading(t.id), reading);
        loaded.societies.advance(1e8);
        assert_eq!(loaded.civilization_profile(t.id), Some(warm));
        loaded.configure_culture_drift(0.5, 2000.0);
        loaded.societies.advance(100.0);
        let mut discovered_late = Game::new(42);
        discovered_late.societies.clock = loaded.societies.clock.clone();
        discovered_late.register_territory(t);
        assert_eq!(
            loaded.civilization_profile(t.id),
            discovered_late.civilization_profile(t.id)
        );
        let (reset, report) = Game::from_save(game.save_state(), GENERATOR_VERSION + 1);
        assert!(!report.world_deltas_kept);
        assert!(reset.culture_reading(t.id).is_none());
        assert_eq!(reset.culture_clock(), game.culture_clock());
    }
}
