//! The ship's rig: upgrades bought at the bench with cargo, owned for the run and never
//! lowered (the same promise as the weapon arsenal). Numbers live in `tuning`.

use super::Material;
use super::tuning as t;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skill {
    /// The beam works faster.
    BeamPower,
    /// The beam reaches farther.
    BeamRange,
    /// More material per unit of ore.
    Yield,
    /// Loose pickups are drawn in from farther.
    Magnet,
    /// A bigger hold.
    Cargo,
}

impl Skill {
    pub const ALL: [Skill; 5] = [
        Self::BeamPower,
        Self::BeamRange,
        Self::Yield,
        Self::Magnet,
        Self::Cargo,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::BeamPower => "BEAM POWER",
            Self::BeamRange => "BEAM RANGE",
            Self::Yield => "ORE YIELD",
            Self::Magnet => "MAGNET",
            Self::Cargo => "CARGO HOLD",
        }
    }

    pub fn max_level(self) -> u8 {
        t::SKILL_MAX
    }

    /// What one level gives, for the bench.
    pub fn summary(self) -> String {
        match self {
            Self::BeamPower => format!("+{:.0}% beam rate", t::POWER_STEP * 100.0),
            Self::BeamRange => format!("+{:.0} reach", t::RANGE_STEP),
            Self::Yield => format!("+{:.0}% per ore", t::YIELD_STEP * 100.0),
            Self::Magnet => format!("+{:.0} pickup pull", t::MAGNET_STEP),
            Self::Cargo => format!("+{:.0} hold each", t::CARGO_STEP),
        }
    }

    fn base_price(self) -> &'static [(Material, f32)] {
        match self {
            Self::BeamPower => &t::PRICE_POWER,
            Self::BeamRange => &t::PRICE_RANGE,
            Self::Yield => &t::PRICE_YIELD,
            Self::Magnet => &t::PRICE_MAGNET,
            Self::Cargo => &t::PRICE_CARGO,
        }
    }

    /// Price of the step from `level` to the next; None at the top.
    pub fn price(self, level: u8) -> Option<Vec<(Material, f32)>> {
        if level >= self.max_level() {
            return None;
        }
        let k = t::PRICE_GROWTH.powi(i32::from(level));
        Some(
            self.base_price()
                .iter()
                .map(|&(m, a)| (m, (a * k).round()))
                .collect(),
        )
    }
}

/// Levels owned. Zero means not bought.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Skills {
    levels: [u8; Skill::ALL.len()],
}

impl Skills {
    pub fn level(&self, skill: Skill) -> u8 {
        self.levels[skill.index()]
    }

    /// Raises a skill one level (never lowers); the new level, or None at the cap.
    pub fn raise(&mut self, skill: Skill) -> Option<u8> {
        let held = self.level(skill);
        if held >= skill.max_level() {
            return None;
        }
        self.levels[skill.index()] = held + 1;
        Some(held + 1)
    }

    fn steps(&self, skill: Skill) -> f32 {
        f32::from(self.level(skill))
    }

    /// Beam rate multiplier.
    pub fn beam_power(&self) -> f32 {
        1.0 + t::POWER_STEP * self.steps(Skill::BeamPower)
    }

    pub fn beam_range(&self) -> f32 {
        t::BEAM_RANGE + t::RANGE_STEP * self.steps(Skill::BeamRange)
    }

    /// Material gained per unit of ore worked.
    pub fn yield_mult(&self) -> f32 {
        1.0 + t::YIELD_STEP * self.steps(Skill::Yield)
    }

    pub fn magnet_bonus(&self) -> f32 {
        t::MAGNET_STEP * self.steps(Skill::Magnet)
    }

    pub fn cargo_bonus(&self) -> f32 {
        t::CARGO_STEP * self.steps(Skill::Cargo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_only_go_up_and_stop_at_the_cap() {
        let mut skills = Skills::default();
        for skill in Skill::ALL {
            assert_eq!(skills.level(skill), 0);
            for want in 1..=skill.max_level() {
                assert_eq!(skills.raise(skill), Some(want));
            }
            assert_eq!(skills.raise(skill), None);
            assert_eq!(skills.level(skill), skill.max_level());
        }
    }

    #[test]
    fn effects_grow_with_level_and_prices_climb() {
        let mut skills = Skills::default();
        assert_eq!(skills.beam_power(), 1.0);
        assert_eq!(skills.beam_range(), t::BEAM_RANGE);
        assert_eq!(skills.yield_mult(), 1.0);
        assert_eq!(skills.magnet_bonus(), 0.0);
        assert_eq!(skills.cargo_bonus(), 0.0);
        for skill in Skill::ALL {
            skills.raise(skill);
        }
        assert!(skills.beam_power() > 1.0 && skills.beam_range() > t::BEAM_RANGE);
        assert!(skills.yield_mult() > 1.0 && skills.magnet_bonus() > 0.0);
        assert!(skills.cargo_bonus() > 0.0);
        for skill in Skill::ALL {
            let first: f32 = skill.price(0).unwrap().iter().map(|p| p.1).sum();
            let last: f32 = skill
                .price(skill.max_level() - 1)
                .unwrap()
                .iter()
                .map(|p| p.1)
                .sum();
            assert!(last > first);
            assert!(skill.price(skill.max_level()).is_none());
        }
    }
}
