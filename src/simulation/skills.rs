//! The ship's rig: upgrades bought at the bench with cargo, owned for the run and never
//! lowered (the same promise as the weapon arsenal). Numbers live in `tuning`.

use super::Material;
use super::tuning as t;
use super::upgrades::{Rarity, Slot};

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
    /// A forward arc shield that turns hostile shots aside. Locked until bought.
    Parry,
    /// A quick short jump that stops at the first obstruction. Locked until bought.
    Dash,
    /// Sonar: the ring reaches farther.
    PingReach,
    /// Sonar: the ring sweeps faster.
    PingSpeed,
    /// Sonar: a shorter cooldown.
    PingCooldown,
    /// Sonar: more echoes of every kind.
    PingTargets,
    /// Sonar tier: pads the enemy has found answer as alerts. Locked until bought.
    EchoPads,
    /// Sonar tier: rich mining spots and renewable planetoids. Locked until bought.
    EchoLodes,
    /// Sonar tier: nests and egg clusters. Locked until bought.
    EchoNests,
    /// Sonar tier: predator density of a sector. Locked until bought.
    EchoPredators,
    /// A deployable beacon, one more standing per level; fast travel needs one. Locked until
    /// bought.
    Beacon,
    /// Ramming a free rock (or any free body) hits harder, the beam's grip is stronger and a
    /// dash cracks like a whip. Locked until bought.
    Shove,
    /// The ship takes less of the impacts it causes (and later of every collision). Needs
    /// some SHOVE to buy. Locked until bought.
    ShovePlating,
    /// Organ slots, one a level; needs a Rare or better core fitted. Locked until bought.
    Symbiosis,
}

/// Which bench tab sells a skill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillTab {
    Rig,
    Sonar,
}

impl Skill {
    pub const ALL: [Skill; 19] = [
        Self::BeamPower,
        Self::BeamRange,
        Self::Yield,
        Self::Magnet,
        Self::Cargo,
        Self::Parry,
        Self::Dash,
        Self::PingReach,
        Self::PingSpeed,
        Self::PingCooldown,
        Self::PingTargets,
        Self::EchoPads,
        Self::EchoLodes,
        Self::EchoNests,
        Self::EchoPredators,
        Self::Beacon,
        Self::Shove,
        Self::ShovePlating,
        Self::Symbiosis,
    ];

    /// The bench tab that sells this skill.
    pub fn tab(self) -> SkillTab {
        match self {
            Self::PingReach
            | Self::PingSpeed
            | Self::PingCooldown
            | Self::PingTargets
            | Self::EchoPads
            | Self::EchoLodes
            | Self::EchoNests
            | Self::EchoPredators => SkillTab::Sonar,
            _ => SkillTab::Rig,
        }
    }

    /// The skills of one bench tab, in order.
    pub fn of_tab(tab: SkillTab) -> Vec<Skill> {
        Self::ALL.into_iter().filter(|s| s.tab() == tab).collect()
    }

    /// Skills that do nothing at level zero and are bought once or climbed from a first
    /// purchase (as opposed to mining upgrades, which also work at level zero).
    pub fn starts_locked(self) -> bool {
        self.is_ability()
            || matches!(
                self,
                Self::EchoPads
                    | Self::EchoLodes
                    | Self::EchoNests
                    | Self::EchoPredators
                    | Self::Beacon
                    | Self::Shove
                    | Self::ShovePlating
                    | Self::Symbiosis
            )
    }

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
            Self::Parry => "PARRY",
            Self::Dash => "DASH",
            Self::PingReach => "PING REACH",
            Self::PingSpeed => "PING SPEED",
            Self::PingCooldown => "PING RECHARGE",
            Self::PingTargets => "PING TARGETS",
            Self::EchoPads => "PAD WATCH",
            Self::EchoLodes => "LODE ECHO",
            Self::EchoNests => "NEST ECHO",
            Self::EchoPredators => "PREDATOR ECHO",
            Self::Beacon => "BEACON",
            Self::Shove => "SHOVE",
            Self::ShovePlating => "PLATING",
            Self::Symbiosis => "SYMBIOSIS",
        }
    }

    pub fn max_level(self) -> u8 {
        match self {
            Self::EchoPads | Self::EchoLodes | Self::EchoNests | Self::EchoPredators => 1,
            Self::Symbiosis => t::SYMBIOSIS_SLOTS as u8,
            _ => t::SKILL_MAX,
        }
    }

    /// What one level gives, for the bench.
    pub fn summary(self) -> String {
        match self {
            Self::BeamPower => format!("+{:.0}% beam rate", t::POWER_STEP * 100.0),
            Self::BeamRange => format!("+{:.0} reach", t::RANGE_STEP),
            Self::Yield => format!("+{:.0}% per ore", t::YIELD_STEP * 100.0),
            Self::Magnet => format!("+{:.0} pickup pull", t::MAGNET_STEP),
            Self::Cargo => format!("+{:.0} hold each", t::CARGO_STEP),
            Self::Dash => format!(
                "{:.0} unit jump, brief invulnerability; dashing through fire or a flinger boosts damage (+{:.0} a level)",
                t::DASH_DISTANCE,
                t::DASH_DISTANCE_STEP
            ),
            Self::Parry => format!(
                "arc shield, {:.0}% block, perfect timing reflects (+{:.0}% block, longer perfect window, harder reflect a level)",
                t::PARRY_CHANCE * 100.0,
                t::PARRY_CHANCE_STEP * 100.0
            ),
            Self::PingReach => format!("+{:.0} reach", t::PING_REACH_STEP),
            Self::PingSpeed => format!("+{:.0} ring speed", t::PING_SPEED_STEP),
            Self::PingCooldown => format!("-{:.1}s recharge", t::PING_COOLDOWN_STEP),
            Self::PingTargets => format!("+{} echo of every kind", t::PING_TARGETS_STEP),
            Self::EchoPads => "pads the enemy has found ping as alerts".to_string(),
            Self::EchoLodes => "rich lodes and renewable planetoids".to_string(),
            Self::EchoNests => "nests and egg clusters".to_string(),
            Self::EchoPredators => "how many predators roam a sector".to_string(),
            Self::Shove => format!(
                "ram a held rock to fling it: +{:.0}% push, longer grip, bigger dash whip",
                t::SHOVE_MULT_STEP * 100.0
            ),
            Self::ShovePlating => format!(
                "-{:.0}% damage from your own rams; level {}+ cuts every collision",
                t::PLATING_CAUSED_STEP * 100.0,
                t::PLATING_ALL_FROM
            ),
            Self::Symbiosis => format!(
                "one organ slot a level; grafts cost crystal and volatiles, upkeep {:.1} volatiles a minute",
                t::ORGAN_UPKEEP
            ),
            Self::Beacon => format!(
                "deploy beacons (H) and jump back to one from the chart; {} more standing a level",
                t::BEACONS_PER_LEVEL
            ),
        }
    }

    fn base_price(self) -> &'static [(Material, f32)] {
        match self {
            Self::BeamPower => &t::PRICE_POWER,
            Self::BeamRange => &t::PRICE_RANGE,
            Self::Yield => &t::PRICE_YIELD,
            Self::Magnet => &t::PRICE_MAGNET,
            Self::Cargo => &t::PRICE_CARGO,
            Self::Parry => &t::PRICE_PARRY,
            Self::Dash => &t::PRICE_DASH,
            Self::PingReach => &t::PRICE_PING_REACH,
            Self::PingSpeed => &t::PRICE_PING_SPEED,
            Self::PingCooldown => &t::PRICE_PING_COOLDOWN,
            Self::PingTargets => &t::PRICE_PING_TARGETS,
            Self::EchoPads => &t::PRICE_ECHO_PADS,
            Self::EchoLodes => &t::PRICE_ECHO_LODES,
            Self::EchoNests => &t::PRICE_ECHO_NESTS,
            Self::EchoPredators => &t::PRICE_ECHO_PREDATORS,
            Self::Beacon => &t::PRICE_BEACON,
            Self::Shove => &t::PRICE_SHOVE,
            Self::ShovePlating => &t::PRICE_SHOVE_PLATING,
            Self::Symbiosis => &t::PRICE_SYMBIOSIS,
        }
    }

    /// Whether this is a locked upgrade: level zero means the ship cannot do it at all.
    pub fn is_ability(self) -> bool {
        matches!(self, Self::Parry | Self::Dash)
    }

    /// The part the ship must already carry before the first purchase.
    pub fn requirement(self) -> Option<(Slot, Rarity)> {
        match self {
            Self::Parry => Some((Slot::Plating, Rarity::Rare)),
            Self::Dash => Some((Slot::Engine, Rarity::Rare)),
            Self::Symbiosis => Some((Slot::Core, Rarity::Rare)),
            _ => None,
        }
    }

    /// Another skill that must be owned before the first purchase.
    pub fn prerequisite(self) -> Option<Skill> {
        match self {
            Self::ShovePlating => Some(Self::Shove),
            _ => None,
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

    /// Chance a hostile shot in the arc is stopped; zero while locked.
    pub fn parry_chance(&self) -> f32 {
        match self.level(Skill::Parry) {
            0 => 0.0,
            n => (t::PARRY_CHANCE + t::PARRY_CHANCE_STEP * f32::from(n - 1)).min(0.95),
        }
    }

    /// Dash reach; zero while locked.
    pub fn dash_distance(&self) -> f32 {
        match self.level(Skill::Dash) {
            0 => 0.0,
            n => t::DASH_DISTANCE + t::DASH_DISTANCE_STEP * f32::from(n - 1),
        }
    }

    pub fn dash_cooldown(&self) -> f32 {
        let n = self.level(Skill::Dash).max(1);
        t::DASH_COOLDOWN - t::DASH_COOLDOWN_STEP * f32::from(n - 1)
    }

    pub fn parry_cooldown(&self) -> f32 {
        let n = self.level(Skill::Parry).max(1);
        t::PARRY_COOLDOWN - t::PARRY_COOLDOWN_STEP * f32::from(n - 1)
    }

    /// Seconds of the parry's opening that count as perfect (zero while locked).
    pub fn parry_perfect(&self) -> f32 {
        match self.level(Skill::Parry) {
            0 => 0.0,
            n => t::PARRY_PERFECT + t::PARRY_PERFECT_STEP * f32::from(n - 1),
        }
    }

    /// Damage multiple of a reflected shot (one while locked).
    pub fn parry_reflect(&self) -> f32 {
        match self.level(Skill::Parry) {
            0 => 1.0,
            n => t::PARRY_REFLECT + t::PARRY_REFLECT_STEP * f32::from(n - 1),
        }
    }

    pub fn cargo_bonus(&self) -> f32 {
        t::CARGO_STEP * self.steps(Skill::Cargo)
    }

    /// Sonar reach, ring speed, recharge and echoes per kind beyond the base ping.
    pub fn ping_range(&self, base: f32) -> f32 {
        base + t::PING_REACH_STEP * self.steps(Skill::PingReach)
    }

    pub fn ping_speed(&self, base: f32) -> f32 {
        base + t::PING_SPEED_STEP * self.steps(Skill::PingSpeed)
    }

    pub fn ping_cooldown(&self, base: f32) -> f32 {
        (base - t::PING_COOLDOWN_STEP * self.steps(Skill::PingCooldown))
            .max(t::PING_COOLDOWN_FLOOR.min(base))
    }

    /// Beacons that may stand at once; zero while locked.
    pub fn beacon_limit(&self) -> usize {
        t::BEACONS_PER_LEVEL * usize::from(self.level(Skill::Beacon))
    }

    /// Share of the charge-up that remains at this beacon level (one at level 1).
    pub fn travel_charge_factor(&self) -> f32 {
        let n = self.level(Skill::Beacon).max(1);
        1.0 - t::TRAVEL_CHARGE_LEVEL_CUT * f32::from(n - 1)
    }

    /// Multiplier on the momentum a ram imparts to a free body (one while locked).
    pub fn shove_mult(&self) -> f32 {
        1.0 + t::SHOVE_MULT_STEP * self.steps(Skill::Shove)
    }

    /// Most speed one ram can add to a free body on top of the ordinary contact.
    pub fn shove_bonus_dv(&self) -> f32 {
        t::SHOVE_BONUS_DV + t::SHOVE_BONUS_DV_STEP * self.steps(Skill::Shove)
    }

    /// Speed limit of a body the ship shoved.
    pub fn shove_speed_cap(&self) -> f32 {
        t::SHOVE_SPEED_CAP + t::SHOVE_SPEED_CAP_STEP * self.steps(Skill::Shove)
    }

    /// The beam grip: open space past which it pulls, its pull cap and where it breaks away.
    pub fn grip_accel(&self) -> f32 {
        t::GRIP_ACCEL + t::GRIP_ACCEL_STEP * self.steps(Skill::Shove)
    }

    pub fn grip_reach(&self) -> f32 {
        t::GRIP_REACH + t::GRIP_REACH_STEP * self.steps(Skill::Shove)
    }

    /// Impulse of a dash whip.
    pub fn whip_impulse(&self) -> f32 {
        t::WHIP_IMPULSE * (1.0 + t::WHIP_STEP * self.steps(Skill::Shove))
    }

    /// Share of an impact the ship takes, as a multiple of the ordinary share, when the ship
    /// caused it (`caused`) or not.
    pub fn plating_factor(&self, caused: bool) -> f32 {
        let level = self.level(Skill::ShovePlating);
        let cut = if caused {
            t::PLATING_CAUSED_STEP * f32::from(level)
        } else {
            t::PLATING_ALL_STEP * f32::from(level.saturating_sub(t::PLATING_ALL_FROM - 1))
        };
        (1.0 - cut).max(0.0)
    }

    /// Organ slots the rig has opened.
    pub fn organ_slots(&self) -> usize {
        usize::from(self.level(Skill::Symbiosis))
    }

    pub fn ping_extra_targets(&self) -> usize {
        t::PING_TARGETS_STEP * usize::from(self.level(Skill::PingTargets))
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
            if skill.max_level() > 1 {
                assert!(last > first);
            }
            assert!(skill.price(skill.max_level()).is_none());
        }
    }
}
