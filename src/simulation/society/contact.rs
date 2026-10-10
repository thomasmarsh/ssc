//! First contact: how a civilization first receives a stranger. The reception is chosen by the
//! shared culture evaluator from the generated profile and one real fact (how strong the
//! stranger looks beside the civilization), then becomes ordinary starting regard. It grants
//! no permission: engagement rules, service gates and trade stay on their own authority.
use super::*;

/// Evaluator stream of the first-contact decision (independent of posture epochs).
const CONTACT_STREAM: u64 = 0x4643_0000_0000;

/// The five receptions, coldest to warmest. The action id is the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reception {
    Wary,
    Cool,
    Neutral,
    Curious,
    Welcoming,
}

impl Reception {
    const ALL: [Self; 5] = [
        Self::Wary,
        Self::Cool,
        Self::Neutral,
        Self::Curious,
        Self::Welcoming,
    ];

    fn from_action(action: u32) -> Self {
        Self::ALL
            .get(action as usize)
            .copied()
            .unwrap_or(Self::Neutral)
    }

    fn offset(self, tune: &Tunables) -> f32 {
        match self {
            Self::Wary => tune.society_contact_wary,
            Self::Cool => tune.society_contact_cool,
            Self::Neutral => 0.0,
            Self::Curious => tune.society_contact_curious,
            Self::Welcoming => tune.society_contact_welcome,
        }
    }
}

/// Short reasons a reception can carry, indexed for the saved note (one per value, then
/// "reserved" for a reception with no leading value).
pub(in crate::simulation) const NOTES: [&str; 8] = [
    "guards its security",
    "wants trade",
    "guards its autonomy",
    "eyes new reach",
    "prizes dependable dealing",
    "open to new ties",
    "protects its habitat",
    "no strong feelings",
];

/// How a ship of `power` looks beside a civilization of `threat`: 0 is no danger at all (the
/// verdict's OUTCLASSED line), 1 is a credible menace (its STRONG line).
fn exposure(power: f32, threat: f32) -> f64 {
    let ratio = f64::from(power / threat.max(0.1).powf(0.8));
    ((ratio - 0.6) / 0.7).clamp(0.0, 1.0)
}

/// Outcome features indexed security, prosperity, autonomy, expansion, reliability, solidarity,
/// habitat: what each reception does for the society's own values. A stranger is a security
/// and autonomy exposure (worse the stronger it looks), a trade chance, a new tie and a risk to
/// its habitat. Magnitudes were fitted so a typical spread of generated profiles opens roughly
/// 11 percent wary, 20 cool, 25 neutral, 24 curious and 19 welcoming for a stranger that
/// poses no danger, and a formidable one meeting 34 percent wary and 33 cool.
pub(super) fn candidates(exposure: f64, settler: bool) -> [culture::Candidate; 5] {
    let seen = 0.5 + exposure;
    let make = |r: Reception, outcomes: [Option<f64>; 7], risk: f64, uncertainty: f64| {
        culture::Candidate {
            action: r as u32,
            // Settlers never rebuff a stranger outright.
            feasible: !(settler && r == Reception::Wary),
            outcomes,
            delayed: 0.0,
            risk,
            uncertainty,
            cost: 0.0,
        }
    };
    [
        make(
            Reception::Wary,
            [
                Some(0.36 * seen),
                Some(-0.85),
                Some(0.89),
                None,
                None,
                Some(-0.65),
                Some(0.15),
            ],
            0.0,
            0.0,
        ),
        make(
            Reception::Cool,
            [
                Some(0.25 * seen),
                Some(-0.99),
                Some(0.25),
                None,
                None,
                None,
                Some(0.45),
            ],
            0.0,
            0.0,
        ),
        make(Reception::Neutral, [None; 7], 0.0, 0.0),
        make(
            Reception::Curious,
            [
                Some(-0.12 * seen),
                Some(0.56),
                None,
                Some(0.77),
                None,
                None,
                None,
            ],
            0.88,
            0.52,
        ),
        make(
            Reception::Welcoming,
            [
                Some(-0.34 * seen),
                Some(0.46),
                Some(-0.75),
                None,
                Some(0.90),
                Some(0.35),
                Some(-0.78),
            ],
            0.14,
            0.16,
        ),
    ]
}

/// A civilization's opening standing toward the ship and the short reason behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::simulation) struct Opening {
    pub(in crate::simulation) start: f32,
    pub(in crate::simulation) note: u8,
}

impl Game {
    /// Chooses how `t` receives the ship now. Pure in the seed, the founding profile, the
    /// civilization and the ship's strength; nothing random per call.
    pub(in crate::simulation) fn first_contact(&self, t: &Territory) -> Opening {
        let base = if t.peaceful() {
            self.tune.regard_start_outpost
        } else {
            self.tune.regard_start
        };
        let neutral = Opening {
            start: base,
            note: (NOTES.len() - 1) as u8,
        };
        let Some(profile) = self.civilization_profile(t.id) else {
            return neutral;
        };
        let depth = world::latent(self.seed, t.capital).depth;
        let threat = world::threat(depth) * t.menace();
        let all = candidates(exposure(self.power(), threat), t.peaceful());
        let Some(decision) = culture::choose(self.seed, t.id, CONTACT_STREAM, profile, &all) else {
            return neutral;
        };
        let reception = Reception::from_action(decision.action);
        // The note is the value this culture holds most strongly among those the chosen
        // reception serves, never a cost.
        let chosen = all[reception as usize];
        let note = chosen
            .outcomes
            .iter()
            .enumerate()
            .filter(|(_, o)| o.is_some_and(|o| o > 0.0))
            .map(|(i, _)| (i, profile.values[i]))
            .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map_or(NOTES.len() - 1, |(i, _)| i);
        Opening {
            start: (base + reception.offset(&self.tune))
                .clamp(self.tune.regard_min, self.tune.regard_max),
            note: note as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MASTER_SEED;
    use crate::world::SectorId;
    use std::collections::{BTreeMap, BTreeSet};

    /// Every distinct civilization whose capital lies within 40 sectors of the origin.
    fn civilizations(seed: u64) -> Vec<Territory> {
        let mut seen = BTreeSet::new();
        let mut all = Vec::new();
        for x in -40..=40 {
            for y in -40..=40 {
                if let Some(t) = world::territory(seed, SectorId { x, y })
                    && seen.insert(t.id)
                {
                    all.push(t);
                }
            }
        }
        all
    }

    /// A game that has first met `t` (no ship movement; the reception only).
    fn met(seed: u64, t: Territory) -> Game {
        let mut game = Game::new(seed);
        game.register_territory(t);
        game.regard_mut(t.id);
        game
    }

    #[test]
    fn receptions_vary_across_civilizations_with_a_reason() {
        let mut labels: BTreeMap<&str, u32> = BTreeMap::new();
        for seed in [MASTER_SEED, 1, 2] {
            let mut game = Game::new(seed);
            for t in civilizations(seed) {
                game.register_territory(t);
                game.regard_mut(t.id);
                let stance = game.civ_stance(t.id);
                *labels.entry(stance.label).or_default() += 1;
                assert!(stance.note.is_some(), "{} has no reason", stance.label);
                let first = game.civ_first_contact(t.id).unwrap();
                assert_eq!(first, stance, "an untouched standing is its first contact");
                if t.peaceful() {
                    assert_ne!(stance.label, "WARY", "settlers never rebuff outright");
                    assert_ne!(game.civ_tier(t.id), Tier::Wary);
                }
                // Never opens FRIENDLY, and grants nothing by itself.
                assert_ne!(game.civ_tier(t.id), Tier::Friendly, "{}", t.id);
                assert!(!game.civilization_service_allowed(t.id));
                assert!(!game.civilization_may_attack(t.id, CivilTarget::Ship));
                assert!(game.civ_regard(t.id).abs() <= 60.0);
            }
        }
        for label in ["WARY", "COOL", "NEUTRAL", "CURIOUS", "WELCOMING"] {
            assert!(
                labels.get(label).is_some_and(|n| *n >= 3),
                "{label}: {labels:?}"
            );
        }
        let wary = labels["WARY"] + labels["COOL"];
        let warm = labels["CURIOUS"] + labels["WELCOMING"];
        assert!(wary > 0 && warm > 0);
    }

    #[test]
    fn a_reception_is_deterministic_saved_and_not_redone_by_a_stronger_ship() {
        let ordinary = civilizations(MASTER_SEED)
            .into_iter()
            .find(|t| !t.peaceful())
            .unwrap();
        let mut game = met(MASTER_SEED, ordinary);
        let again = met(MASTER_SEED, ordinary);
        assert_eq!(game.civ_stance(ordinary.id), again.civ_stance(ordinary.id));
        assert_eq!(game.civ_regard(ordinary.id), again.civ_regard(ordinary.id));
        let before = (game.civ_stance(ordinary.id), game.civ_regard(ordinary.id));
        // A much stronger ship later does not reopen the introduction.
        game.collect(Item::Part(upgrades::Part {
            name: "Test Plate".into(),
            slot: upgrades::Slot::Plating,
            rarity: upgrades::Rarity::Rare,
            grade: 1.0,
            stem: String::new(),
            core: usize::MAX,
            effects: vec![upgrades::Effect::Stat(upgrades::Stat::Hull, 4.0)],
        }));
        assert!(game.power() > 1.5);
        game.regard_mut(ordinary.id);
        assert_eq!(
            (game.civ_stance(ordinary.id), game.civ_regard(ordinary.id)),
            before
        );
        let (state, generator) = save::SaveState::from_text(&game.save_state().to_text()).unwrap();
        let (loaded, _) = Game::from_save(state, generator);
        assert_eq!(loaded.civ_stance(ordinary.id), before.0);
        assert_eq!(loaded.civ_regard(ordinary.id), before.1);
        assert_eq!(
            loaded.civ_first_contact(ordinary.id),
            game.civ_first_contact(ordinary.id)
        );
    }

    #[test]
    fn a_formidable_stranger_is_received_colder_than_a_harmless_one() {
        // The same profiles, the same candidates: only the exposure fact differs.
        let (mut cold_harmless, mut cold_formidable) = (0, 0);
        let mut total = 0;
        for seed in 1..=4u64 {
            for x in (-40..=40).step_by(2) {
                for y in (-40..=40).step_by(2) {
                    let origin = Origin::new((x * 100 + y) as u64, SectorId { x, y });
                    let profile = culture::profile(seed, origin, 0.0);
                    let cold = |exposure: f64| {
                        let all = candidates(exposure, false);
                        let d = culture::choose(seed, origin.actor, CONTACT_STREAM, profile, &all)
                            .unwrap();
                        Reception::from_action(d.action) <= Reception::Cool
                    };
                    cold_harmless += u32::from(cold(0.0));
                    cold_formidable += u32::from(cold(1.0));
                    total += 1;
                }
            }
        }
        assert!(
            cold_formidable > cold_harmless + total / 10,
            "{cold_harmless} {cold_formidable}"
        );
        assert_eq!(exposure(0.1, 4.0), 0.0);
        assert_eq!(exposure(50.0, 1.0), 1.0);
    }

    #[test]
    fn the_reason_holds_only_while_the_standing_still_reads_as_it_opened() {
        let ordinary = civilizations(MASTER_SEED)
            .into_iter()
            .find(|t| !t.peaceful())
            .unwrap();
        let mut game = met(MASTER_SEED, ordinary);
        let opened = game.civ_stance(ordinary.id);
        assert!(opened.note.is_some());
        game.shift_regard(ordinary.id, -30.0);
        let hurt = game.civ_stance(ordinary.id);
        assert_ne!(hurt.label, opened.label);
        assert_eq!(
            hurt.note, None,
            "an old reason must not explain a new grievance"
        );
        // The record of how it began stays.
        assert_eq!(game.civ_first_contact(ordinary.id).unwrap(), opened);
    }

    #[test]
    fn the_reception_reaches_the_contact_text_the_hud_report_and_the_chart() {
        let seed = MASTER_SEED;
        let ordinary = civilizations(seed)
            .into_iter()
            .find(|t| !t.peaceful())
            .unwrap();
        let game = met(seed, ordinary);
        let stance = game.civ_stance(ordinary.id);
        let text = game.contact_culture_text(ordinary.id);
        assert!(
            text.contains(&format!("First contact: {}", stance.label)),
            "{text}"
        );
        assert!(text.contains(stance.note.unwrap()), "{text}");
        let reading = game.civilization_relationship(ordinary.id).unwrap();
        assert!(reading.opinion == Some(stance.label) || reading.opinion == Some("TENSE"));
    }

    #[test]
    fn outposts_open_warmer_than_ordinary_civilizations_but_read_the_same_ladder() {
        let o = crate::territory::outpost(MASTER_SEED);
        let game = met(MASTER_SEED, o);
        let tune = game.tune;
        let regard = game.civ_regard(o.id);
        assert!(regard >= tune.regard_start_outpost + tune.society_contact_cool);
        assert!(regard <= tune.regard_start_outpost + tune.society_contact_welcome);
        let lift = regard - tune.regard_start_outpost;
        let label = game.civ_stance(o.id).label;
        let expect = if lift < tune.stance_cool_below {
            "COOL"
        } else if lift < tune.stance_curious_from {
            "NEUTRAL"
        } else if lift < tune.stance_welcome_from {
            "CURIOUS"
        } else {
            "WELCOMING"
        };
        assert_eq!(label, expect);
    }
}
