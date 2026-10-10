//! Coherent society values and normalized, observer-known decision outcomes.
use crate::world::{SectorId, hash2};
use serde::{Deserialize, Serialize};

pub const PROFILE_VERSION: u32 = 1;
pub const DEFAULT_TIMESCALE: f64 = 86_400.0;
const FIELD_SALT: u64 = 0xC017_0000_0000_0001;
const DECISION_SALT: u64 = 0xDEC1_0000_0000_0001;

/// Values concern the acting society and its actual ties, not every foreign actor.
pub const VALUES: [&str; 7] = [
    "security",
    "prosperity",
    "autonomy",
    "expansion",
    "reliability",
    "solidarity",
    "habitat",
];

/// Stable founding identity, separate from similarity of cultural coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub actor: u64,
    pub anchor: SectorId,
    pub generator: u32,
}

impl Origin {
    pub fn new(actor: u64, anchor: SectorId) -> Self {
        Self {
            actor,
            anchor,
            generator: PROFILE_VERSION,
        }
    }
}

/// All coordinates are bounded 0..1. Response axes affect scoring, not permissions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub values: [f64; 7],
    pub risk_tolerance: f64,
    pub patience: f64,
    pub fidelity: f64,
}

/// Gradient Perlin noise in three dimensions, with independent seed/channel salts.
fn perlin(seed: u64, channel: u64, p: [f64; 3]) -> f64 {
    let floor = p.map(f64::floor);
    let cell = floor.map(|v| v.rem_euclid(4_294_967_296.0) as u32 as i32);
    let local = std::array::from_fn::<_, 3, _>(|i| p[i] - floor[i]);
    let fade = local.map(|v| v * v * v * (v * (v * 6.0 - 15.0) + 10.0));
    let mut sum = 0.0;
    for x in 0..2 {
        for y in 0..2 {
            for z in 0..2 {
                let h = hash2(
                    hash2(
                        seed ^ channel,
                        cell[0].wrapping_add(x),
                        cell[1].wrapping_add(y),
                    ),
                    cell[2].wrapping_add(z),
                    0,
                );
                let d = [
                    local[0] - f64::from(x),
                    local[1] - f64::from(y),
                    local[2] - f64::from(z),
                ];
                // Twelve edge directions, all of length sqrt(2).
                let omit = (h % 3) as usize;
                let a = (omit + 1) % 3;
                let b = (omit + 2) % 3;
                let dot =
                    if h & 4 == 0 { d[a] } else { -d[a] } + if h & 8 == 0 { d[b] } else { -d[b] };
                let weight = [x, y, z]
                    .into_iter()
                    .enumerate()
                    .map(
                        |(i, corner)| {
                            if corner == 0 { 1.0 - fade[i] } else { fade[i] }
                        },
                    )
                    .product::<f64>();
                sum += dot * weight;
            }
        }
    }
    sum.clamp(-1.0, 1.0)
}

pub fn profile(seed: u64, origin: Origin, phase: f64) -> Profile {
    let p = [
        f64::from(origin.anchor.x) / 28.0 + 0.37,
        f64::from(origin.anchor.y) / 28.0 + 0.61,
        phase + 0.23,
    ];
    let fields: [f64; 10] = std::array::from_fn(|i| {
        perlin(
            seed ^ FIELD_SALT ^ u64::from(origin.generator),
            i as u64 * 0x9E37 + 1,
            p,
        )
    });
    let values = std::array::from_fn(|i| {
        (0.5 + 0.65 * (0.8 * fields[i] + 0.2 * fields[(i + 1) % 7])).clamp(0.0, 1.0)
    });
    Profile {
        values,
        risk_tolerance: (0.5 + fields[7] * 0.65).clamp(0.0, 1.0),
        patience: (0.5 + fields[8] * 0.65).clamp(0.0, 1.0),
        // Continuous small tail; most choices have almost no permitted regret.
        fidelity: 1.0 - 0.15 * (0.5 + fields[9] * 0.5).clamp(0.0, 1.0).powi(6),
    }
}

/// Shared universe clock. Private controls ensure invalid inputs cannot change authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    temperature: f64,
    timescale: f64,
    phase: f64,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            temperature: 0.0,
            timescale: DEFAULT_TIMESCALE,
            phase: 0.0,
        }
    }
}

impl Clock {
    pub fn temperature(&self) -> f64 {
        self.temperature
    }
    pub fn timescale(&self) -> f64 {
        self.timescale
    }
    pub fn phase(&self) -> f64 {
        self.phase
    }
    pub fn configure(&mut self, temperature: f64, timescale: f64) -> bool {
        if !temperature.is_finite() || !timescale.is_finite() || timescale <= 0.0 {
            return false;
        }
        self.temperature = temperature.clamp(0.0, 1.0);
        self.timescale = timescale;
        true
    }
    pub fn advance(&mut self, dt: f64) {
        if dt.is_finite() && dt > 0.0 && self.temperature > 0.0 {
            let next = self.phase + self.temperature * dt / self.timescale;
            if next.is_finite() {
                self.phase = next;
            }
        }
    }
    pub(crate) fn valid(&self) -> bool {
        self.temperature.is_finite()
            && (0.0..=1.0).contains(&self.temperature)
            && self.timescale.is_finite()
            && self.timescale > 0.0
            && self.phase.is_finite()
            && self.phase >= 0.0
    }
}

/// None means unknown, never a forecast benefit. Feasibility belongs to real rule queries.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub action: u32,
    pub feasible: bool,
    pub outcomes: [Option<f64>; 7],
    pub delayed: f64,
    pub risk: f64,
    pub uncertainty: f64,
    pub cost: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decision {
    pub action: u32,
    pub score: f64,
    pub regret: f64,
    pub reason: &'static str,
    pub unknown: usize,
}

/// No per-frame randomness: imperfection is stable for an actor/action/decision epoch.
pub fn choose(
    seed: u64,
    actor: u64,
    epoch: u64,
    profile: Profile,
    candidates: &[Candidate],
) -> Option<Decision> {
    let scores: Vec<_> = candidates
        .iter()
        .filter_map(|c| {
            if !c.feasible
                || c.outcomes
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || !(-1.0..=1.0).contains(v))
                || [c.delayed, c.risk, c.uncertainty, c.cost]
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return None;
            }
            let mut terms = [0.0; 7];
            for (i, term) in terms.iter_mut().enumerate() {
                *term = profile.values[i]
                    * c.outcomes[i].unwrap_or(0.0)
                    * (1.0 - c.delayed * (1.0 - profile.patience) * 0.5);
            }
            // Security gained by expansion has a modest additional value; it grants no claim.
            let interaction = 0.1
                * profile.values[0]
                * profile.values[3]
                * c.outcomes[0].unwrap_or(0.0).max(0.0)
                * c.outcomes[3].unwrap_or(0.0).max(0.0);
            let penalties = [
                c.cost,
                c.risk * (1.0 - profile.risk_tolerance),
                c.uncertainty * 0.5,
            ];
            let score = terms.iter().sum::<f64>() + interaction - penalties.iter().sum::<f64>();
            let mut cause = ("known outcomes", 0.0);
            for (i, term) in terms.iter().enumerate() {
                if term.abs() > cause.1 {
                    cause = (VALUES[i], term.abs());
                }
            }
            for (label, value) in ["cost", "risk", "uncertainty"].into_iter().zip(penalties) {
                if value > cause.1 {
                    cause = (label, value);
                }
            }
            Some((c, score, cause.0))
        })
        .collect();
    let best = scores
        .iter()
        .map(|(_, score, _)| *score)
        .max_by(f64::total_cmp)?;
    let bound = 0.04 * (1.0 - profile.fidelity);
    scores
        .iter()
        .filter(|(_, score, _)| best - score <= bound)
        .map(|(c, score, reason)| {
            let h = hash2(seed ^ DECISION_SALT ^ actor, c.action as i32, 0);
            let jitter = perlin(h, epoch, [0.37, 0.61, 0.23]) * bound;
            (
                Decision {
                    action: c.action,
                    score: *score,
                    regret: best - score,
                    reason,
                    unknown: c.outcomes.iter().filter(|v| v.is_none()).count(),
                },
                score + jitter,
            )
        })
        .max_by(|(a, sa), (b, sb)| sa.total_cmp(sb).then_with(|| b.action.cmp(&a.action)))
        .map(|(decision, _)| decision)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_are_coherent_bounded_and_independent_of_actor_identity() {
        let origin = Origin::new(1, SectorId { x: 17, y: -9 });
        let a = profile(42, origin, 0.0);
        let b = profile(42, Origin::new(2, SectorId { x: 18, y: -9 }), 0.0);
        assert_ne!(a.values, b.values);
        assert!(
            a.values
                .iter()
                .zip(b.values)
                .all(|(x, y)| (x - y).abs() < 0.08)
        );
        assert_eq!(a, profile(42, Origin::new(2, origin.anchor), 0.0));
        assert_ne!(a, profile(43, origin, 0.0));
        assert!(a.values.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(a.values.windows(2).any(|w| w[0] != w[1]));
        assert!(a.fidelity >= 0.85 && a.fidelity <= 1.0);
    }
    #[test]
    fn clock_freezes_resumes_without_catchup_and_rejects_invalid_controls() {
        let mut clock = Clock::default();
        clock.advance(1e9);
        assert_eq!(clock.phase(), 0.0);
        assert!(clock.configure(0.5, 1000.0));
        clock.advance(100.0);
        assert_eq!(clock.phase(), 0.05);
        assert!(clock.configure(-1.0, 2000.0));
        let frozen = clock.clone();
        clock.advance(1e9);
        assert_eq!(clock, frozen);
        assert!(!clock.configure(f64::NAN, 1.0));
        assert!(!clock.configure(1.0, 0.0));
        assert_eq!(clock, frozen);
        assert!(clock.configure(5.0, 2000.0));
        clock.advance(100.0);
        assert_eq!(clock.phase(), 0.1);
        let mut partitioned = Clock::default();
        partitioned.configure(1.0, 2000.0);
        for _ in 0..200 {
            partitioned.advance(1.0);
        }
        assert!((partitioned.phase() - clock.phase()).abs() < 1e-12);
    }
    #[test]
    fn evaluation_preserves_feasibility_unknowns_and_bounded_regret() {
        let p = profile(42, Origin::new(1, SectorId::ORIGIN), 0.0);
        let mut a = Candidate {
            action: 1,
            feasible: true,
            outcomes: [None; 7],
            delayed: 0.0,
            risk: 0.0,
            uncertainty: 0.0,
            cost: 0.0,
        };
        a.outcomes[0] = Some(0.7);
        let mut b = a;
        b.action = 2;
        b.outcomes[0] = Some(0.6);
        let mut impossible = a;
        impossible.action = 3;
        impossible.feasible = false;
        impossible.outcomes = [Some(1.0); 7];
        for epoch in 0..100 {
            let d = choose(42, 1, epoch, p, &[a, b, impossible]).unwrap();
            assert_eq!(d.action, 1);
            assert_eq!(d.reason, "security");
            assert_eq!(d.unknown, 6);
            assert!(d.regret <= 0.04 * (1.0 - p.fidelity));
            assert_eq!(Some(d), choose(42, 1, epoch, p, &[impossible, b, a]));
        }
        assert!(choose(42, 1, 0, p, &[impossible]).is_none());
        b.outcomes[0] = Some(f64::NAN);
        assert!(choose(42, 1, 0, p, &[b]).is_none());
    }

    #[test]
    fn slight_imperfection_only_changes_close_choices_and_never_flickers() {
        let mut p = profile(42, Origin::new(1, SectorId::ORIGIN), 0.0);
        p.fidelity = 0.85;
        let a = Candidate {
            action: 1,
            feasible: true,
            outcomes: [None; 7],
            delayed: 0.0,
            risk: 0.0,
            uncertainty: 0.0,
            cost: 0.0,
        };
        let b = Candidate {
            action: 2,
            cost: 0.0001,
            ..a
        };
        let mut imperfect = false;
        for epoch in 0..100 {
            let d = choose(42, 1, epoch, p, &[a, b]).unwrap();
            assert!(d.regret <= 0.006);
            assert_eq!(Some(d), choose(42, 1, epoch, p, &[b, a]));
            imperfect |= d.action == 2;
        }
        assert!(imperfect);
        p.fidelity = 1.0;
        assert_eq!(choose(42, 1, 0, p, &[a, b]).unwrap().action, 1);
    }
}
