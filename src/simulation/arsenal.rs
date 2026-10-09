//! The ship's arsenal: weapon profiles the ship owns for the run, and the boosts it can
//! run on cargo. Pure data, no game state beyond the collection itself.
//!
//! The rule that shapes everything here is that nothing you own is ever taken away or made
//! worse by finding something: a profile found again gains a level (never loses one), a
//! boost found again keeps the stronger version. What can run out is the *fuel*: every
//! profile but the stock gun is paid for in cargo materials, so mining and salvage stay
//! worth doing, and a dry profile falls back to the free stock fire.

use super::Material;
use super::upgrades::{Effect, Rarity, Slot, Trait};

/// A way of shooting. Exactly one is active at a time; the stock gun is always owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Profile {
    /// The plain pellet gun: free, and what every other profile falls back to.
    Stock,
    Spread,
    Needles,
    Missiles,
    Mines,
    Nova,
    Broadside,
    Tail,
    Homing,
    Pierce,
    Blast,
}

/// The kind of damage a profile deals, for adaptive resistance: tough enemies and apexes harden
/// against the family that has hurt them most lately (see `adapt`). Four families, so a ship
/// with a couple of guns can always switch to one that has not been spammed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    /// Plain pellets and whatever fans, flanks or seeks with them.
    Kinetic,
    /// Needle swarms: many thin hits.
    Needle,
    /// The lance: shots that pass through (the bypass family).
    Lance,
    /// Missiles, mines, novas and bursts: area damage.
    Explosive,
}

impl Family {
    pub const ALL: [Family; 4] = [Self::Kinetic, Self::Needle, Self::Lance, Self::Explosive];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Kinetic => "KINETIC",
            Self::Needle => "NEEDLE",
            Self::Lance => "LANCE",
            Self::Explosive => "EXPLOSIVE",
        }
    }

    /// The tint of the family's pip on a hull bar, as sRGB.
    pub fn tint(self) -> [f32; 3] {
        match self {
            Self::Kinetic => [1.0, 0.82, 0.3],
            Self::Needle => [0.4, 0.9, 1.0],
            Self::Lance => [0.95, 0.95, 1.0],
            Self::Explosive => [1.0, 0.5, 0.25],
        }
    }
}

/// How a profile's shots carry over distance: full damage out to the sweet spot, then a mild
/// falloff over `FALLOFF_SPAN` more, down to `floor` of the damage and never below (a far hit
/// still hurts). Numbers live with the profile so the tradeoff reads at a glance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reach {
    pub sweet: f32,
    pub floor: f32,
}

impl Reach {
    /// The damage multiplier at `distance` travelled.
    pub fn at(self, distance: f32) -> f32 {
        let past = ((distance - self.sweet) / super::tuning::FALLOFF_SPAN).clamp(0.0, 1.0);
        1.0 - (1.0 - self.floor) * past
    }
}

/// How a profile is billed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Billing {
    /// Nothing.
    Free,
    /// Once per trigger pull of the main gun.
    Volley,
    /// Once per launch of its own launcher (a missile salvo, a mine, a nova pulse).
    Launch,
}

impl Profile {
    pub const ALL: [Profile; 11] = [
        Self::Stock,
        Self::Spread,
        Self::Needles,
        Self::Missiles,
        Self::Mines,
        Self::Nova,
        Self::Broadside,
        Self::Tail,
        Self::Homing,
        Self::Pierce,
        Self::Blast,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&p| p == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stock => "STOCK",
            Self::Spread => "SPREAD",
            Self::Needles => "NEEDLES",
            Self::Missiles => "MISSILES",
            Self::Mines => "MINES",
            Self::Nova => "NOVA",
            Self::Broadside => "BROADSIDE",
            Self::Tail => "TAIL GUN",
            Self::Homing => "HOMING",
            Self::Pierce => "LANCE",
            Self::Blast => "BLAST",
        }
    }

    /// The damage family the profile's hits belong to.
    pub fn family(self) -> Family {
        match self {
            Self::Stock | Self::Spread | Self::Broadside | Self::Tail | Self::Homing => {
                Family::Kinetic
            }
            Self::Needles => Family::Needle,
            Self::Pierce => Family::Lance,
            Self::Missiles | Self::Mines | Self::Nova | Self::Blast => Family::Explosive,
        }
    }

    /// Where the profile's shots are at their best and how much they keep far away.
    pub fn reach(self) -> Reach {
        let (sweet, floor) = match self {
            Self::Stock => (800.0, 0.6),
            Self::Spread => (450.0, 0.45),
            Self::Needles => (500.0, 0.4),
            Self::Missiles => (1300.0, 0.8),
            Self::Mines => (300.0, 1.0),
            Self::Nova => (400.0, 0.7),
            Self::Broadside | Self::Tail => (600.0, 0.55),
            Self::Homing => (1000.0, 0.7),
            Self::Pierce => (1100.0, 0.55),
            Self::Blast => (800.0, 0.6),
        };
        Reach { sweet, floor }
    }

    /// How hard the profile kicks the ship per trigger pull, relative to the stock gun (see
    /// `tuning::RECOIL`): the lance and the blast are heavy, a needle burst is not.
    pub fn recoil(self) -> f32 {
        match self {
            Self::Pierce => 1.6,
            Self::Blast => 1.3,
            Self::Stock | Self::Homing => 1.0,
            Self::Spread | Self::Broadside | Self::Tail => 0.7,
            Self::Needles => 0.4,
            Self::Missiles | Self::Mines | Self::Nova => 0.0,
        }
    }

    /// The trait whose levels this profile is (the stock gun has none).
    pub fn weapon_trait(self) -> Option<Trait> {
        match self {
            Self::Stock => None,
            Self::Spread => Some(Trait::Spread),
            Self::Needles => Some(Trait::Needles),
            Self::Missiles => Some(Trait::Missiles),
            Self::Mines => Some(Trait::Mines),
            Self::Nova => Some(Trait::Nova),
            Self::Broadside => Some(Trait::Broadside),
            Self::Tail => Some(Trait::Tailgun),
            Self::Homing => Some(Trait::Homing),
            Self::Pierce => Some(Trait::Pierce),
            Self::Blast => Some(Trait::Blast),
        }
    }

    pub fn from_trait(kind: Trait) -> Option<Profile> {
        Self::ALL
            .into_iter()
            .find(|p| p.weapon_trait() == Some(kind))
    }

    pub fn max_level(self) -> u8 {
        self.weapon_trait().map_or(1, Trait::cap)
    }

    /// Powered profiles share manufactured fuel; stock remains free.
    pub fn material(self) -> Option<Material> {
        match self {
            Self::Stock => None,
            _ => Some(Material::Fuel),
        }
    }

    pub fn billing(self) -> Billing {
        match self {
            Self::Stock => Billing::Free,
            Self::Missiles | Self::Mines | Self::Nova => Billing::Launch,
            _ => Billing::Volley,
        }
    }

    /// Material per trigger pull (or per launch) at level 1. The stock gun fires about six
    /// times a second, so a full 200 hold is roughly one to two minutes of continuous fire.
    fn base_cost(self) -> f32 {
        match self {
            Self::Stock => 0.0,
            Self::Spread => 0.45,
            Self::Needles => 0.7,
            Self::Broadside => 0.45,
            Self::Tail => 0.25,
            Self::Pierce => 0.3,
            Self::Blast => 0.5,
            Self::Homing => 0.3,
            Self::Missiles => 3.5,
            Self::Mines => 3.0,
            Self::Nova => 5.0,
        }
    }

    /// What one use costs at a level: a higher level hits harder and costs a little more.
    pub fn cost(self, level: u8) -> f32 {
        self.base_cost() * (1.0 + 0.2 * f32::from(level.max(1) - 1))
    }

    /// Short account of what the profile does, for notices.
    pub fn summary(self) -> &'static str {
        match self {
            Self::Stock => "plain pellets, free",
            Self::Spread => "fanned shots",
            Self::Needles => "dense needle burst",
            Self::Missiles => "homing missile salvos",
            Self::Mines => "lays mines near hostiles",
            Self::Nova => "ring pulse near hostiles",
            Self::Broadside => "flank guns",
            Self::Tail => "stern gun",
            Self::Homing => "seeking shots",
            Self::Pierce => "shots pass through",
            Self::Blast => "shots burst on impact",
        }
    }
}

impl Trait {
    /// How much one level of this trait adds to a ship's volley rating (see `Stats::power`).
    /// Traits that are not weapon profiles add nothing.
    pub fn volley_weight(self) -> f32 {
        match self {
            Self::Spread | Self::Broadside | Self::Needles => 0.5,
            Self::Pierce => 0.4,
            Self::Missiles => 0.4,
            Self::Nova => 0.35,
            Self::Homing => 0.3,
            Self::Blast => 0.3,
            Self::Tailgun | Self::Mines => 0.25,
            _ => 0.0,
        }
    }
}

/// What finding a profile again did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gain {
    /// Newly owned at this level.
    New(u8),
    Upgraded {
        from: u8,
        to: u8,
    },
    /// Already at the top level; the find is paid out in fuel only.
    Maxed,
}

/// When an owned boost burns its fuel. A boost only runs (and only drains) while its
/// condition holds, so it costs nothing while you are not using it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Need {
    Firing,
    Thrusting,
    /// A hostile close by, or the ship was recently hit.
    Danger,
    /// A pickup within reach.
    Loot,
    /// A gravity well close by.
    Wells,
    /// A hostile cord is latched on.
    Cords,
}

impl Need {
    pub fn label(self) -> &'static str {
        match self {
            Self::Firing => "firing",
            Self::Thrusting => "thrusting",
            Self::Danger => "danger",
            Self::Loot => "loot near",
            Self::Wells => "gravity",
            Self::Cords => "cords",
        }
    }
}

/// An owned boost: the old timed surges, now paid per second of use from the cargo hold.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Boost {
    pub name: String,
    pub slot: Slot,
    pub rarity: Rarity,
    pub effects: Vec<Effect>,
    pub material: Material,
    /// Material per second while running.
    pub drain: f32,
    pub need: Need,
    /// Running right now (its effects count in the ship's stats).
    pub running: bool,
    /// Wanted but the hold had none: the HUD shows it as dry.
    pub dry: bool,
}

impl Boost {
    pub fn rating(&self) -> f32 {
        self.effects.iter().map(Effect::rating).sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoostGain {
    New,
    Upgraded,
    /// Nothing stronger than what is owned; the find is paid out in fuel only.
    Same,
}

/// Everything the ship can shoot with and boost with.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Arsenal {
    levels: [u8; Profile::ALL.len()],
    pub active: Profile,
    /// The profile before the last switch (or fall-back): where a dry gun goes first.
    pub previous: Profile,
    pub boosts: Vec<Boost>,
    /// Master switch for boosts, so fuel can be saved.
    pub boosts_on: bool,
}

impl Default for Arsenal {
    fn default() -> Self {
        let mut levels = [0; Profile::ALL.len()];
        levels[Profile::Stock.index()] = 1;
        Self {
            levels,
            active: Profile::Stock,
            previous: Profile::Stock,
            boosts: Vec::new(),
            boosts_on: true,
        }
    }
}

impl Arsenal {
    /// Level of a profile; zero when not owned.
    pub fn level(&self, profile: Profile) -> u8 {
        self.levels[profile.index()]
    }

    pub fn owns(&self, profile: Profile) -> bool {
        self.level(profile) > 0
    }

    /// Owned profiles in their fixed order (the stock gun first).
    pub fn owned(&self) -> Vec<Profile> {
        Profile::ALL.into_iter().filter(|&p| self.owns(p)).collect()
    }

    /// Unlocks a profile or raises its level; it can only ever go up. Finding the level you
    /// have (or a lower one) is worth one level; finding a higher one jumps to it.
    pub fn acquire(&mut self, profile: Profile, found: u8) -> Gain {
        let cap = profile.max_level();
        let held = self.level(profile);
        let slot = &mut self.levels[profile.index()];
        if held == 0 {
            *slot = found.clamp(1, cap);
            return Gain::New(*slot);
        }
        let to = if found > held { found } else { held + 1 }.min(cap);
        if to > held {
            *slot = to;
            Gain::Upgraded { from: held, to }
        } else {
            Gain::Maxed
        }
    }

    /// Raises an owned profile one level (a bench upgrade). It can only go up and stops at
    /// the trait cap; returns the new level, or None if not owned or already maxed.
    pub fn raise(&mut self, profile: Profile) -> Option<u8> {
        let held = self.level(profile);
        if held == 0 || held >= profile.max_level() {
            return None;
        }
        self.levels[profile.index()] = held + 1;
        Some(held + 1)
    }

    /// Makes an owned profile active. False if it is not owned or already active.
    pub fn set_active(&mut self, profile: Profile) -> bool {
        if !self.owns(profile) || profile == self.active {
            return false;
        }
        self.previous = self.active;
        self.active = profile;
        true
    }

    /// The next (`step` > 0) or previous owned profile, wrapping. None when only one is owned.
    pub fn cycle(&mut self, step: i32) -> Option<Profile> {
        let owned = self.owned();
        if owned.len() < 2 {
            return None;
        }
        let at = owned.iter().position(|&p| p == self.active).unwrap_or(0) as i32;
        let next = owned[(at + step.signum()).rem_euclid(owned.len() as i32) as usize];
        self.set_active(next).then_some(next)
    }

    /// The `n`th owned profile (0 is the stock gun).
    pub fn select(&mut self, n: usize) -> Option<Profile> {
        let profile = *self.owned().get(n)?;
        self.set_active(profile).then_some(profile)
    }

    /// The effect the active profile gives the ship's stats.
    pub fn active_effect(&self) -> Option<Effect> {
        let kind = self.active.weapon_trait()?;
        Some(Effect::Trait(kind, self.level(self.active)))
    }

    /// Volley rating the arsenal adds to the ship's power: the best owned profile in full
    /// and a third of the rest (versatility is worth something, but one fires at a time).
    /// It does not depend on which profile is active or whether it is dry.
    pub fn volley_bonus(&self) -> f32 {
        let values: Vec<f32> = Profile::ALL
            .into_iter()
            .filter_map(|p| Some(p.weapon_trait()?.volley_weight() * f32::from(self.level(p))))
            .collect();
        let best = values.iter().copied().fold(0.0, f32::max);
        best + 0.33 * (values.iter().sum::<f32>() - best)
    }

    /// Adds a boost, or keeps the stronger of two of the same name.
    pub fn add_boost(&mut self, boost: Boost) -> BoostGain {
        match self.boosts.iter_mut().find(|b| b.name == boost.name) {
            Some(held) if boost.rating() > held.rating() + 1e-4 => {
                held.rarity = boost.rarity;
                held.effects = boost.effects;
                held.drain = held.drain.max(boost.drain);
                BoostGain::Upgraded
            }
            Some(_) => BoostGain::Same,
            None => {
                self.boosts.push(boost);
                BoostGain::New
            }
        }
    }

    /// Effects of the boosts that are running now.
    pub fn boost_effects(&self) -> impl Iterator<Item = &Effect> {
        self.boosts
            .iter()
            .filter(|b| b.running)
            .flat_map(|b| &b.effects)
    }

    pub fn stop_boosts(&mut self) {
        for boost in &mut self.boosts {
            boost.running = false;
            boost.dry = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_only_ever_gain_levels() {
        let mut arsenal = Arsenal::default();
        assert_eq!(arsenal.acquire(Profile::Spread, 1), Gain::New(1));
        assert_eq!(
            arsenal.acquire(Profile::Spread, 1),
            Gain::Upgraded { from: 1, to: 2 }
        );
        // A lower find than what is held is still worth one level.
        assert_eq!(
            arsenal.acquire(Profile::Spread, 1),
            Gain::Upgraded { from: 2, to: 3 }
        );
        assert_eq!(arsenal.acquire(Profile::Spread, 1), Gain::Maxed);
        assert_eq!(arsenal.level(Profile::Spread), Trait::Spread.cap());
        // A deep find jumps ahead, clamped to the cap.
        assert_eq!(arsenal.acquire(Profile::Needles, 9), Gain::New(3));
    }

    #[test]
    fn cycling_wraps_and_skips_what_is_not_owned() {
        let mut arsenal = Arsenal::default();
        assert_eq!(arsenal.cycle(1), None);
        arsenal.acquire(Profile::Missiles, 1);
        arsenal.acquire(Profile::Homing, 1);
        assert_eq!(arsenal.cycle(1), Some(Profile::Missiles));
        assert_eq!(arsenal.cycle(1), Some(Profile::Homing));
        assert_eq!(arsenal.cycle(1), Some(Profile::Stock));
        assert_eq!(arsenal.cycle(-1), Some(Profile::Homing));
        assert_eq!(arsenal.previous, Profile::Stock);
        assert_eq!(arsenal.select(1), Some(Profile::Missiles));
        assert_eq!(arsenal.select(7), None);
    }

    #[test]
    fn every_profile_is_billed_in_a_material_except_stock() {
        for profile in Profile::ALL {
            match profile {
                Profile::Stock => {
                    assert_eq!(profile.billing(), Billing::Free);
                    assert_eq!(profile.cost(1), 0.0);
                    assert!(profile.material().is_none());
                }
                _ => {
                    assert_ne!(profile.billing(), Billing::Free);
                    assert!(profile.material().is_some() && profile.cost(1) > 0.0);
                    assert!(profile.cost(profile.max_level()) >= profile.cost(1));
                    assert_eq!(
                        Profile::from_trait(profile.weapon_trait().unwrap()),
                        Some(profile)
                    );
                }
            }
        }
    }

    #[test]
    fn every_profile_has_a_family_a_sweet_spot_and_a_falloff_that_never_reaches_zero() {
        let mut seen = std::collections::HashSet::new();
        for profile in Profile::ALL {
            seen.insert(profile.family());
            let reach = profile.reach();
            assert!(
                reach.sweet >= 250.0 && (0.3..=1.0).contains(&reach.floor),
                "{profile:?}"
            );
            // Full damage inside the sweet spot, then a slide that never rises and has a floor.
            assert_eq!(reach.at(0.0), 1.0);
            assert_eq!(reach.at(reach.sweet), 1.0);
            let mut last = 1.0;
            for step in 0..=200 {
                let m = reach.at(reach.sweet + step as f32 * 30.0);
                assert!(
                    m <= last + 1e-6 && m >= reach.floor - 1e-6 && m > 0.0,
                    "{profile:?}"
                );
                last = m;
            }
            assert!((reach.at(1e6) - reach.floor).abs() < 1e-6);
            assert!(profile.recoil() >= 0.0);
        }
        assert_eq!(seen.len(), Family::ALL.len(), "every family has a profile");
        // The long guns trade damage for reach: the lance keeps its sweet spot longest of the
        // direct guns, the shotgun and needles lose theirs soonest.
        assert!(Profile::Pierce.reach().sweet > Profile::Stock.reach().sweet);
        assert!(Profile::Spread.reach().sweet < Profile::Stock.reach().sweet);
        assert!(Profile::Needles.reach().floor < Profile::Stock.reach().floor);
    }

    #[test]
    fn the_arsenal_adds_power_whatever_is_active() {
        let mut arsenal = Arsenal::default();
        assert_eq!(arsenal.volley_bonus(), 0.0);
        arsenal.acquire(Profile::Spread, 2);
        let one = arsenal.volley_bonus();
        arsenal.acquire(Profile::Needles, 1);
        let two = arsenal.volley_bonus();
        assert!(one > 0.0 && two > one);
        arsenal.set_active(Profile::Needles);
        assert_eq!(arsenal.volley_bonus(), two);
    }
}
