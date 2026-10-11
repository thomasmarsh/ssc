//! The capability poset: which channel of harm each enemy power and weapon feeds, which
//! player organs, traits and skills answer each channel at what degree, and which channels may
//! gate an area. Design: `docs/CAPABILITIES.md` (sections 1.3, 2 and 5.3). This module is data
//! and pure functions only; nothing in the simulation reads it yet (slice K0), so adding a
//! power, weapon, organ, trait or skill fails to compile until it is classified here.
//!
//! Every coverage source carries a short stable `reason` and every channel a one-line `need`
//! and `answer`, so a UI can say what covers what and why, and show progress as degrees add
//! up (the player never has to memorize combos).

use crate::simulation::organs::Organ;
use crate::simulation::skills::Skill;
use crate::simulation::upgrades::{Loadout, Trait};

/// Number of channels.
pub const CHANNELS: usize = 13;
/// Highest coverage degree: 1 is a tax reducer, 2 clears a gate, 3 is immunity or comfort.
pub const MAX_DEGREE: u8 = 3;

/// One way the world hurts or constrains the ship, named by what the player must do about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channel {
    Volley,
    Mines,
    Bypass,
    Jam,
    Phase,
    Close,
    Field,
    Cord,
    Ram,
    Drain,
    Swarm,
    Armor,
    Info,
}

/// What lacking a channel's answer does to a build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Lacking the answer can make an area unsurvivable (only four channels).
    Gate,
    /// Lacking it costs danger, upkeep or a slower route, never entry.
    Tax,
    /// Timing or position answers it; gear only widens the margin.
    Tactic,
}

impl Channel {
    pub const ALL: [Channel; CHANNELS] = [
        Self::Volley,
        Self::Mines,
        Self::Bypass,
        Self::Jam,
        Self::Phase,
        Self::Close,
        Self::Field,
        Self::Cord,
        Self::Ram,
        Self::Drain,
        Self::Swarm,
        Self::Armor,
        Self::Info,
    ];

    pub fn index(self) -> usize {
        match self {
            Self::Volley => 0,
            Self::Mines => 1,
            Self::Bypass => 2,
            Self::Jam => 3,
            Self::Phase => 4,
            Self::Close => 5,
            Self::Field => 6,
            Self::Cord => 7,
            Self::Ram => 8,
            Self::Drain => 9,
            Self::Swarm => 10,
            Self::Armor => 11,
            Self::Info => 12,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Volley => "VOLLEY",
            Self::Mines => "MINES",
            Self::Bypass => "BYPASS",
            Self::Jam => "JAM",
            Self::Phase => "PHASE",
            Self::Close => "CLOSE",
            Self::Field => "FIELD",
            Self::Cord => "CORD",
            Self::Ram => "RAM",
            Self::Drain => "DRAIN",
            Self::Swarm => "SWARM",
            Self::Armor => "ARMOR",
            Self::Info => "INFO",
        }
    }

    /// Gate, tax or tactic by default (section 2.3). Only JAM, FIELD, ARMOR and INFO gate; a
    /// fifth gate needs a certificate and a BALANCE review.
    pub fn kind(self) -> Kind {
        match self {
            Self::Jam | Self::Field | Self::Armor | Self::Info => Kind::Gate,
            Self::Volley | Self::Bypass | Self::Close | Self::Ram | Self::Drain | Self::Swarm => {
                Kind::Tax
            }
            Self::Mines | Self::Phase | Self::Cord => Kind::Tactic,
        }
    }

    pub fn can_gate(self) -> bool {
        self.kind() == Kind::Gate
    }

    /// One line: what this channel asks of the ship.
    pub fn need(self) -> &'static str {
        match self {
            Self::Volley => "Survive bursts of shots, or kill the shooter first.",
            Self::Mines => "Spot placed sigils and mines before touching them.",
            Self::Bypass => "Take hits that skip the shield on hull alone.",
            Self::Jam => "Keep weapons, dash and aim working while systems are jammed.",
            Self::Phase => "Hit enemies that are untargetable except in short windows.",
            Self::Close => "Answer enemies that blink or charge in and strike.",
            Self::Field => "Fly and fight through pulls, shoves, slow zones and wells.",
            Self::Cord => "Break or avoid cords that latch, haul and web the ship.",
            Self::Ram => "Take body contact, charges and flings.",
            Self::Drain => "Outlast slow losses of shield and hull.",
            Self::Swarm => "Deal with numbers, more bodies than single shots.",
            Self::Armor => "Get damage through plating, bubbles and adapted hides.",
            Self::Info => "See true targets through decoys, glare and darkness.",
        }
    }

    /// One line: what answers this channel.
    pub fn answer(self) -> &'static str {
        match self {
            Self::Volley => "Hull and shield, parry, dash, or range that kills first.",
            Self::Mines => "Sonar to see sigils, then a shot to clear them.",
            Self::Bypass => "A deeper hull, or a parry that reflects the spear.",
            Self::Jam => {
                "A Faraday organ (immune at level 3), a Gyro for confusion, or leave the ring."
            }
            Self::Phase => "Wait out the solid window; a Veil organ widens it.",
            Self::Close => {
                "Parry or dash through the tell, or a tail gun and field; a Seam needle shortens the escape."
            }
            Self::Field => "Ballast, an Anchor or a skip node; thrust alone only reduces the cost.",
            Self::Cord => "Cord shears or a Cord-cutter, a dash, or three shots on the cord.",
            Self::Ram => "A lunatic field or ramming plating; mobility to stay clear.",
            Self::Drain => {
                "Dash off, a Remora mend, a Tick tonic or a Gizzard, or siphon from kills."
            }
            Self::Swarm => {
                "Area weapons: nova, blast, missiles and mines; a Mote cloud eats the first shots."
            }
            Self::Armor => {
                "Lance (pierce) for bubbles, needles or a Pith spur to strip shields, heavy hits."
            }
            Self::Info => {
                "Sonar reach and echo tiers, an Argus eye or a Gloom vesicle; ping from outside the dark."
            }
        }
    }
}

/// Coverage of one channel, 0 to `MAX_DEGREE`, additive across sources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cap(u8);

impl Cap {
    pub const ZERO: Cap = Cap(0);

    pub fn new(degree: u8) -> Cap {
        Cap(degree.min(MAX_DEGREE))
    }

    pub fn degree(self) -> u8 {
        self.0
    }

    /// Adds a source's degree, saturating at `MAX_DEGREE`.
    pub fn plus(self, degree: u8) -> Cap {
        Cap::new(self.0.saturating_add(degree))
    }
}

/// What one source gives to one channel at its top level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cover {
    pub channel: Channel,
    /// Degree reached at the source's top level; lower levels give `ceil(max * level / top)`.
    pub max: u8,
    /// Stable short phrase for the UI: how this source answers the channel.
    pub reason: &'static str,
}

/// A struct literal, so tables are promoted to `'static`.
macro_rules! cover {
    ($channel:expr, $max:expr, $reason:expr) => {
        Cover {
            channel: $channel,
            max: $max,
            reason: $reason,
        }
    };
}

/// The degree a source of `level` out of `top` gives for a cover of `max`.
pub fn degree_at(max: u8, level: u8, top: u8) -> u8 {
    if top == 0 || level == 0 {
        return 0;
    }
    let level = level.min(top) as u32;
    (u32::from(max) * level).div_ceil(u32::from(top)) as u8
}

/// What an organ covers at its top level (3).
pub fn organ_covers(organ: Organ) -> &'static [Cover] {
    use Channel::*;
    match organ {
        Organ::Remora => &[cover!(Drain, 2, "mends the hull while quiet")],
        Organ::Faraday => &[
            cover!(
                Jam,
                3,
                "shortens jams, no HUD jam from level 2, immune at level 3"
            ),
            cover!(Info, 1, "shortens a glare's blind sonar and lit stealth"),
        ],
        Organ::Veil => &[cover!(
            Phase,
            2,
            "keeps the ship intangible after a dash and lets shots hit phased bodies"
        )],
        Organ::Skipjack => &[
            cover!(Cord, 2, "hops over cords"),
            cover!(Close, 1, "hops clear of a charge"),
            cover!(Field, 1, "hops out of a pull"),
        ],
        Organ::ArgusEye => &[cover!(
            Info,
            2,
            "a glare blinds the sonar and lights the stealth less (not at all at level 3) and mimics are seen close up"
        )],
        Organ::Gloom => &[cover!(
            Info,
            1,
            "a quiet ship is noticed from less far off, field or no field"
        )],
        Organ::Anchor => &[
            cover!(Field, 2, "pulls, pushes and shoves lose their grip"),
            cover!(Close, 1, "a drag or a shove cannot throw the ship"),
        ],
        Organ::Cutter => &[cover!(
            Cord,
            2,
            "parts latched cords at once, with or without shears"
        )],
        Organ::Spur => &[cover!(
            Armor,
            2,
            "shots skip a share of the shield they strike"
        )],
        Organ::Motes => &[
            cover!(Volley, 1, "a mote takes one hostile shot and grows back"),
            cover!(Swarm, 2, "motes eat the first shots of a crowd"),
        ],
        Organ::Seam => &[cover!(
            Close,
            1,
            "a shorter jump charge leaves a lost fight sooner"
        )],
        Organ::Gyro => &[cover!(
            Jam,
            1,
            "confusion sways less and never flips the turn"
        )],
        Organ::Tonic => &[cover!(Drain, 2, "a hullworm drains less")],
        Organ::Gizzard => &[cover!(Drain, 1, "a swallowing blob digests more slowly")],
        Organ::Tympanum => &[cover!(Field, 1, "a song ring's blow and shove are weaker")],
    }
}

/// What a trait covers at its cap.
pub fn trait_covers(kind: Trait) -> &'static [Cover] {
    use Channel::*;
    match kind {
        Trait::Spread => &[cover!(Swarm, 1, "a wider fan hits more bodies")],
        Trait::Pierce => &[
            cover!(Armor, 3, "shots pass through bubbles and plating"),
            cover!(Bypass, 1, "shots pass through the spear"),
        ],
        Trait::Homing => &[
            cover!(Volley, 1, "shots seek the shooter"),
            cover!(Mines, 1, "seeking shots clear sigils"),
        ],
        Trait::Broadside => &[cover!(Swarm, 1, "flank guns reach surrounding bodies")],
        Trait::Tailgun => &[cover!(Close, 1, "the stern gun hits what closes in")],
        Trait::Blast => &[
            cover!(Swarm, 2, "bursts hit every body in the cloud"),
            cover!(Armor, 1, "area hits ignore density"),
        ],
        Trait::Ram => &[cover!(Ram, 1, "rams hit back")],
        Trait::Shears => &[cover!(Cord, 2, "cuts latched cords at once")],
        Trait::Ballast => &[
            cover!(Field, 2, "wells barely tug and cannot hurt"),
            cover!(Close, 1, "resists drag"),
        ],
        Trait::Siphon => &[cover!(Drain, 1, "kills mend the hull")],
        Trait::Aura => &[
            cover!(Ram, 2, "flings what touches the ship"),
            cover!(Close, 1, "flings what closes in"),
            cover!(Swarm, 1, "flings the crowd away"),
        ],
        Trait::Missiles => &[cover!(Swarm, 2, "salvos hit many bodies")],
        Trait::Mines => &[
            cover!(Swarm, 1, "mines catch followers"),
            cover!(Close, 1, "mines punish a pursuit"),
        ],
        Trait::Needles => &[
            cover!(Armor, 2, "thin hits strip shields by volume"),
            cover!(Swarm, 1, "dense bursts cover a wide arc"),
        ],
        Trait::Nova => &[cover!(Swarm, 3, "a ring of shots hits all around")],
        Trait::Hardening => &[cover!(
            Jam,
            2,
            "a hardened casing shortens jams and glitches"
        )],
    }
}

/// What a skill covers at its max level.
pub fn skill_covers(skill: Skill) -> &'static [Cover] {
    use Channel::*;
    match skill {
        Skill::Parry => &[
            cover!(Volley, 1, "turns shots aside in an arc"),
            cover!(Bypass, 1, "reflects the spear"),
            cover!(Close, 1, "turns a strike aside"),
        ],
        Skill::Dash => &[
            cover!(Volley, 1, "slips out of a volley"),
            cover!(Field, 1, "breaks out of a pull"),
            cover!(Cord, 2, "snaps weak cords"),
            cover!(Ram, 1, "slips a charge"),
            cover!(Drain, 2, "tears off a latch"),
            cover!(Close, 1, "slips a strike"),
        ],
        Skill::ShovePlating => &[cover!(Ram, 1, "takes less of every impact")],
        Skill::PingReach => &[cover!(Info, 1, "sees truth from farther away")],
        Skill::PingTargets => &[cover!(Mines, 1, "marks more sigils and mines")],
        Skill::EchoPredators => &[cover!(Info, 1, "reads predator density before entry")],
        Skill::BeamPower
        | Skill::BeamRange
        | Skill::Yield
        | Skill::Magnet
        | Skill::Cargo
        | Skill::PingSpeed
        | Skill::PingCooldown
        | Skill::EchoPads
        | Skill::EchoLodes
        | Skill::EchoNests
        | Skill::Beacon
        | Skill::Shove
        | Skill::Symbiosis => &[],
    }
}

/// A coverage source on a ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Organ(Organ),
    Trait(Trait),
    Skill(Skill),
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Organ(o) => o.label(),
            Self::Trait(t) => t.label(),
            Self::Skill(s) => s.label(),
        }
    }

    pub fn covers(self) -> &'static [Cover] {
        match self {
            Self::Organ(o) => organ_covers(o),
            Self::Trait(t) => trait_covers(t),
            Self::Skill(s) => skill_covers(s),
        }
    }
}

/// One source's live contribution to one channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contribution {
    pub source: Source,
    pub channel: Channel,
    pub degree: u8,
    pub reason: &'static str,
}

/// Coverage of every channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    caps: [Cap; CHANNELS],
}

impl Coverage {
    pub fn get(&self, channel: Channel) -> Cap {
        self.caps[channel.index()]
    }

    pub fn is_zero(&self) -> bool {
        self.caps.iter().all(|c| c.degree() == 0)
    }

    /// Whether this coverage is at least `other` on every channel (the poset order).
    pub fn dominates(&self, other: &Coverage) -> bool {
        self.caps.iter().zip(&other.caps).all(|(a, b)| a >= b)
    }
}

/// Every live contribution of a loadout: fitted and awake organs, traits at their live level,
/// skills at their bought level. Sorted by channel, then by descending degree.
pub fn contributions(loadout: &Loadout) -> Vec<Contribution> {
    let mut out = Vec::new();
    let mut push = |source: Source, level: u8, top: u8| {
        for c in source.covers() {
            let degree = degree_at(c.max, level, top);
            if degree > 0 {
                out.push(Contribution {
                    source,
                    channel: c.channel,
                    degree,
                    reason: c.reason,
                });
            }
        }
    };
    for organ in Organ::ALL {
        if let Some(strain) = loadout.organs.active(organ) {
            push(
                Source::Organ(organ),
                strain.level,
                crate::simulation::tuning::ORGAN_LEVELS,
            );
        }
    }
    let stats = loadout.stats();
    for kind in Trait::ALL {
        push(Source::Trait(kind), stats.level(kind), kind.cap());
    }
    for skill in Skill::ALL {
        push(
            Source::Skill(skill),
            loadout.skills.level(skill),
            skill.max_level(),
        );
    }
    out.sort_by_key(|c| (c.channel, std::cmp::Reverse(c.degree)));
    out
}

/// The coverage vector of a loadout: contributions add per channel, capped at `MAX_DEGREE`.
pub fn coverage(loadout: &Loadout) -> Coverage {
    let mut cov = Coverage::default();
    for c in contributions(loadout) {
        let cap = &mut cov.caps[c.channel.index()];
        *cap = cap.plus(c.degree);
    }
    cov
}

/// Enemy powers by channel.
pub mod power {
    use super::Channel;
    use crate::power::Power;

    /// The one channel a power feeds, or None where the power is not hostile to the ship
    /// (a bond, a traversal door).
    pub fn channel(power: Power) -> Option<Channel> {
        match power {
            Power::Phase => Some(Channel::Phase),
            Power::Repel | Power::Warp | Power::Lens | Power::Song => Some(Channel::Field),
            Power::Blink => Some(Channel::Close),
            Power::Bypass => Some(Channel::Bypass),
            Power::Emp | Power::Confuse => Some(Channel::Jam),
            Power::Glare | Power::Mimic | Power::Dim => Some(Channel::Info),
            Power::Latch | Power::Engulf => Some(Channel::Drain),
            Power::Cloud => Some(Channel::Armor),
            Power::Devour => Some(Channel::Ram),
            Power::Weave | Power::Sling => Some(Channel::Cord),
            Power::Rune => Some(Channel::Mines),
            Power::Split => Some(Channel::Swarm),
            Power::Symbiote | Power::Rift => None,
        }
    }
}

/// Enemy weapons by channel.
pub mod weapon {
    use super::Channel;
    use crate::genome::Weapon;

    /// The channel a weapon feeds, or None for an unarmed creature.
    pub fn channel(weapon: Weapon) -> Option<Channel> {
        match weapon {
            Weapon::Projectile
            | Weapon::Needles
            | Weapon::Missile
            | Weapon::Nova
            | Weapon::Spiral => Some(Channel::Volley),
            Weapon::Mine => Some(Channel::Mines),
            Weapon::Tether => Some(Channel::Cord),
            Weapon::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Weapon;
    use crate::power::Power;

    #[test]
    fn channel_indices_are_dense_and_ordered() {
        for (i, c) in Channel::ALL.iter().enumerate() {
            assert_eq!(c.index(), i);
        }
        assert_eq!(Channel::ALL.len(), CHANNELS);
    }

    #[test]
    fn the_gate_set_is_exactly_the_four() {
        let gates: Vec<_> = Channel::ALL.into_iter().filter(|c| c.can_gate()).collect();
        assert_eq!(
            gates,
            [Channel::Jam, Channel::Field, Channel::Armor, Channel::Info]
        );
    }

    #[test]
    fn every_power_is_classified_and_only_boons_are_not() {
        let unmapped: Vec<_> = Power::ALL
            .into_iter()
            .filter(|&p| power::channel(p).is_none())
            .collect();
        assert_eq!(unmapped, [Power::Symbiote, Power::Rift]);
    }

    #[test]
    fn every_weapon_is_classified() {
        for w in [
            Weapon::Projectile,
            Weapon::Needles,
            Weapon::Missile,
            Weapon::Mine,
            Weapon::Nova,
            Weapon::Spiral,
            Weapon::Tether,
        ] {
            assert!(weapon::channel(w).is_some(), "{w:?}");
        }
        assert_eq!(weapon::channel(Weapon::None), None);
    }

    #[test]
    fn every_source_has_stable_reasons_and_sane_degrees() {
        let mut sources = Vec::new();
        sources.extend(Organ::ALL.map(Source::Organ));
        sources.extend(Trait::ALL.map(Source::Trait));
        sources.extend(Skill::ALL.map(Source::Skill));
        for s in sources {
            let covers = s.covers();
            for (i, c) in covers.iter().enumerate() {
                assert!(!c.reason.is_empty(), "{} reason", s.label());
                assert!((1..=MAX_DEGREE).contains(&c.max), "{} degree", s.label());
                assert!(
                    covers[..i].iter().all(|p| p.channel != c.channel),
                    "{} lists {:?} twice",
                    s.label(),
                    c.channel
                );
            }
        }
    }

    #[test]
    fn every_ward_answers_the_channel_its_donor_feeds() {
        use crate::simulation::organs::{Aspect, ORGANS};
        for k in ORGANS.iter().filter(|k| k.aspect == Aspect::Ward) {
            let fed = power::channel(k.power).expect("a ward's donor is hostile");
            assert!(
                organ_covers(k.organ).iter().any(|c| c.channel == fed),
                "{} does not answer {fed:?}",
                k.label
            );
        }
    }

    #[test]
    fn every_organ_effect_is_named_by_its_channel_answer() {
        // The need and answer strings name every catalog organ somewhere, so the readout can
        // say what answers a channel.
        let answers: String = Channel::ALL.iter().map(|c| c.answer()).collect();
        for name in [
            "Anchor",
            "Cord-cutter",
            "Mote cloud",
            "Seam needle",
            "Gyro",
            "Tick tonic",
            "Gizzard",
            "Pith spur",
            "Argus eye",
            "Gloom vesicle",
        ] {
            assert!(answers.contains(name), "{name} is in no channel answer");
        }
    }

    #[test]
    fn every_channel_has_need_and_answer() {
        for c in Channel::ALL {
            assert!(!c.need().is_empty() && !c.answer().is_empty());
            assert!(!c.need().contains('\u{2014}') && !c.answer().contains('\u{2014}'));
        }
    }

    #[test]
    fn every_channel_is_answerable_by_some_source() {
        for c in Channel::ALL {
            let any = Organ::ALL
                .iter()
                .any(|&o| organ_covers(o).iter().any(|k| k.channel == c))
                || Trait::ALL
                    .iter()
                    .any(|&t| trait_covers(t).iter().any(|k| k.channel == c))
                || Skill::ALL
                    .iter()
                    .any(|&s| skill_covers(s).iter().any(|k| k.channel == c));
            // PHASE has only the Veil, which is still a source.
            assert!(any, "{c:?} has no answer");
        }
    }

    #[test]
    fn a_stock_loadout_covers_nothing() {
        let cov = coverage(&Loadout::default());
        assert!(cov.is_zero());
        assert!(contributions(&Loadout::default()).is_empty());
    }

    #[test]
    fn degree_scales_with_level_and_saturates() {
        assert_eq!(degree_at(3, 1, 3), 1);
        assert_eq!(degree_at(3, 2, 3), 2);
        assert_eq!(degree_at(3, 3, 3), 3);
        assert_eq!(degree_at(2, 1, 1), 2);
        assert_eq!(degree_at(3, 0, 3), 0);
        assert_eq!(Cap::new(2).plus(5).degree(), MAX_DEGREE);
    }

    /// A loadout with Faraday owned at `level`, fitted or not (built the way a save would: the
    /// arrays are sized by the organ table, so the text is generated from it).
    fn with_faraday(level: u8, fitted: bool) -> Loadout {
        let slots = if fitted { "[Faraday]" } else { "[]" };
        let owned = Organ::ALL
            .iter()
            .map(|&o| {
                if o == Organ::Faraday {
                    format!("Some((organ:Faraday,level:{level},magnitude:1.0))")
                } else {
                    "None".to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        let paid = vec!["false"; Organ::ALL.len()].join(",");
        let text = format!("(owned:({owned}),slots:{slots},paid:({paid}),loan:None,dormant:false)");
        Loadout {
            organs: ron::from_str(&text).expect("organs"),
            ..Loadout::default()
        }
    }

    #[test]
    fn a_fitted_faraday_covers_jam_by_level() {
        assert!(coverage(&with_faraday(2, false)).is_zero());
        for level in 1..=3 {
            let loadout = with_faraday(level, true);
            let cov = coverage(&loadout);
            assert_eq!(cov.get(Channel::Jam).degree(), level);
            assert!(cov.dominates(&Coverage::default()));
            let why = contributions(&loadout);
            assert!(
                why.iter()
                    .any(|c| c.channel == Channel::Jam && !c.reason.is_empty())
            );
        }
    }

    // ---- K9: properties and certificates over a generated window (docs/CAPABILITIES.md 2.4, 3.5)

    use crate::apex::Archetype;
    use crate::readout::{AreaProfile, AreaReadout, Verdict};
    use crate::realm::{self, RealmKind};
    use crate::simulation::organs;
    use crate::simulation::tuning::Tunables;
    use crate::world::SectorId;
    use std::collections::{HashMap, HashSet};

    const SEEDS: [u64; 4] = [crate::config::MASTER_SEED, 1, 7, 0xC0FFEE];
    /// The lock core: where a realm's effects are at full strength (`realm::KEEPER_BAND`'s ceiling).
    const LOCK_CORE: f32 = 0.85;

    fn window() -> impl Iterator<Item = SectorId> {
        (-100..100).flat_map(|x| (-100..100).map(move |y| SectorId { x, y }))
    }

    /// The best a ward donor pays on `channel`: an organ some carrier of `power` may leave.
    fn ward_from(power: Power, channel: Channel) -> Option<Organ> {
        organs::harvestable(power).map(|k| k.organ).find(|&o| {
            organ_covers(o)
                .iter()
                .any(|c| c.channel == channel && c.max >= 2)
        })
    }

    /// Degree a build can reach on `channel` with traits and skills alone (no kill needed:
    /// supplier parts and research nodes), stacked as `coverage` stacks them.
    fn non_kill_degree(channel: Channel) -> u8 {
        let mut cap = Cap::new(0);
        for &t in &Trait::ALL {
            for c in trait_covers(t).iter().filter(|c| c.channel == channel) {
                cap = cap.plus(c.max);
            }
        }
        for &s in &Skill::ALL {
            for c in skill_covers(s).iter().filter(|c| c.channel == channel) {
                cap = cap.plus(c.max);
            }
        }
        cap.degree()
    }

    #[test]
    fn at_most_four_gate_channels_exist() {
        let gates: Vec<Channel> = Channel::ALL.into_iter().filter(|c| c.can_gate()).collect();
        assert!(gates.len() <= 4, "{gates:?}");
        // Whatever a readout blocks on is one of them (a tactic or a tax never blocks).
        let tune = Tunables::default();
        for seed in SEEDS {
            for id in window().step_by(7) {
                let p = AreaProfile::of(seed, id);
                if let Verdict::Blocked(c) =
                    AreaReadout::read(&p, &Loadout::default(), 10.0, &tune, false).verdict
                {
                    assert!(c.can_gate(), "{id:?} blocks on {c:?}");
                }
            }
        }
    }

    #[test]
    fn every_archetype_has_its_own_verb() {
        let mut seen: HashMap<&str, Archetype> = HashMap::new();
        for a in Archetype::ALL {
            assert!(!a.verb().trim().is_empty(), "{a:?} has no verb");
            let clash = seen.insert(a.verb(), a);
            assert!(clash.is_none(), "{a:?} shares a verb with {clash:?}");
        }
    }

    /// One certificate per gate: a ward that a realm keeper pays on a rim, and a way that needs
    /// no kill (a supplier part or a research node, GAME_LOOP section 5).
    #[test]
    fn every_gate_has_a_ward_source_and_a_non_kill_alternative() {
        let gates: Vec<Channel> = Channel::ALL.into_iter().filter(|c| c.can_gate()).collect();
        let carried: Vec<Power> = RealmKind::all()
            .filter(|k| *k != RealmKind::CRADLE)
            .filter_map(|k| k.spec().keeper.map(|(_, p)| p))
            .collect();
        for gate in gates {
            let wards: Vec<Organ> = carried.iter().filter_map(|&p| ward_from(p, gate)).collect();
            assert!(!wards.is_empty(), "{gate:?}: no keeper pays a ward");
            assert!(
                non_kill_degree(gate) >= 2,
                "{gate:?}: no non-kill alternative reaches degree 2"
            );
        }
    }

    /// The key is outside the lock: every realm with a keeper row keeps its keeper on a rim,
    /// inside its own realm, below the lock core, and a keeper on a gate channel pays a ward there or the channel has a non-kill route.
    #[test]
    fn the_key_stands_outside_the_lock() {
        let mut keepers = 0;
        for seed in SEEDS {
            let mut realms: HashSet<u64> = HashSet::new();
            for id in window().step_by(3) {
                let here = realm::realm(seed, id);
                if !realms.insert(here.key) || here.kind == RealmKind::CRADLE {
                    continue;
                }
                let Some((archetype, power)) = here.spec().keeper else {
                    continue;
                };
                let k = realm::keeper_of(seed, id)
                    .unwrap_or_else(|| panic!("{} has no keeper site", here.name));
                assert_eq!((k.archetype, k.power), (archetype, power));
                let site = realm::realm(seed, k.sector);
                assert_eq!(site.key, here.key, "{}: key in its own realm", here.name);
                assert!(
                    site.intensity < LOCK_CORE,
                    "{}: key at intensity {} is inside the lock",
                    here.name,
                    site.intensity
                );
                assert_eq!(realm::keeper_at(seed, k.sector), Some(k));
                if let Some(channel) = power::channel(power).filter(|c| c.can_gate()) {
                    assert!(
                        ward_from(power, channel).is_some() || non_kill_degree(channel) >= 2,
                        "{}: keeper pays no ward for {channel:?} and no alternative",
                        here.name
                    );
                }
                keepers += 1;
            }
        }
        assert!(keepers >= 8, "only {keepers} keepers in four windows");
    }

    #[test]
    fn home_and_the_cradle_are_open_and_neutral() {
        let tune = Tunables::default();
        for seed in SEEDS {
            let home = SectorId { x: 0, y: 0 };
            assert_eq!(realm::realm(seed, home).kind, RealmKind::CRADLE);
            for id in window().filter(|id| realm::realm(seed, *id).kind == RealmKind::CRADLE) {
                assert!(realm::effects(seed, id).is_neutral(), "{id:?}");
                assert!(realm::keeper_of(seed, id).is_none(), "{id:?}");
            }
            for x in -6..=6 {
                for y in -6..=6 {
                    let id = SectorId { x, y };
                    assert_eq!(realm::realm(seed, id).kind, RealmKind::CRADLE);
                    let p = AreaProfile::of(seed, id);
                    let r = AreaReadout::read(&p, &Loadout::default(), 10.0, &tune, false);
                    assert!(
                        !matches!(r.verdict, Verdict::Blocked(_)),
                        "{id:?} is blocked next to HOME"
                    );
                }
            }
            let p = AreaProfile::of(seed, home);
            let r = AreaReadout::read(&p, &Loadout::default(), 10.0, &tune, false);
            assert_eq!(r.verdict, Verdict::Open, "HOME is Open");
        }
    }

    /// A blocked sector always has an answer (a ward a keeper pays or a non-kill route to
    /// degree 2) and a way around: some unblocked sector is reachable without crossing the
    /// lock (found by a bounded flood outward from the sector).
    #[test]
    fn no_sector_is_blocked_without_a_skirt_or_an_answer() {
        let tune = Tunables::default();
        let blocked = |seed: u64, id: SectorId| -> Option<Channel> {
            let p = AreaProfile::of(seed, id);
            match AreaReadout::read(&p, &Loadout::default(), 10.0, &tune, false).verdict {
                Verdict::Blocked(c) => Some(c),
                _ => None,
            }
        };
        for seed in SEEDS {
            for id in window().step_by(2) {
                let Some(c) = blocked(seed, id) else {
                    continue;
                };
                let kept =
                    realm::keeper_of(seed, id).is_some_and(|k| ward_from(k.power, c).is_some());
                assert!(
                    kept || non_kill_degree(c) >= 2,
                    "{seed} {id:?} blocked on {c:?} with no ward and no alternative"
                );
                let mut seen: HashSet<SectorId> = HashSet::from([id]);
                let mut frontier = vec![id];
                let mut out = false;
                'flood: while let Some(at) = frontier.pop() {
                    if seen.len() > 4000 {
                        break;
                    }
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let n = SectorId {
                            x: at.x + dx,
                            y: at.y + dy,
                        };
                        if !seen.insert(n) {
                            continue;
                        }
                        if blocked(seed, n).is_none() {
                            out = true;
                            break 'flood;
                        }
                        frontier.push(n);
                    }
                }
                assert!(out, "{seed} {id:?} blocked on {c:?} with no skirt");
            }
        }
    }
}
