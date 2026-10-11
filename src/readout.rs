//! The area readout (slice K3, `docs/CAPABILITIES.md` section 3 and `docs/BALANCE.md` 6.4 and
//! 6.5): what an area asks of a ship, what this ship answers, and what is missing, said
//! plainly before the player enters. A pure view-model: `AreaProfile` is a function of the seed
//! and a sector (the threat model's shares, level-independent), `AreaReadout` reads it against
//! a loadout. The HUD tag, the entering banner, the star map and the sidebar all draw this one
//! struct, so they cannot disagree with `bin/threat`.
//!
//! The player never has to learn combos: every need is a sentence ("needs JAM cover 2, have 0:
//! FARADAY or JAM HARDENING answers it"), and where sources add up the readout says so.

use crate::capability::{
    self, CHANNELS, Channel, Contribution, Kind, MAX_DEGREE, Source, organ_covers, skill_covers,
    trait_covers,
};
use crate::power::Power;
use crate::simulation::organs::{self, Organ};
use crate::simulation::skills::Skill;
use crate::simulation::tuning::Tunables;
use crate::simulation::upgrades::{Loadout, Trait};
use crate::threat::{self, Class, Tier};
use crate::world::SectorId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// A shooter of the area, reduced to what the burst warning needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Shooter {
    pub name: String,
    /// The share of its damage that skips the shield (it only meets the hull).
    pub pith: f32,
    /// One volley of every armed part, every shot landing.
    pub volley: f32,
    /// What lands in the alpha-strike window at half range.
    pub window: f32,
}

/// An elder of the area and the ward or gland its carried power can leave.
#[derive(Clone, Debug, PartialEq)]
pub struct Keeper {
    pub channel: Channel,
    pub power: Power,
    pub organ: Organ,
}

/// What an area is made of, for any build: a pure function of the master seed and the sector.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaProfile {
    pub sector: SectorId,
    /// The threat level of the area (lambda).
    pub level: f32,
    /// Share of the danger weight by channel at cover zero (level-independent).
    pub share: [f32; CHANNELS],
    /// Per channel, how much more dangerous the area is with that channel uncovered than
    /// with it fully covered (`danger(cover 0) / danger(cover 3) - 1`, the threat model's own
    /// pricing): what a missing answer really costs here.
    pub lift: [f32; CHANNELS],
    /// Per channel, the worst window burst over the reference pool with the ward removed.
    pub gate_burst: [f32; CHANNELS],
    pub shooters: Vec<Shooter>,
    pub keepers: Vec<Keeper>,
    /// The keeper of the realm this sector lies in, wherever it stands (docs 4.2): the one
    /// elder whose slaying pays the realm's ward.
    pub realm_keeper: Option<crate::realm::Keeper>,
}

type Memo = Mutex<HashMap<(u64, i32, i32), Arc<AreaProfile>>>;

fn cache() -> &'static Memo {
    static CACHE: OnceLock<Memo> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl AreaProfile {
    /// The profile of `sector` in the world `seed`, measured by the threat model once and
    /// remembered (a pure memo: the result never depends on anything else).
    pub fn of(seed: u64, sector: SectorId) -> Arc<AreaProfile> {
        let key = (seed, sector.x, sector.y);
        if let Some(hit) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
            return hit;
        }
        let built = Arc::new(Self::measure(seed, sector));
        if let Ok(mut c) = cache().lock() {
            if c.len() > 4096 {
                c.clear();
            }
            c.insert(key, built.clone());
        }
        built
    }

    fn measure(seed: u64, sector: SectorId) -> AreaProfile {
        let report = threat::assess_sector(seed, sector);
        let mut shooters = Vec::new();
        let mut keepers: Vec<Keeper> = Vec::new();
        for o in &report.organisms {
            if threat::is_hostile(o) {
                shooters.push(Shooter {
                    name: o.name.clone(),
                    pith: o.pith,
                    volley: o.volley_damage * o.armed_parts as f32,
                    window: o.burst_expected,
                });
            }
            if o.class != Class::Apex {
                continue;
            }
            for name in &o.powers {
                for power in Power::ALL {
                    if format!("{power:?}").to_lowercase() != *name {
                        continue;
                    }
                    for kind in organs::harvestable(power) {
                        let Some(channel) = capability::power::channel(power) else {
                            continue;
                        };
                        if !keepers.iter().any(|k| k.organ == kind.organ) {
                            keepers.push(Keeper {
                                channel,
                                power,
                                organ: kind.organ,
                            });
                        }
                    }
                }
            }
        }
        let bare = threat::tier_bare();
        let d0 = report.danger_for(&bare).max(1e-6);
        let mut lift = [0.0f32; CHANNELS];
        for c in Channel::ALL {
            if report.share[c.index()] <= 0.0 {
                continue;
            }
            let mut covered = bare.clone();
            covered.cover[c.index()] = MAX_DEGREE;
            lift[c.index()] = (d0 / report.danger_for(&covered).max(1e-6) - 1.0).max(0.0);
        }
        AreaProfile {
            sector,
            level: report.threat,
            share: report.share,
            lift,
            gate_burst: report.gate_burst,
            shooters,
            keepers,
            realm_keeper: crate::realm::keeper_of(seed, sector),
        }
    }

    /// The need per channel, 0 to 3 (docs 3.2): `ceil(3 * share / ref)` and zero below the floor.
    /// Volley is the baseline every ship answers with hull, shield and the rating itself, so it
    /// is read through the band and the burst warning and never as a need.
    pub fn needs(&self, tune: &Tunables) -> [u8; CHANNELS] {
        let mut out = [0u8; CHANNELS];
        for c in Channel::ALL {
            let share = self.share[c.index()];
            if c == Channel::Volley
                || share < tune.readout_need_floor
                || self.lift[c.index()] < tune.readout_tax_min
            {
                continue;
            }
            let degree = (f32::from(MAX_DEGREE) * (share / tune.readout_need_ref).min(1.0)).ceil();
            out[c.index()] = (degree as u8).clamp(1, MAX_DEGREE);
        }
        out
    }
}

/// The rating of the ship against the area, as the HUD verdict ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    Outclassed,
    Underpowered,
    Even,
    Strong,
    Trivial,
}

impl Band {
    pub fn of(ratio: f32, tune: &Tunables) -> Band {
        if ratio < 0.6 {
            Band::Outclassed
        } else if ratio < 0.85 {
            Band::Underpowered
        } else if ratio < 1.3 {
            Band::Even
        } else if ratio < tune.readout_trivial {
            Band::Strong
        } else {
            Band::Trivial
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Band::Outclassed => "OUTCLASSED",
            Band::Underpowered => "UNDERPOWERED",
            Band::Even => "EVEN",
            Band::Strong => "STRONG",
            Band::Trivial => "TRIVIAL",
        }
    }
}

/// What the area is to this build.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    Open,
    /// Enterable at this much more danger (0.25 is plus 25 percent).
    Taxed(f32),
    /// Lethal in a window without the ward of this channel.
    Blocked(Channel),
}

/// How loudly a readout should be shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mood {
    Calm,
    Notice,
    Warn,
    Danger,
}

/// One thing the area asks that this build does not fully answer, or answers by stacking.
#[derive(Clone, Debug, PartialEq)]
pub struct NeedLine {
    pub channel: Channel,
    pub need: u8,
    pub have: u8,
    pub kind: Kind,
    /// One plain sentence: what is missing and what answers it.
    pub text: String,
    /// The channel's own need and answer sentences (`capability::Channel`).
    pub why: &'static str,
    pub answer: &'static str,
    /// Set when sources add up, or when part of the answer is already aboard.
    pub synergy: Option<String>,
}

impl NeedLine {
    pub fn met(&self) -> bool {
        self.have >= self.need
    }
}

/// The warning of a volley against the pool.
#[derive(Clone, Debug, PartialEq)]
pub struct BurstWarn {
    pub who: String,
    /// One volley as a share of the ship's pool (1.0 is the whole pool).
    pub volley: f32,
    /// What lands in the alpha-strike window as a share of the pool.
    pub window: f32,
    pub lethal: bool,
}

impl BurstWarn {
    pub fn text(&self) -> String {
        let (what, share) = if self.volley >= self.window.min(1.0) || self.window < 1.0 {
            ("one volley", self.volley)
        } else {
            ("one second of fire", self.window)
        };
        let pct = (share * 100.0).round() as u32;
        if self.lethal {
            format!(
                "WARNING: {what} from {} is {pct} percent of your pool and can end you",
                self.who
            )
        } else {
            format!(
                "WARNING: {what} from {} is {pct} percent of your pool",
                self.who
            )
        }
    }
}

/// The eight headings of the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bearing {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl Bearing {
    /// The heading of a step of (`dx`, `dy`) sectors with north as +y; None for no step.
    pub fn of_step(dx: i32, dy: i32) -> Option<Bearing> {
        Some(match (dx.signum(), dy.signum()) {
            (0, 1) => Bearing::North,
            (1, 1) => Bearing::NorthEast,
            (1, 0) => Bearing::East,
            (1, -1) => Bearing::SouthEast,
            (0, -1) => Bearing::South,
            (-1, -1) => Bearing::SouthWest,
            (-1, 0) => Bearing::West,
            (-1, 1) => Bearing::NorthWest,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Bearing::North => "NORTH",
            Bearing::NorthEast => "NORTH-EAST",
            Bearing::East => "EAST",
            Bearing::SouthEast => "SOUTH-EAST",
            Bearing::South => "SOUTH",
            Bearing::SouthWest => "SOUTH-WEST",
            Bearing::West => "WEST",
            Bearing::NorthWest => "NORTH-WEST",
        }
    }
}

/// The neighbor that is the way around.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skirt {
    pub bearing: Bearing,
    pub sector: SectorId,
    pub verdict: Verdict,
}

/// Everything the banner, HUD tag, star map and sidebar say about one sector.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaReadout {
    pub sector: SectorId,
    pub level: f32,
    /// The rating over the level, before taxes, and with them.
    pub ratio: f32,
    pub ratio_eff: f32,
    pub band: Band,
    pub verdict: Verdict,
    pub needs: Vec<NeedLine>,
    pub burst: Option<BurstWarn>,
    /// Where a ward for a missing answer can come from, when the sector shows it.
    pub key: Option<String>,
    pub skirt: Option<Skirt>,
}

impl AreaReadout {
    /// Reads `profile` against a ship: `loadout` is the build, `power` its rating (the HUD's
    /// `Game::power`), `known` whether the sector's elders are charted.
    pub fn read(
        profile: &AreaProfile,
        loadout: &Loadout,
        power: f32,
        tune: &Tunables,
        known: bool,
    ) -> AreaReadout {
        let tier = ship_tier(loadout, power);
        let cov = capability::coverage(loadout);
        let need = profile.needs(tune);
        let live = capability::contributions(loadout);
        let mut needs = Vec::new();
        let mut tax = 0.0f32;
        let mut blocked: Option<(Channel, f32)> = None;
        for c in Channel::ALL {
            let n = need[c.index()];
            if n == 0 {
                continue;
            }
            let have = cov.get(c).degree();
            let kind = c.kind();
            let gate = kind == Kind::Gate && n == MAX_DEGREE && have < 2;
            if kind != Kind::Tactic && n > have {
                // What the area really adds without this answer, scaled by how far from full
                // cover the build stands (the threat model prices it, `AreaProfile::lift`).
                let full = profile.lift[c.index()].min(tune.readout_tax_k);
                tax += full * f32::from(n - have) / f32::from(n);
            }
            let burst = profile.gate_burst[c.index()];
            if gate && burst >= 1.0 && blocked.is_none_or(|(_, b)| burst > b) {
                blocked = Some((c, burst));
            }
            if have >= n && !stacked(&live, c) {
                continue;
            }
            needs.push(need_line(c, n, have, kind, gate, &live));
        }
        let tax = tax.min(tune.readout_tax_cap);
        let level = profile.level.max(0.01);
        let ratio = power / level.powf(tune.balance_ref_exponent.max(0.01));
        let ratio_eff = ratio / (1.0 + tax);
        let verdict = match blocked {
            Some((c, _)) => Verdict::Blocked(c),
            None if tax > tune.readout_tax_min => Verdict::Taxed(tax),
            None => Verdict::Open,
        };
        let burst = worst_burst(profile, &tier, tune);
        let key = if known {
            keeper_hint(profile, &needs)
        } else {
            None
        };
        AreaReadout {
            sector: profile.sector,
            level: profile.level,
            ratio,
            ratio_eff,
            band: Band::of(ratio_eff, tune),
            verdict,
            needs,
            burst,
            key,
            skirt: None,
        }
    }

    /// The unmet needs, worst first.
    pub fn missing(&self) -> impl Iterator<Item = &NeedLine> {
        self.needs.iter().filter(|n| !n.met())
    }

    pub fn mood(&self) -> Mood {
        let lethal = self.burst.as_ref().is_some_and(|b| b.lethal);
        if matches!(self.verdict, Verdict::Blocked(_)) || self.band == Band::Outclassed || lethal {
            Mood::Danger
        } else if matches!(self.verdict, Verdict::Taxed(_))
            || self.band == Band::Underpowered
            || self.burst.is_some()
        {
            Mood::Warn
        } else if self.band == Band::Trivial {
            Mood::Calm
        } else {
            Mood::Notice
        }
    }

    /// Whether the entering banner is worth posting (an unremarkable area is not announced).
    pub fn notable(&self) -> bool {
        self.mood() >= Mood::Warn
    }

    /// The verdict in a few words.
    pub fn verdict_text(&self) -> String {
        match self.verdict {
            Verdict::Open => "OPEN".to_string(),
            Verdict::Taxed(t) => format!("TAXED +{:.0} percent", t * 100.0),
            Verdict::Blocked(c) => format!("BLOCKED by {}", c.label()),
        }
    }

    /// Level, rating and (when it is not plain Open) the verdict in one short line.
    pub fn headline_short(&self) -> String {
        match self.verdict {
            Verdict::Open => format!("LEVEL {:.1}  {}", self.level, self.band.label()),
            _ => format!(
                "LEVEL {:.1}  {}  {}",
                self.level,
                self.band.label(),
                self.verdict_text()
            ),
        }
    }

    /// The headline and the first missing answer, as one line.
    pub fn headline(&self) -> String {
        let mut out = self.headline_short();
        if let Some(n) = self.missing().next() {
            out.push_str("  ");
            out.push_str(&n.text);
        }
        out
    }

    /// The skirt as one sentence.
    pub fn skirt_text(&self) -> Option<String> {
        let s = self.skirt?;
        let how = match s.verdict {
            Verdict::Open => "OPEN".to_string(),
            Verdict::Taxed(t) => format!("TAXED +{:.0} percent", t * 100.0),
            Verdict::Blocked(_) => return None,
        };
        Some(format!("SKIRT {}: {how}", s.bearing.label()))
    }

    /// Cost of crossing: lower is easier, infinite when blocked (for skirt choice).
    pub fn cost(&self) -> f32 {
        match self.verdict {
            Verdict::Blocked(_) => f32::INFINITY,
            _ => 1.0 / self.ratio_eff.max(1e-3),
        }
    }
}

/// The ship as a reference tier, so the same burst functions price it as the threat tool does.
pub fn ship_tier(loadout: &Loadout, power: f32) -> Tier {
    let mut cover = [0u8; CHANNELS];
    let cov = capability::coverage(loadout);
    for c in Channel::ALL {
        cover[c.index()] = cov.get(c).degree();
    }
    let share =
        |skill: Skill| f32::from(loadout.skills.level(skill)) / f32::from(skill.max_level().max(1));
    Tier {
        label: "ship".into(),
        stats: loadout.stats(),
        power,
        cover,
        parry: share(Skill::Parry),
        dash: share(Skill::Dash),
        needle: loadout.arsenal.active.family() == crate::simulation::arsenal::Family::Needle,
    }
}

fn worst_burst(profile: &AreaProfile, tier: &Tier, tune: &Tunables) -> Option<BurstWarn> {
    let mut worst: Option<(f32, BurstWarn)> = None;
    for s in &profile.shooters {
        let pool = tier.pool(s.pith).max(1.0);
        let (volley, window) = (s.volley / pool, s.window / pool);
        if volley < tune.readout_volley_warn && window < 1.0 {
            continue;
        }
        let hot = volley.max(window);
        if worst.as_ref().is_none_or(|(h, _)| hot > *h) {
            let warn = BurstWarn {
                who: s.name.clone(),
                volley,
                window,
                lethal: volley >= 1.0 || window >= 1.0,
            };
            worst = Some((hot, warn));
        }
    }
    worst.map(|(_, w)| w)
}

/// Whether two or more live sources feed the channel (they add up).
fn stacked(live: &[Contribution], channel: Channel) -> bool {
    live.iter().filter(|c| c.channel == channel).count() >= 2
}

/// The sources that answer `channel`, by the degree they reach alone, best first, leaving out
/// the ones already giving it.
fn answers(channel: Channel, live: &[Contribution]) -> Vec<(Source, u8)> {
    let mut out: Vec<(Source, u8)> = Vec::new();
    let mut push = |s: Source, covers: &'static [capability::Cover]| {
        if live.iter().any(|c| c.source == s) {
            return;
        }
        if let Some(c) = covers.iter().find(|c| c.channel == channel) {
            out.push((s, c.max));
        }
    };
    for o in Organ::ALL {
        push(Source::Organ(o), organ_covers(o));
    }
    for t in Trait::ALL {
        push(Source::Trait(t), trait_covers(t));
    }
    for s in Skill::ALL {
        push(Source::Skill(s), skill_covers(s));
    }
    out.sort_by_key(|(s, d)| (std::cmp::Reverse(*d), s.label()));
    out
}

fn need_line(
    channel: Channel,
    need: u8,
    have: u8,
    kind: Kind,
    gate: bool,
    live: &[Contribution],
) -> NeedLine {
    let sources = answers(channel, live);
    let missing = need.saturating_sub(have);
    let strong: Vec<String> = sources
        .iter()
        .filter(|(_, d)| *d >= missing.max(1))
        .take(3)
        .map(|(s, _)| s.label().to_uppercase())
        .collect();
    let answer = match strong.as_slice() {
        [] => {
            // No single part reaches it: say which ones add up.
            let parts: Vec<String> = sources
                .iter()
                .take(3)
                .map(|(s, d)| format!("{} {d}", s.label().to_uppercase()))
                .collect();
            if parts.is_empty() {
                channel.answer().to_string()
            } else {
                format!("{} add up", parts.join(" + "))
            }
        }
        [a] => format!("{a} answers it"),
        [a, rest @ ..] => format!("{a} or {} answers it", rest.join(" or ")),
    };
    let text = if kind == Kind::Tactic {
        format!(
            "{} area: {} (timing beats gear)",
            channel.label(),
            channel.need().trim_end_matches('.')
        )
    } else if have >= need {
        format!(
            "{} covered ({have} of {need}) by stacked sources",
            channel.label()
        )
    } else {
        let lead = if gate { "needs" } else { "better with" };
        format!(
            "{lead} {} cover {need}, have {have}: {answer}",
            channel.label()
        )
    };
    let mine: Vec<&Contribution> = live.iter().filter(|c| c.channel == channel).collect();
    let synergy = match mine.len() {
        0 => None,
        1 if have < need => {
            let c = mine[0];
            Some(format!(
                "{} already gives {} ({}); it adds to what you fit next",
                c.source.label().to_uppercase(),
                c.degree,
                c.reason
            ))
        }
        1 => None,
        _ => {
            let parts: Vec<String> = mine
                .iter()
                .map(|c| format!("{} {}", c.source.label().to_uppercase(), c.degree))
                .collect();
            let verb = if have >= need {
                "work together to cover it"
            } else {
                "add up but still fall short"
            };
            Some(format!(
                "{} {verb}: sources stack, no single one is required",
                parts.join(" + ")
            ))
        }
    };
    NeedLine {
        channel,
        need,
        have,
        kind,
        text,
        why: channel.need(),
        answer: channel.answer(),
        synergy,
    }
}

/// The eight-way heading of a step (north is +y), by angle.
fn heading(dx: i32, dy: i32) -> Bearing {
    const ORDER: [Bearing; 8] = [
        Bearing::East,
        Bearing::NorthEast,
        Bearing::North,
        Bearing::NorthWest,
        Bearing::West,
        Bearing::SouthWest,
        Bearing::South,
        Bearing::SouthEast,
    ];
    let angle = (dy as f32).atan2(dx as f32);
    let octant = (angle / std::f32::consts::FRAC_PI_4).round() as i32;
    ORDER[octant.rem_euclid(8) as usize]
}

/// Where the ward of a missing channel is held, from the elders the area shows.
fn keeper_hint(profile: &AreaProfile, needs: &[NeedLine]) -> Option<String> {
    for n in needs.iter().filter(|n| !n.met()) {
        if let Some(k) = profile.realm_keeper {
            let organ = organs::harvestable(k.power).find(|kind| {
                organ_covers(kind.organ)
                    .iter()
                    .any(|c| c.channel == n.channel && c.max >= 2)
            });
            if let Some(kind) = organ {
                let (dx, dy) = (k.sector.x - profile.sector.x, k.sector.y - profile.sector.y);
                let place = match dx.abs().max(dy.abs()) {
                    0 => "in this sector".to_string(),
                    d => format!(
                        "{d} sector{} {}",
                        if d == 1 { "" } else { "s" },
                        heading(dx, dy).label()
                    ),
                };
                return Some(format!(
                    "KEEPER: the {} carrying {} stands {place}; slain, it leaves a {} strain",
                    k.archetype.label().to_uppercase(),
                    format!("{:?}", k.power).to_uppercase(),
                    kind.label
                ));
            }
        }
        for k in &profile.keepers {
            let answers = organ_covers(k.organ)
                .iter()
                .any(|c| c.channel == n.channel && c.max >= 2);
            if answers {
                return Some(format!(
                    "KEEPER: an elder here carries {}; slain, it may leave a {} strain",
                    format!("{:?}", k.power).to_uppercase(),
                    k.organ.label()
                ));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tuning::DEFAULT;

    fn profile(share: [f32; CHANNELS], burst: [f32; CHANNELS]) -> AreaProfile {
        AreaProfile {
            sector: SectorId { x: 5, y: 5 },
            level: 4.0,
            share,
            lift: share.map(|v| if v > 0.0 { 1.0 } else { 0.0 }),
            gate_burst: burst,
            shooters: Vec::new(),
            keepers: Vec::new(),
            realm_keeper: None,
        }
    }

    fn shares(c: Channel, v: f32) -> [f32; CHANNELS] {
        let mut s = [0.0; CHANNELS];
        s[c.index()] = v;
        s
    }

    #[test]
    fn needs_follow_the_share_and_ignore_volley_and_the_floor() {
        let mut s = shares(Channel::Jam, 0.5);
        s[Channel::Swarm.index()] = 0.1;
        s[Channel::Volley.index()] = 0.9;
        s[Channel::Armor.index()] = 0.03;
        let need = profile(s, [0.0; CHANNELS]).needs(&DEFAULT);
        assert_eq!(need[Channel::Jam.index()], 3);
        assert_eq!(need[Channel::Swarm.index()], 2);
        assert_eq!(need[Channel::Volley.index()], 0);
        assert_eq!(need[Channel::Armor.index()], 0);
    }

    #[test]
    fn a_missing_jam_ward_is_taxed_and_said_plainly() {
        let p = profile(shares(Channel::Jam, 0.5), [0.0; CHANNELS]);
        let r = AreaReadout::read(&p, &Loadout::default(), 4.0, &DEFAULT, false);
        assert!(matches!(r.verdict, Verdict::Taxed(t) if t > 0.4));
        let line = r.missing().next().expect("a need");
        assert_eq!((line.channel, line.need, line.have), (Channel::Jam, 3, 0));
        assert!(line.text.contains("JAM cover 3, have 0"), "{}", line.text);
        assert!(line.text.contains("FARADAY"), "{}", line.text);
        assert_eq!(r.mood(), Mood::Warn);
    }

    #[test]
    fn a_gate_without_its_ward_and_a_lethal_burst_blocks() {
        let mut burst = [0.0; CHANNELS];
        burst[Channel::Jam.index()] = 1.4;
        let p = profile(shares(Channel::Jam, 0.5), burst);
        let r = AreaReadout::read(&p, &Loadout::default(), 1.0, &DEFAULT, false);
        assert_eq!(r.verdict, Verdict::Blocked(Channel::Jam));
        assert_eq!(r.mood(), Mood::Danger);
        assert!(r.cost().is_infinite());
        assert!(r.headline().contains("BLOCKED by JAM"));
    }

    #[test]
    fn an_empty_area_is_open_and_the_bands_ladder() {
        let p = profile([0.0; CHANNELS], [0.0; CHANNELS]);
        let r = AreaReadout::read(&p, &Loadout::default(), 1.0, &DEFAULT, false);
        assert_eq!(r.verdict, Verdict::Open);
        assert!(r.needs.is_empty() && r.burst.is_none());
        assert_eq!(Band::of(0.5, &DEFAULT), Band::Outclassed);
        assert_eq!(Band::of(0.7, &DEFAULT), Band::Underpowered);
        assert_eq!(Band::of(1.0, &DEFAULT), Band::Even);
        assert_eq!(Band::of(2.0, &DEFAULT), Band::Strong);
        assert_eq!(Band::of(9.0, &DEFAULT), Band::Trivial);
    }

    #[test]
    fn a_big_volley_warns_in_percent_of_the_pool() {
        let mut p = profile([0.0; CHANNELS], [0.0; CHANNELS]);
        let tier = ship_tier(&Loadout::default(), 1.0);
        let pool = tier.pool(0.0);
        p.shooters.push(Shooter {
            name: "Needler".into(),
            pith: 0.0,
            volley: pool * 0.8,
            window: pool * 0.9,
        });
        let r = AreaReadout::read(&p, &Loadout::default(), 1.0, &DEFAULT, false);
        let warn = r.burst.as_ref().expect("a warning");
        assert!(!warn.lethal);
        assert!(
            warn.text().contains("80 percent of your pool"),
            "{}",
            warn.text()
        );
        assert!(r.mood() >= Mood::Warn);
    }

    #[test]
    fn bearings_cover_the_compass() {
        assert_eq!(Bearing::of_step(0, 1), Some(Bearing::North));
        assert_eq!(Bearing::of_step(-1, -1), Some(Bearing::SouthWest));
        assert_eq!(Bearing::of_step(0, 0), None);
    }

    #[test]
    fn the_profile_of_home_needs_nothing_and_is_memoized() {
        let a = AreaProfile::of(42, SectorId::ORIGIN);
        let b = AreaProfile::of(42, SectorId::ORIGIN);
        assert!(Arc::ptr_eq(&a, &b));
        assert!(a.needs(&DEFAULT).iter().all(|&n| n == 0));
        let r = AreaReadout::read(&a, &Loadout::default(), 1.0, &DEFAULT, true);
        assert_eq!(r.verdict, Verdict::Open);
    }

    #[test]
    fn the_key_names_the_realm_keeper_and_its_heading() {
        let mut p = profile(shares(Channel::Jam, 0.5), [0.0; CHANNELS]);
        p.realm_keeper = Some(crate::realm::Keeper {
            sector: SectorId { x: 5, y: 17 },
            archetype: crate::apex::Archetype::Maelstrom,
            power: Power::Emp,
        });
        let line = NeedLine {
            channel: Channel::Jam,
            need: 3,
            have: 0,
            kind: Kind::Gate,
            text: String::new(),
            why: "",
            answer: "",
            synergy: None,
        };
        let hint = keeper_hint(&p, &[line]).expect("a keeper hint");
        assert!(hint.contains("MAELSTROM") && hint.contains("EMP"), "{hint}");
        assert!(hint.contains("12 sectors NORTH"), "{hint}");
        assert!(hint.contains("FARADAY"), "{hint}");
    }
}
