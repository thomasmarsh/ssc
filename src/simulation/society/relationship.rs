//! Bounded, directly attributed civilization -> player history, independent of opinion/ROE.
use super::*;

/// Fixed cause slots bound storage regardless of the number of projectiles or mining ticks.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Relationship {
    trust: f64,
    claim: f64,
    attack: f64,
    credit_used: f64,
    credit_remaining: f64,
    last_trust: Option<TrustCause>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum TrustCause {
    FulfilledJob,
    PlayerHarm,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelationshipReading {
    pub trust: f64,
    pub friction: f64,
    pub cause: Option<&'static str>,
    pub remedy: Option<&'static str>,
    pub trust_cause: Option<&'static str>,
    /// Opinion summary (a tier, or TENSE): never a targeting or purchase predicate.
    pub opinion: Option<&'static str>,
    /// Posture and its leading reason as of the last contact.
    pub posture: Option<&'static str>,
    pub posture_reason: Option<&'static str>,
    /// A skirmish the player can attribute, and the last resolved operation or war.
    pub operation: Option<&'static str>,
    pub outcome: Option<&'static str>,
}

impl RelationshipReading {
    fn trust_text(self) -> String {
        let mut text = format!("Trust {:+.0} / friction {:.0}", self.trust, self.friction);
        if let Some(cause) = self.cause {
            text.push_str(&format!(" - {cause}; {}", self.remedy.unwrap_or("")));
        } else if let Some(cause) = self.trust_cause {
            text.push_str(&format!(" - {cause}"));
        }
        text
    }

    /// Opinion summary, then the posture known from the last contact with its leading reason.
    fn standing_text(self) -> Option<String> {
        let mut text = format!("Opinion {}", self.opinion?);
        match (self.posture, self.posture_reason) {
            (Some(posture), Some(reason)) => {
                text.push_str(&format!(" / {posture} ({reason}, at last contact)"));
            }
            (Some(posture), None) => text.push_str(&format!(" / {posture} (at last contact)")),
            _ => text.push_str(" / posture unknown"),
        }
        Some(text)
    }

    /// Anything the player can attribute: a live skirmish, else the last resolution.
    fn operation_text(self) -> Option<String> {
        match (self.operation, self.outcome) {
            (Some(operation), _) => Some(format!("{operation} against your drones")),
            (None, Some(outcome)) => Some(format!("Last: {outcome}")),
            _ => None,
        }
    }

    /// Player-visible first-hand facts, never cultural affinity or unmodeled dependency.
    pub fn text(self) -> String {
        let mut text = self.trust_text();
        for extra in [self.standing_text(), self.operation_text()]
            .into_iter()
            .flatten()
        {
            text.push('\n');
            text.push_str(&extra);
        }
        text
    }

    /// The compact CONTACT form: the friendly seat already implies a friendly, peaceful
    /// summary, so the standing clause appears only when something noteworthy changes it.
    pub fn contact_text(self) -> String {
        let mut text = self.trust_text();
        let notable = self.opinion == Some("TENSE")
            || self.posture.is_some_and(|posture| posture != "DEFENSIVE");
        let standing = self.standing_text().filter(|_| notable);
        for extra in [standing, self.operation_text()].into_iter().flatten() {
            text.push('\n');
            text.push_str(&extra);
        }
        text
    }
}

impl Relationship {
    pub(super) fn advance(&mut self, dt: f64, tune: &Tunables) {
        self.claim = (self.claim - dt * f64::from(tune.society_claim_decay)).max(0.0);
        self.attack = (self.attack - dt * f64::from(tune.society_attack_decay)).max(0.0);
        self.credit_remaining = (self.credit_remaining - dt).max(0.0);
        if self.credit_remaining == 0.0 {
            self.credit_used = 0.0;
        }
    }

    pub(super) fn fulfilled(&mut self, tune: &Tunables) {
        if self.credit_remaining == 0.0 {
            self.credit_remaining = f64::from(tune.society_trust_window);
        }
        let gain = f64::from(tune.society_job_trust)
            .min((f64::from(tune.society_trust_gain_cap) - self.credit_used).max(0.0));
        self.credit_used += gain;
        self.trust = (self.trust + gain).min(100.0);
        if gain > 0.0 {
            self.last_trust = Some(TrustCause::FulfilledJob);
        }
    }

    pub(super) fn harm(&mut self, damage: f64, tune: &Tunables) {
        self.trust = (self.trust - damage * f64::from(tune.society_harm_trust)).max(-100.0);
        self.attack = (self.attack + damage * f64::from(tune.society_harm_friction)).min(100.0);
        // Actual attacks take priority over a lesser claim dispute at the shared cap.
        self.claim = self.claim.min(100.0 - self.attack);
        self.last_trust = Some(TrustCause::PlayerHarm);
    }

    pub(super) fn mined(&mut self, amount: f64, tune: &Tunables) {
        if amount.is_finite() && amount > 0.0 {
            self.claim = (self.claim + amount * f64::from(tune.society_mine_friction))
                .min(100.0 - self.attack);
        }
    }

    pub(super) fn reading(&self) -> RelationshipReading {
        let (cause, remedy) = if self.attack > 0.0 {
            (Some("player harm"), Some("cease attacks; allow time"))
        } else if self.claim > 0.0 {
            (Some("claim mining"), Some("stop mining; allow time"))
        } else {
            (None, None)
        };
        RelationshipReading {
            trust: self.trust,
            friction: self.claim + self.attack,
            cause,
            remedy,
            trust_cause: self.last_trust.map(|cause| match cause {
                TrustCause::FulfilledJob => "fulfilled job",
                TrustCause::PlayerHarm => "player harm",
            }),
            opinion: None,
            posture: None,
            posture_reason: None,
            operation: None,
            outcome: None,
        }
    }

    pub(super) fn sanitize(&mut self, tune: &Tunables) {
        fn bounded(value: f64, min: f64, max: f64) -> f64 {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                0.0
            }
        }
        self.trust = bounded(self.trust, -100.0, 100.0);
        self.attack = bounded(self.attack, 0.0, 100.0);
        self.claim = bounded(self.claim, 0.0, 100.0 - self.attack);
        self.credit_used = bounded(self.credit_used, 0.0, 100.0);
        self.credit_remaining = bounded(
            self.credit_remaining,
            0.0,
            f64::from(tune.society_trust_window),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::territory::outpost;

    #[test]
    fn bounded_credit_harm_and_causes_follow_simulation_time_across_reload() {
        let t = outpost(crate::config::MASTER_SEED);
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.register_territory(t);
        for _ in 0..20 {
            game.civil_job_fulfilled(t.id);
        }
        assert_eq!(game.civilization_relationship(t.id).unwrap().trust, 10.0);
        game.civil_claim_mined(t.id, 50.0);
        game.civil_player_harm(t.id, 20.0);
        let reading = game.civilization_relationship(t.id).unwrap();
        assert!((reading.trust - 8.0).abs() < 1e-5);
        assert!((reading.friction - 20.0).abs() < 1e-5);
        assert_eq!(reading.cause, Some("player harm"));
        assert!(!game.civilization_may_attack(t.id, CivilTarget::Fleet));
        let (state, generator) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (mut loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.civilization_relationship(t.id), Some(reading));
        loaded.civil_job_fulfilled(t.id);
        assert_eq!(loaded.civilization_relationship(t.id), Some(reading));
        game.civs.societies.advance(100.0, &game.tune);
        for _ in 0..400 {
            loaded.civs.societies.advance(0.25, &loaded.tune);
        }
        let a = game.civilization_relationship(t.id).unwrap();
        let b = loaded.civilization_relationship(t.id).unwrap();
        assert_eq!(a.trust, b.trust);
        assert!((a.friction - b.friction).abs() < 1e-9);
        assert_eq!(a.cause, b.cause);
        assert_eq!(a.trust, reading.trust); // Grievances fading cannot restore lost trust.
        loaded.teleport(Vec2::ZERO);
        loaded.civs.societies.advance(200.0, &loaded.tune);
        loaded.civil_job_fulfilled(t.id);
        assert!((loaded.civilization_relationship(t.id).unwrap().trust - 13.0).abs() < 1e-5);
        loaded.civs.societies.advance(1000.0, &loaded.tune);
        assert_eq!(
            loaded.civilization_relationship(t.id).unwrap().friction,
            0.0
        );
        let (reset, _) =
            Game::from_save(loaded.save_state(), crate::sectormap::GENERATOR_VERSION + 1);
        assert!(
            reset
                .civilization_relationship(t.id)
                .is_none_or(|r| r.trust == 0.0)
        );
    }

    #[test]
    fn history_cannot_be_farmed_from_visits_gifts_barter_or_opinion() {
        let t = outpost(crate::config::MASTER_SEED);
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.register_territory(t);
        game.shift_regard(t.id, 100.0);
        game.civs.societies.advance(100_000.0, &game.tune);
        game.society_legacy_response(t.id);
        let reading = game.civilization_relationship(t.id).unwrap();
        assert_eq!((reading.trust, reading.friction), (0.0, 0.0));
        assert!(game.civilization_relationship(u64::MAX).is_none());
        game.civil_claim_mined(t.id, 1e9);
        game.civil_player_harm(t.id, 1e9);
        let reading = game.civilization_relationship(t.id).unwrap();
        assert_eq!((reading.trust, reading.friction), (-100.0, 100.0));
        assert_eq!(
            game.civilization_engagement(t.id),
            EngagementRule::SelfDefense
        );
        game.civs.societies.advance(30.0, &game.tune);
        assert_eq!(game.civilization_engagement(t.id), EngagementRule::Peace);
        assert!(game.civilization_relationship(t.id).unwrap().friction > 0.0);
    }
}
