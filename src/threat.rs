//! Difficulty measurement: what a generated organism can do to a ship, how long it takes to
//! kill, and how dangerous a sector is, all read from the genome, phenotype and realm of its
//! spawns. Read-only: nothing here changes generation or play, and the numbers use the live
//! tunables (`Tunables::DEFAULT`) and the same formulas as the simulation (`weapons`,
//! `creature`, `damage_bypassing`). `src/bin/threat.rs` prints the tables; `docs/BALANCE.md`
//! records the results and proposes the model.
//!
//! Units: the **power index** of an organism is `sqrt(ttk_organism / ttk_ship)` of a duel with
//! the bare ship, so it is on the same scale as `Stats::power` (the bare ship is 1) and an
//! even match is one. A sector's **danger** is `sqrt(sum(hostility * power^2))` over its
//! organisms (Lanchester's square law for a simultaneous engagement).

use crate::apex::Archetype;
use crate::capability::{self, CHANNELS, Channel};
use crate::genome::{Diet, Genome, Species, Trigger, Weapon};
use crate::power::Power;
use crate::range::ring;
use crate::realm;
use crate::simulation::BodyKind;
use crate::simulation::burst::{hit_fraction, weapon_numbers};
use crate::simulation::skills::Skill;
use crate::simulation::tuning::{DEFAULT, Tunables};
use crate::simulation::upgrades::{Loadout, Rarity, Source, Stats, roll_part};
use crate::world::{self, Rng, SectorId, SectorParams, Spawn};

/// The ship's hit radius (`Game::make_body`).
pub const SHIP_RADIUS: f32 = 14.0;
/// Seconds a player needs to see a burst coming and answer it: the alpha-strike window.
pub const WINDOW: f32 = 1.0;
/// The contact cooldown after a ram (`ram_contact`).
const CONTACT_COOLDOWN: f32 = 0.65;
/// The share of the time a melee attacker (a bite, a link, a latch, a digest) is landing on
/// the ship while it is in play: the duty that prices contact damage per second.
const CONTACT_DUTY: f32 = 0.25;
/// Seconds an elder rests after a lunge (`apexes::closers` holds `state.clock` at 1.5).
const LUNGE_RECOVER: f32 = 1.5;
/// Mean of the per-body fire stagger `(id % 7) * 0.13`.
pub(crate) const STAGGER: f32 = 0.39;

/// What kind of thing an organism is, for grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// A wild creature of the sector's ecology.
    Wild,
    /// A member of a civilization (territory) or its fleet.
    Civil,
    /// An apex elder.
    Apex,
    /// A station, turret or fortress gun (no genome).
    Structure,
    /// A tenant of a husk, counted by its multiplicity.
    Den,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wild => "wild",
            Self::Civil => "civil",
            Self::Apex => "apex",
            Self::Structure => "structure",
            Self::Den => "den",
        }
    }

    /// How readily the class goes for the ship unprovoked, as a multiplier on its power in
    /// the sector's danger.
    fn hostility(self, trigger: Trigger) -> f32 {
        match self {
            Self::Apex | Self::Structure => 1.0,
            Self::Civil => 0.3,
            Self::Wild | Self::Den => match trigger {
                Trigger::Sight => 1.0,
                Trigger::Proximity => 0.5,
                Trigger::Harm => 0.15,
            },
        }
    }
}

/// One organism's measured numbers. Damage figures are raw (before the ship's armor).
#[derive(Clone, Debug)]
pub struct Organism {
    pub class: Class,
    pub name: String,
    pub lineage: u64,
    pub weapon: Weapon,
    pub volley: u8,
    pub parts: u32,
    pub armed_parts: u32,
    pub copies: u32,
    pub radius: f32,
    /// Depth threat the creature was born with (it divides the damage it takes).
    pub threat: f32,
    /// Depth and realm sharpening of its damage.
    pub sharpness: f32,
    pub hull: f32,
    pub shield: f32,
    /// The damage the bare ship's gun must deal (before threat and plating) to kill the
    /// head: shield and hull times the realm's multipliers.
    pub pool: f32,
    pub plating: f32,
    pub adaptive: bool,
    /// One projectile.
    pub shot_damage: f32,
    /// Shots per volley per armed part.
    pub shots: u32,
    /// Seconds between one armed part's volleys, calm and enraged.
    pub period: f32,
    pub period_enraged: f32,
    /// The hardest single thing that can land: a projectile, a mine or a bite.
    pub max_hit: f32,
    pub contact_hit: f32,
    /// Damage of one volley of one part if every shot lands.
    pub volley_damage: f32,
    /// What lands in `WINDOW` seconds with every shot hitting, enraged, all parts.
    pub burst_potential: f32,
    /// The same at half weapon range against the ship's radius.
    pub burst_expected: f32,
    /// Sustained damage per second, expected hits, calm.
    pub dps: f32,
    pub pith: f32,
    pub range: f32,
    pub sight: f32,
    pub shot_speed: f32,
    /// Seconds a shot takes to cross the weapon's range: the player's reaction time.
    pub flight_time: f32,
    /// Seconds of visible warning before the organism's biggest burst (an elder's barrage
    /// winds up on the ground); zero when it simply fires.
    pub telegraph: f32,
    pub speed: f32,
    pub cruise: f32,
    pub accel: f32,
    pub evasion: f32,
    pub powers: Vec<String>,
    pub trigger: Trigger,
    /// Seconds the bare ship's gun needs, and the seconds this takes to kill the bare ship.
    pub ttk: f32,
    pub ttk_ship: f32,
    /// The organism's power index (see the module note).
    pub power: f32,
    pub hostility: f32,
    /// The power index before the powers' flair (`power = core * product(1 + term)`).
    pub core: f32,
    /// Each live power's flair term (`role weight * strength`) and the channel it feeds
    /// (`capability::power::channel`; None for a bond or a door, which no cover touches).
    pub flair: Vec<(Option<Channel>, f32)>,
    /// The channel the organism's gun feeds.
    pub weapon_channel: Option<Channel>,
    /// What its powers take from the ship for a moment (EMP removes parry and dash).
    pub disables: Vec<Disable>,
    /// Where the organism's sustained damage to the ship comes from: expected damage per
    /// second and the channel it feeds (the gun, the bite, a lunge, a charge, a cord, a
    /// drain). `core` is priced from their sum, so the channel shares are honest.
    pub sources: Vec<(Option<Channel>, f32)>,
    /// A swarm's flair term and its channel (needle shots snag in it more: `power_for`).
    pub cloud: Option<(Option<Channel>, f32)>,
    /// A haste time bubble's strength (zero for none): armed kin in the sector fire faster
    /// (`pair_partners`, CAPABILITIES K8 row 7).
    pub haste: f32,
}

/// A power that does not hurt but takes a capability away (docs/CAPABILITIES.md 2.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disable {
    pub power: Power,
    pub parry: bool,
    pub dash: bool,
    /// Share of the time the capability is off: hold over period, at most one.
    pub duty: f32,
}

/// What a power takes from the ship, if anything. Exhaustive on purpose.
pub fn disables(power: Power) -> (bool, bool) {
    match power {
        // Stormcap: parry and dash are dead for the hold (BESTIARY fairness caps).
        Power::Emp => (true, true),
        // Scrambled controls break the dash line, not the parry arc.
        Power::Confuse => (false, true),
        Power::Phase
        | Power::Repel
        | Power::Warp
        | Power::Lens
        | Power::Blink
        | Power::Bypass
        | Power::Glare
        | Power::Mimic
        | Power::Latch
        | Power::Symbiote
        | Power::Cloud
        | Power::Devour
        | Power::Weave
        | Power::Song
        | Power::Dim
        | Power::Rift
        | Power::Sling
        | Power::Rune
        | Power::Split
        | Power::Engulf => (false, false),
    }
}

impl Organism {
    /// Burst against a tier's pool: above one it can end the ship inside one window.
    pub fn burst_ratio(&self, tier: &Tier) -> f32 {
        self.burst_expected / tier.pool(self.pith)
    }

    /// The power index against `tier`: each power's flair is cut by the tier's cover of its
    /// channel (`1 - cover / 3`, so degree 3 is immunity). At cover 0 it is `power` exactly.
    pub fn power_for(&self, tier: &Tier) -> f32 {
        let mut flair = 1.0;
        for &(channel, term) in &self.flair {
            let k = channel.map_or(1.0, |c| 1.0 - f32::from(tier.cover[c.index()]) / 3.0);
            flair *= 1.0 + term * k;
        }
        // A needle gun in a swarm: the cloud swallows needles `cloud_needle_density` times as
        // often, so its term is that much larger against a needle kit (K8, row 8).
        if tier.needle
            && let Some((channel, term)) = self.cloud
        {
            let k = channel.map_or(1.0, |c| 1.0 - f32::from(tier.cover[c.index()]) / 3.0);
            let more = (term * DEFAULT.cloud_needle_density * k).min(1.0);
            flair *= (1.0 + more) / (1.0 + term * k);
        }
        self.core * flair
    }

    /// The burst ratio against `tier` with its parry and dash counted: mitigation scales the
    /// pool by `1 / (1 - m)`, and a disabling power of this organism switches its capability off
    /// for the duty. Equal to `burst_ratio` for a tier without those skills.
    pub fn burst_ratio_for(&self, tier: &Tier) -> f32 {
        let (mut off_parry, mut off_dash) = (0.0f32, 0.0f32);
        for d in &self.disables {
            if d.parry {
                off_parry = off_parry.max(d.duty);
            }
            if d.dash {
                off_dash = off_dash.max(d.duty);
            }
        }
        let m = tier.mitigation(off_parry, off_dash);
        self.burst_expected / (tier.pool(self.pith) / (1.0 - m))
    }

    /// Where the organism's weight in the danger goes, by channel, at its power against `tier`
    /// (not normalized: `hostility * copies * power_for^2`, split by channel). The gun keeps
    /// the share the flair does not explain (`1 / flair^2`); the rest is split over the powers'
    /// channels by `ln(1 + term)`. A bond, an unarmed body or a bare door has an unattributed
    /// part (the second value).
    pub fn channel_weight(&self, tier: &Tier) -> ([f32; CHANNELS], f32) {
        let power = self.power_for(tier);
        let total = self.hostility * self.copies as f32 * power * power;
        let mut out = [0.0f32; CHANNELS];
        let ratio = if self.core > 0.0 {
            power / self.core
        } else {
            1.0
        };
        let gun = 1.0 / (ratio * ratio).max(1.0);
        let mut loose = 0.0;
        // The part the flair does not explain is split over the damage sources by their
        // expected damage per second (the gun alone when nothing else bites).
        let harm: f32 = self.sources.iter().map(|&(_, d)| d).sum();
        if harm > 1e-9 {
            for &(c, d) in &self.sources {
                match c {
                    Some(c) => out[c.index()] += total * gun * d / harm,
                    None => loose += total * gun * d / harm,
                }
            }
        } else {
            match self.weapon_channel {
                Some(c) => out[c.index()] += total * gun,
                None => loose += total * gun,
            }
        }
        let rest = total * (1.0 - gun);
        let sum: f32 = self
            .flair
            .iter()
            .map(|&(c, t)| {
                let k = c.map_or(1.0, |c| 1.0 - f32::from(tier.cover[c.index()]) / 3.0);
                (1.0 + t * k).ln()
            })
            .sum();
        for &(c, t) in &self.flair {
            let k = c.map_or(1.0, |c| 1.0 - f32::from(tier.cover[c.index()]) / 3.0);
            let part = rest * (1.0 + t * k).ln() / sum.max(1e-9);
            match c {
                Some(c) => out[c.index()] += part,
                None => loose += part,
            }
        }
        (out, loose)
    }

    pub fn potential_ratio(&self, tier: &Tier) -> f32 {
        self.burst_potential / tier.pool(self.pith)
    }
}

/// A reference player: what a given kit gives.
#[derive(Clone, Debug)]
pub struct Tier {
    pub label: String,
    pub stats: Stats,
    pub power: f32,
    /// Capability degree per channel (`capability::coverage`), 0 for the bare ship.
    pub cover: [u8; CHANNELS],
    /// Trained share of the parry and dash skills, 0 to 1.
    pub parry: f32,
    pub dash: f32,
    /// The active gun fires needles (a swarm swallows them more often).
    pub needle: bool,
}

impl Tier {
    pub fn hull(&self) -> f32 {
        self.stats.max_hull
    }

    pub fn shield(&self) -> f32 {
        self.stats.max_shield
    }

    /// What the ship can absorb in one window at full health, in raw damage: shield and hull
    /// over armor. A share `pith` skips the shield, so only the hull counts against it.
    pub fn pool(&self, pith: f32) -> f32 {
        let guard = self.stats.guard.max(0.05);
        let total = self.stats.max_hull + self.stats.max_shield;
        let hull_only = self.stats.max_hull;
        let blended = total * (1.0 - pith) + hull_only * pith;
        blended / guard
    }

    pub fn dps(&self) -> f32 {
        self.stats.damage / self.stats.fire_period.max(0.01)
    }

    /// The share of damage parry and dash turn aside while the given fractions of the time
    /// each is disabled: `1 - (1 - parry_m (1 - off)) (1 - dash_m (1 - off))`. Zero for a
    /// tier without the skills.
    pub fn mitigation(&self, parry_off: f32, dash_off: f32) -> f32 {
        let p = DEFAULT.balance_parry_mitigation * self.parry * (1.0 - parry_off.clamp(0.0, 1.0));
        let d = DEFAULT.balance_dash_mitigation * self.dash * (1.0 - dash_off.clamp(0.0, 1.0));
        1.0 - (1.0 - p) * (1.0 - d)
    }

    /// The same kit with one ward per gateable channel fitted at degree 2 (a gate cleared),
    /// on top of whatever it already covers.
    pub fn warded(&self) -> Tier {
        let mut t = self.clone();
        for c in Channel::ALL.into_iter().filter(|c| c.can_gate()) {
            let at = &mut t.cover[c.index()];
            *at = (*at).max(2);
        }
        t.label = format!("{}+wards", self.label);
        t
    }

    /// The same kit with parry and dash fully trained.
    /// The same kit with a needle gun in hand (a cloud swallows it more often).
    pub fn needled(&self) -> Tier {
        let mut t = self.clone();
        t.needle = true;
        t.label = format!("{}+needles", self.label);
        t
    }

    pub fn skilled(&self) -> Tier {
        let mut t = self.clone();
        t.parry = 1.0;
        t.dash = 1.0;
        t.label = format!("{}+skills", self.label);
        t
    }
}

/// The bare starting ship.
pub fn tier_bare() -> Tier {
    let stats = Stats::BASE;
    Tier {
        label: "bare".into(),
        stats,
        power: Loadout::default().power(),
        cover: [0; CHANNELS],
        parry: 0.0,
        dash: 0.0,
        needle: false,
    }
}

/// A kit as found at `depth`: `rolls` parts rolled at that depth's threat grade, each bolted
/// on or displacing a weaker one. `maxed` rolls only Epic parts and a lot of them (every slot
/// full of the best of many). `graded` adds the equipment grade of a supplier at that depth.
pub fn tier_at(seed: u64, depth: f32, maxed: bool, graded: bool) -> Tier {
    let grade = world::threat(depth);
    let mut rng = Rng::new(seed ^ 0x7A1E_0000_0000_0001 ^ depth.to_bits() as u64);
    let mut source = Source::plain(grade, SectorParams::HOME);
    let rolls = if maxed {
        source.min_rarity = Rarity::Epic;
        400
    } else {
        14
    };
    let mut loadout = Loadout::default();
    for _ in 0..rolls {
        loadout.acquire(roll_part(&mut rng, &source));
    }
    if graded {
        loadout.equipment_grade = grade;
    }
    let label = format!(
        "{}{} d{depth:.0}",
        if maxed { "maxed" } else { "typical" },
        if graded { "+grade" } else { "" }
    );
    Tier {
        label,
        stats: loadout.stats(),
        power: loadout.power(),
        cover: cover_of(&loadout),
        parry: skill_fraction(&loadout, Skill::Parry),
        dash: skill_fraction(&loadout, Skill::Dash),
        needle: false,
    }
}

/// A loadout's coverage as a channel-indexed array.
fn cover_of(loadout: &Loadout) -> [u8; CHANNELS] {
    let cov = capability::coverage(loadout);
    let mut out = [0u8; CHANNELS];
    for c in Channel::ALL {
        out[c.index()] = cov.get(c).degree();
    }
    out
}

fn skill_fraction(loadout: &Loadout, skill: Skill) -> f32 {
    f32::from(loadout.skills.level(skill)) / f32::from(skill.max_level().max(1))
}

/// How many of a genome's bodies carry its gun (the same rule as `Game::armed_part`).
fn armed_parts(g: &Genome) -> u32 {
    if g.weapon == Weapon::None {
        return 0;
    }
    if g.anatomy.is_some()
        && let Some(plan) = g.developed_body(g.radius)
    {
        let mounts = plan.nodes.iter().filter(|n| n.mount).count() as u32;
        if mounts > 0 {
            return mounts;
        }
        return (0..plan.nodes.len().min(255))
            .filter(|p| g.armed(*p as u8))
            .count()
            .max(1) as u32;
    }
    (0..g.parts().min(255))
        .filter(|p| g.armed(*p as u8))
        .count()
        .max(1) as u32
}

/// Effective speed of the projectile of a weapon at the muzzle.
fn flight_speed(weapon: Weapon, shot_speed: f32) -> f32 {
    match weapon {
        Weapon::Needles => shot_speed * 2.2,
        Weapon::Missile => (shot_speed * 0.55).max(140.0),
        Weapon::Nova => (shot_speed * 0.6).max(110.0),
        Weapon::Spiral => (shot_speed * 0.8).max(120.0),
        Weapon::Projectile | Weapon::Mine | Weapon::None | Weapon::Tether => shot_speed,
    }
    .max(1.0)
}

/// The numbers of a creature genome as born with `phenotype`, against the bare ship.
fn assess_genome(
    class: Class,
    species: &Species,
    phenotype: world::Phenotype,
    copies: u32,
    closer: Option<Archetype>,
    tune: &Tunables,
) -> Organism {
    let g = &species.genome;
    let foe = phenotype.foe;
    let threat = phenotype.threat.max(1.0);
    let sharp = phenotype.sharpness();
    let armed = armed_parts(g);
    let (base_damage, shots, pace) = weapon_numbers(g.weapon, g.volley, tune);
    // The burst budget (docs/BALANCE.md 5.4) is held at fire time by the simulation; the
    // model reads the same scale so its numbers are what is played.
    let scale = crate::simulation::burst::genome_scale(g, &phenotype, armed, tune);
    let shot_damage = base_damage * sharp * scale;
    let aggression = phenotype.aggression.max(0.2);
    let rage = if g.rage > 0.0 { 0.4 / 1.15 } else { 1.0 };
    let period = ((g.fire_period + STAGGER) * pace / aggression).max(0.05);
    let period_enraged = period * rage;
    let pith = g.bypass_share();
    let volley_damage = shot_damage * shots as f32;
    let volleys_in_window = 1.0 + (WINDOW / period_enraged).floor();
    let mut burst_potential = armed as f32 * volley_damage * volleys_in_window;
    let reach = g.weapon_range;
    let at = (reach * 0.5).clamp(150.0, 500.0);
    let hits = hit_fraction(g.weapon, shots, at, SHIP_RADIUS);
    let mut burst_expected = burst_potential * hits;
    let mut telegraph = 0.0;
    if class == Class::Apex {
        // An elder's barrage: a fan of pellets beyond `barrage_range`, wound up for
        // `barrage_windup` seconds with markers on the ground (`apexes::Move::BarrageWind`).
        let n = u32::from(tune.barrage_shots_enraged);
        let share = if n == 1 { 1.0 } else { 0.7 };
        let each = tune.weapon_pellet_damage * share * tune.barrage_share * sharp;
        let each = each * crate::simulation::burst::barrage_scale(tune, threat, each, n);
        let barrage = each * n as f32;
        burst_potential = burst_potential.max(barrage);
        let landed = barrage * hit_fraction(Weapon::Projectile, n, tune.barrage_range, SHIP_RADIUS);
        burst_expected = burst_expected.max(landed);
        telegraph = tune.barrage_windup;
    }
    let dps = armed as f32 * volley_damage * hits / period;
    let contact_hit = g.contact_damage * sharp;
    // An enraged elder stings harder (`apexes::enrage`).
    let sting = if class == Class::Apex {
        tune.elder_enrage_sting.max(1.0)
    } else {
        1.0
    };
    let max_hit = shot_damage.max(contact_hit * sting);
    let speed = g.speed * (1.0 + (phenotype.aggression - 1.0) * 0.4) * foe.speed;
    let cruise = g.cruise * foe.speed * 1.4;
    let tow = (g.parts() as f32 * 0.6).max(1.0);
    let accel = speed * 2.2 * tow;
    let strafe = g.strafe;
    let size = (SHIP_RADIUS / g.radius.max(4.0)).clamp(0.5, 2.0);
    let mut powers = Vec::new();
    let (mut blink, mut phase, mut flair) = (0.0, 0.0, 1.0);
    let (mut cloud, mut haste): (Option<(Option<Channel>, f32)>, f32) = (None, 0.0);
    let mut terms = Vec::new();
    let mut edges = Vec::new();
    // The sources of sustained damage to the ship, expected, calm.
    let mut sources: Vec<(Option<Channel>, f32)> = vec![
        (capability::weapon::channel(g.weapon), dps),
        (
            Some(Channel::Ram),
            contact_hit / CONTACT_COOLDOWN * CONTACT_DUTY,
        ),
    ];
    if class == Class::Apex
        && let Some(archetype) = closer
    {
        // The range closers (`apexes::closers`, `juggernaut`): one contact hit a cycle, the
        // cycle being the wait, the telegraph and the move. A lunge fires at a ship that
        // snipes from `snipe_range`; a charge starts anywhere in its band and reaches the
        // share of it that `speed * time` covers.
        if archetype.lunges() {
            let cycle = tune.snipe_after + tune.lunge_windup + tune.lunge_time + LUNGE_RECOVER;
            let lands = (tune.lunge_speed * tune.lunge_time / tune.snipe_range.max(1.0)).min(1.0);
            sources.push((Some(Channel::Close), contact_hit * lands / cycle));
        }
        if archetype == Archetype::Juggernaut {
            let cycle =
                tune.elder_charge_every_calm + tune.elder_charge_windup + tune.elder_charge_time;
            let band = (tune.elder_charge_range_max - tune.elder_charge_range_min).max(1.0);
            let reach = tune.elder_charge_speed * tune.elder_charge_time;
            let lands = ((reach - tune.elder_charge_range_min) / band).clamp(0.0, 1.0);
            sources.push((Some(Channel::Close), contact_hit * lands / cycle));
        }
    }
    if g.weapon == Weapon::Tether && g.diet == Diet::Siphon {
        // The cord feeds on the shield while it holds (`tether::siphon`).
        sources.push((Some(Channel::Drain), tune.tether_siphon_rate * CONTACT_DUTY));
    }
    if g.bond > 0.0 {
        // A bonded body trails a cord to its neighbour; a ship crossing it takes a link hit.
        sources.push((
            Some(Channel::Cord),
            tune.tether_link_damage / CONTACT_COOLDOWN * CONTACT_DUTY * g.bond.min(1.0),
        ));
    }
    for c in g.live_powers() {
        powers.push(format!("{:?}", c.power).to_lowercase());
        match c.power {
            Power::Blink => blink = c.strength,
            Power::Phase => phase = c.strength,
            Power::Latch => {
                // Priced as the drain it is (`parasite`): the carrier's diet says what it eats.
                let rate = match g.diet {
                    Diet::Rocks | Diet::Graze => 0.0,
                    Diet::Hunt => crate::power::LATCH_HULL_DRAIN,
                    _ => crate::power::LATCH_DRAIN.0 + crate::power::LATCH_DRAIN.1 * c.strength,
                };
                sources.push((capability::power::channel(c.power), rate * CONTACT_DUTY));
                // It also eats the ship's biomass first (K8): a tax on a symbiosis build that
                // the Remora answers, priced as a Drain term the cover cuts.
                let term = role(c.power).weight()
                    * c.strength
                    * tune.buff_extra_flair
                    * tune.latch_biomass_share;
                flair *= 1.0 + term;
                terms.push((capability::power::channel(c.power), term));
                continue;
            }
            Power::Engulf => {
                let rate = crate::power::ENGULF_DPS * (crate::power::ENGULF_DPS_GAIN + c.strength);
                sources.push((capability::power::channel(c.power), rate * CONTACT_DUTY));
                continue;
            }
            _ => {}
        }
        let term = role(c.power).weight() * c.strength;
        flair *= 1.0 + term;
        terms.push((capability::power::channel(c.power), term));
        // The verbs K8 added to glare (a blind sonar) and repel (flung mines) are priced as a
        // second term on the same channel, so a cover cuts both.
        let verb = match c.power {
            Power::Glare => tune.glare_sonar_share,
            Power::Repel => 1.0,
            _ => 0.0,
        };
        if verb > 0.0 {
            let extra = term * tune.buff_extra_flair * verb;
            flair *= 1.0 + extra;
            terms.push((capability::power::channel(c.power), extra));
        }
        match c.power {
            Power::Cloud => cloud = Some((capability::power::channel(c.power), term)),
            Power::Warp if g.warp > 0.0 => haste = c.strength,
            _ => {}
        }
        let (parry, dash) = disables(c.power);
        if parry || dash {
            let params = g.power_params(c.power);
            edges.push(Disable {
                power: c.power,
                parry,
                dash,
                duty: (params.hold / params.period.max(0.1)).clamp(0.0, 1.0),
            });
        }
    }
    let evasion = 1.0 + 0.5 * strafe + 0.5 * blink + 0.5 * phase + 0.25 * (size - 1.0).max(0.0);
    let shield_pool = g.shield * foe.shield.max(0.05);
    let hull_pool = g.hull * foe.hull.max(0.05);
    let pool = shield_pool + hull_pool;
    // The bare ship's gun: one shot of 26 every 0.16 s, cut by plating and divided by threat.
    let shot = Stats::BASE.damage;
    let through = (shot - foe.plating).max(shot * tune.plating_floor) / threat;
    let ttk = pool / (through / Stats::BASE.fire_period) * evasion;
    let bare_pool = Stats::BASE.max_hull + Stats::BASE.max_shield;
    let ship_dps = sources.iter().map(|&(_, d)| d).sum::<f32>().max(0.01);
    let ttk_ship = bare_pool / ship_dps;
    let agility = (speed / 460.0).clamp(0.2, 3.0);
    let core = (ttk / ttk_ship).sqrt() * agility.powf(0.3);
    let power = core * flair;
    Organism {
        class,
        name: species.name(),
        lineage: species.lineage,
        weapon: g.weapon,
        volley: g.volley,
        parts: g.parts(),
        armed_parts: armed,
        copies,
        radius: g.radius,
        threat,
        sharpness: sharp,
        hull: g.hull,
        shield: g.shield,
        pool,
        plating: foe.plating,
        adaptive: pool >= tune.adapt_min_pool,
        shot_damage,
        shots,
        period,
        period_enraged,
        max_hit,
        contact_hit,
        volley_damage,
        burst_potential,
        burst_expected,
        dps,
        pith,
        range: reach,
        sight: g.sight * phenotype.sensor_acuity,
        shot_speed: flight_speed(g.weapon, g.shot_speed),
        flight_time: reach / flight_speed(g.weapon, g.shot_speed),
        telegraph,
        speed,
        cruise,
        accel,
        evasion,
        powers,
        trigger: g.trigger,
        ttk,
        ttk_ship,
        power,
        hostility: class.hostility(g.trigger),
        core,
        flair: terms,
        weapon_channel: capability::weapon::channel(g.weapon),
        disables: edges,
        sources,
        cloud,
        haste,
    }
}

/// What a carried power does to the ship, for the organism's power index. Exhaustive on
/// purpose: a new `Power` does not compile until it is classified here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Hurts, pulls or hides the damage: bypass, cloud, devour, rune, rift, sling, latch.
    Damage,
    /// Takes the ship's options: emp, glare, song, confuse, weave, engulf, dim, repel.
    Control,
    /// Makes it hard to hit or to escape: blink, phase, warp.
    Mobility,
    /// Hides it or its attack: mimic, lens.
    Stealth,
    /// Feeds or multiplies the organism: symbiote, split.
    Support,
}

impl Role {
    /// Share of the organism's power index one full-strength power of the role adds.
    pub fn weight(self) -> f32 {
        match self {
            Self::Damage => 0.20,
            Self::Control => 0.15,
            Self::Mobility => 0.10,
            Self::Stealth => 0.10,
            Self::Support => 0.05,
        }
    }
}

/// The role of a power.
pub fn role(power: Power) -> Role {
    match power {
        Power::Bypass
        | Power::Cloud
        | Power::Devour
        | Power::Rune
        | Power::Rift
        | Power::Sling
        | Power::Latch => Role::Damage,
        Power::Emp
        | Power::Glare
        | Power::Song
        | Power::Confuse
        | Power::Weave
        | Power::Engulf
        | Power::Dim
        | Power::Repel => Role::Control,
        Power::Blink | Power::Phase | Power::Warp => Role::Mobility,
        Power::Mimic | Power::Lens => Role::Stealth,
        Power::Symbiote | Power::Split => Role::Support,
    }
}

/// A gun with no genome (a station, a fortress turret, a base's arms).
fn assess_structure(spawn: &Spawn, tune: &Tunables) -> Option<Organism> {
    let (weapon, volley) = spawn.arms?;
    let sharp = spawn.phenotype.sharpness();
    let (base_damage, shots, pace) = weapon_numbers(weapon, volley, tune);
    let aggression = spawn.phenotype.aggression.max(0.2);
    let fort = spawn.fort.is_some();
    let period = if fort { tune.fort_period } else { 2.4 } * pace / aggression;
    let reach = if fort {
        tune.fort_turret_reach
    } else {
        tune.ecology_turret_reach
    };
    let reach_for_budget = if fort {
        tune.fort_turret_reach
    } else {
        tune.ecology_turret_reach
    };
    let scale = crate::simulation::burst::structure_scale(
        tune,
        &spawn.phenotype,
        weapon,
        volley,
        fort,
        reach_for_budget,
    );
    let shot_damage = base_damage * sharp * scale;
    let volley_damage = shot_damage * shots as f32;
    let burst_potential = volley_damage * (1.0 + (WINDOW / period).floor());
    let at = (reach * 0.5).clamp(150.0, 500.0);
    let hits = hit_fraction(weapon, shots, at, SHIP_RADIUS);
    let dps = volley_damage * hits / period;
    let hull = spawn
        .base_kind
        .map_or(crate::world::BaseKind::Turret.hull(), |k| k.hull());
    let threat = spawn.phenotype.threat.max(1.0);
    let pool = hull;
    let ttk = pool / (Stats::BASE.damage / Stats::BASE.fire_period) * threat.sqrt();
    let ttk_ship = (Stats::BASE.max_hull + Stats::BASE.max_shield) / dps.max(0.01);
    let name = match spawn.base_kind {
        Some(k) => k.label().to_string(),
        None => "fort turret".to_string(),
    };
    Some(Organism {
        class: Class::Structure,
        name,
        lineage: 0,
        weapon,
        volley,
        parts: 1,
        armed_parts: 1,
        copies: 1,
        radius: spawn.radius.unwrap_or(40.0),
        threat,
        sharpness: sharp,
        hull,
        shield: 0.0,
        pool,
        plating: 0.0,
        adaptive: false,
        shot_damage,
        shots,
        period,
        period_enraged: period,
        max_hit: shot_damage,
        contact_hit: 0.0,
        volley_damage,
        burst_potential,
        burst_expected: burst_potential * hits,
        dps,
        pith: 0.0,
        range: reach,
        sight: reach,
        shot_speed: if fort { tune.fort_shot_speed } else { 380.0 },
        flight_time: reach / if fort { tune.fort_shot_speed } else { 380.0 },
        telegraph: 0.0,
        speed: 0.0,
        cruise: 0.0,
        accel: 0.0,
        evasion: 1.0,
        powers: Vec::new(),
        trigger: Trigger::Sight,
        ttk,
        ttk_ship,
        power: (ttk / ttk_ship).sqrt(),
        hostility: Class::Structure.hostility(Trigger::Sight),
        core: (ttk / ttk_ship).sqrt(),
        flair: Vec::new(),
        weapon_channel: capability::weapon::channel(weapon),
        disables: Vec::new(),
        cloud: None,
        haste: 0.0,
        sources: vec![(capability::weapon::channel(weapon), dps)],
    })
}

/// The organisms of one spawn (none for rocks and wells). A husk contributes its tenants.
/// An apex elder's range closers need its sector's archetype: see `assess_spawn_in`.
pub fn assess_spawn(spawn: &Spawn, tune: &Tunables) -> Vec<Organism> {
    assess_spawn_in(spawn, None, tune)
}

/// `assess_spawn` for a spawn of a sector whose elder (if any) is of `archetype`: a lunge or
/// a charge is a source of the elder's damage.
pub fn assess_spawn_in(
    spawn: &Spawn,
    archetype: Option<Archetype>,
    tune: &Tunables,
) -> Vec<Organism> {
    let mut out = Vec::new();
    if let (Some((species, count)), true) = (spawn.den, spawn.kind != BodyKind::Creature) {
        out.push(assess_genome(
            Class::Den,
            &species,
            spawn.phenotype,
            u32::from(count),
            None,
            tune,
        ));
    }
    match (spawn.kind, spawn.species) {
        (BodyKind::Creature, Some(species)) => {
            let class = if spawn.apex.is_some() {
                Class::Apex
            } else if spawn.civ.is_some() {
                Class::Civil
            } else {
                Class::Wild
            };
            let closer = archetype.filter(|_| class == Class::Apex);
            out.push(assess_genome(
                class,
                &species,
                spawn.phenotype,
                1,
                closer,
                tune,
            ));
        }
        _ => out.extend(assess_structure(spawn, tune)),
    }
    out
}

/// Damage that is in a sector but is no organism's: a gravity well or maw (the ship in its
/// core takes `dps`) and a herd's sting. Priced like an organism whose ttk is the player's
/// `WINDOW` to answer it, so `power^2 = WINDOW / ttk_ship` with `ttk_ship` the bare ship's
/// pool over the hazard's damage per second while it is landing (`CONTACT_DUTY`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hazard {
    pub name: &'static str,
    pub channel: Channel,
    /// Damage per second to the ship when it is in the hazard, times the duty.
    pub dps: f32,
    /// How readily it goes for the ship (a well always; a herd as its trigger says).
    pub hostility: f32,
}

impl Hazard {
    /// `power^2` against a ship of `tier`, the tier's cover of the channel cutting the damage
    /// (a ballast is immune to a well's core: degree three).
    pub fn weight_for(&self, tier: &Tier) -> f32 {
        let k = 1.0 - f32::from(tier.cover[self.channel.index()]) / 3.0;
        let bare = Stats::BASE.max_hull + Stats::BASE.max_shield;
        self.hostility * WINDOW * self.dps * k / bare
    }
}

/// The hazards of sector `id`: its wells and maws (`well::of_sector`, genome damage rate and
/// all) and the sting of its herd (`herd::plan`, `flock::sting_rate` at the cap).
fn hazards_of(seed: u64, id: SectorId, spawns: &[Spawn], tune: &Tunables) -> Vec<Hazard> {
    let mut out = Vec::new();
    for well in crate::well::of_sector(seed, id, spawns) {
        out.push(Hazard {
            name: if well.genome.mode == crate::well::Mode::Maw {
                "maw"
            } else {
                "well"
            },
            channel: Channel::Field,
            dps: well.genome.dps * CONTACT_DUTY,
            hostility: 1.0,
        });
    }
    if let Some(plan) = crate::herd::plan(seed, id) {
        let g = &plan.species.genome;
        out.push(Hazard {
            name: "herd",
            channel: Channel::Ram,
            dps: g.contact_damage * tune.flock_sting_rate * tune.flock_sting_cap as f32,
            hostility: Class::Wild.hostility(g.trigger),
        });
    }
    out
}

/// One sector's organisms and its danger index.
#[derive(Clone, Debug)]
pub struct SectorReport {
    pub id: SectorId,
    pub ring: u32,
    pub depth: f32,
    pub threat: f32,
    pub realm: &'static str,
    pub organisms: Vec<Organism>,
    /// Wells, maws and the herd's sting: environmental damage that joins the danger.
    pub hazards: Vec<Hazard>,
    /// `sqrt(sum(hostility * copies * power^2))` over organisms and hazards.
    pub danger: f32,
    /// Share of the danger weight (`hostility * copies * power^2`) by channel, at cover 0.
    /// Sums to at most one; the rest is unattributed (unarmed bodies, bonds).
    pub share: [f32; CHANNELS],
    /// Per channel, the worst hostile window burst over the unmitigated reference pool at the
    /// organism's own level (`pool_ref`), counted only where a power of that channel is
    /// present: what the area asks of a build whose ward and parry and dash are gone. At or
    /// above one it is lethal in a window without the answer. Zero where the channel is absent.
    pub gate_burst: [f32; CHANNELS],
}

impl SectorReport {
    /// The organisms that make most of the danger, largest first, with their share.
    pub fn contributors(&self, top: usize) -> Vec<(&Organism, f32)> {
        let weight = |o: &Organism| o.hostility * o.copies as f32 * o.power * o.power;
        let total: f32 = self.organisms.iter().map(weight).sum::<f32>().max(1e-6);
        let mut list: Vec<_> = self
            .organisms
            .iter()
            .map(|o| (o, weight(o) / total))
            .collect();
        list.sort_by(|a, b| b.1.total_cmp(&a.1));
        list.truncate(top);
        list
    }

    /// The channel shares against `tier` (its cover cuts the powers' weight), normalized by the
    /// whole danger weight at that tier, so they sum to at most one.
    pub fn share_for(&self, tier: &Tier) -> [f32; CHANNELS] {
        let mut out = [0.0f32; CHANNELS];
        let mut total = 0.0f32;
        for o in &self.organisms {
            let (w, loose) = o.channel_weight(tier);
            for (a, b) in out.iter_mut().zip(w) {
                *a += b;
            }
            total += w.iter().sum::<f32>() + loose;
        }
        for h in &self.hazards {
            let w = h.weight_for(tier);
            out[h.channel.index()] += w;
            total += w;
        }
        let total = total.max(1e-6);
        out.map(|v| v / total)
    }

    /// The danger index at `tier`: `danger` with every power cut by the tier's cover.
    pub fn danger_for(&self, tier: &Tier) -> f32 {
        let organisms: f32 = self
            .organisms
            .iter()
            .map(|o| {
                let p = o.power_for(tier);
                o.hostility * o.copies as f32 * p * p
            })
            .sum();
        let hazards: f32 = self.hazards.iter().map(|h| h.weight_for(tier)).sum();
        (organisms + hazards).sqrt()
    }

    /// The largest expected window burst of any organism that can fire at the ship.
    pub fn max_burst(&self) -> f32 {
        self.organisms
            .iter()
            .filter(|o| o.hostility >= 0.3)
            .map(|o| o.burst_expected)
            .fold(0.0, f32::max)
    }

    /// Whether any hostile organism can end `tier` inside one window (`expected` counts the
    /// shots that land at half range, else every shot).
    pub fn lethal_for(&self, tier: &Tier, expected: bool) -> bool {
        self.organisms
            .iter()
            .filter(|o| o.hostility >= 0.3)
            .any(|o| {
                if expected {
                    o.burst_ratio(tier) >= 1.0
                } else {
                    o.potential_ratio(tier) >= 1.0
                }
            })
    }
}

/// Measures sector `id` of the world `seed`.
pub fn assess_sector(seed: u64, id: SectorId) -> SectorReport {
    let params = world::latent(seed, id);
    let mut organisms = Vec::new();
    let spawns = world::generate(seed, id);
    let archetype = Some(crate::apex::archetype(seed, id));
    for spawn in &spawns {
        organisms.extend(assess_spawn_in(spawn, archetype, &DEFAULT));
    }
    pair_partners(&mut organisms);
    let hazards = hazards_of(seed, id, &spawns, &DEFAULT);
    let tier = tier_bare();
    let danger = (organisms
        .iter()
        .map(|o| o.hostility * o.copies as f32 * o.power * o.power)
        .sum::<f32>()
        + hazards.iter().map(|h| h.weight_for(&tier)).sum::<f32>())
    .sqrt();
    let (_, kind, _) = realm::identity(seed, id);
    let mut report = SectorReport {
        id,
        ring: ring(id),
        depth: params.depth,
        threat: world::threat(params.depth),
        realm: kind.spec().id,
        organisms,
        hazards,
        danger,
        share: [0.0; CHANNELS],
        gate_burst: [0.0; CHANNELS],
    };
    report.share = report.share_for(&tier);
    report.gate_burst = gate_bursts(&report.organisms);
    report
}

/// A haste time bubble is a support power (CAPABILITIES K8, row 7): it adds nothing alone and
/// speeds the fire of every armed organism in its sector by `1 + WARP_HASTE * strength`.
/// Priced as a partner term: the gun's source and the burst scale by that rate, `ttk_ship`
/// shrinks by the total damage gain, and the power index grows by its square root (the
/// `core = sqrt(ttk / ttk_ship)` law). A sector without a bubble is untouched.
pub fn pair_partners(organisms: &mut [Organism]) {
    let haste = organisms.iter().map(|o| o.haste).fold(0.0f32, f32::max);
    if haste <= 0.0 {
        return;
    }
    let rate = 1.0 + crate::power::WARP_HASTE * haste;
    for o in organisms
        .iter_mut()
        .filter(|o| o.haste <= 0.0 && o.dps > 0.0)
    {
        let total: f32 = o.sources.iter().map(|&(_, d)| d).sum();
        let Some(gun) = o.sources.first().map(|&(_, d)| d) else {
            continue;
        };
        if total <= 0.0 || gun <= 0.0 {
            continue;
        }
        let gain = (total + gun * (rate - 1.0)) / total;
        o.sources[0].1 = gun * rate;
        o.dps *= rate;
        o.burst_potential *= rate;
        o.burst_expected *= rate;
        o.period /= rate;
        o.period_enraged /= rate;
        o.ttk_ship /= gain;
        o.core *= gain.sqrt();
        // The same product `power_for` forms at cover zero, so the two stay bit-identical.
        o.power = o.power_for(&tier_bare());
    }
}

/// `SectorReport::gate_burst`: for each channel some organism carries a power of, the worst
/// hostile burst of the sector over its unmitigated reference pool. A disabling power makes
/// the whole sector's guns the burst of its channel (the carrier fires with its group).
fn gate_bursts(organisms: &[Organism]) -> [f32; CHANNELS] {
    let mut out = [0.0f32; CHANNELS];
    let mut present = [false; CHANNELS];
    for o in organisms.iter().filter(|o| is_hostile(o)) {
        for &(c, _) in &o.flair {
            if let Some(c) = c {
                present[c.index()] = true;
            }
        }
    }
    let worst = organisms
        .iter()
        .filter(|o| is_hostile(o))
        .map(|o| o.burst_expected / crate::simulation::burst::pool_ref(o.threat, o.pith, &DEFAULT))
        .fold(0.0f32, f32::max);
    for c in Channel::ALL {
        if present[c.index()] {
            out[c.index()] = worst;
        }
    }
    out
}

/// The sectors of Moore ring `r`, at most `limit` of them spread evenly around it.
pub fn ring_sectors(r: i32, limit: usize) -> Vec<SectorId> {
    if r == 0 {
        return vec![SectorId::ORIGIN];
    }
    let mut cells = Vec::new();
    for x in -r..=r {
        cells.push(SectorId { x, y: -r });
        cells.push(SectorId { x, y: r });
    }
    for y in (-r + 1)..r {
        cells.push(SectorId { x: -r, y });
        cells.push(SectorId { x: r, y });
    }
    cells.sort_by_key(|c| (c.x, c.y));
    if cells.len() <= limit {
        return cells;
    }
    (0..limit).map(|k| cells[k * cells.len() / limit]).collect()
}

/// A distribution summary.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dist {
    pub n: usize,
    pub min: f32,
    pub p50: f32,
    pub p90: f32,
    pub p99: f32,
    pub max: f32,
    pub mean: f32,
}

impl Dist {
    pub fn of(mut values: Vec<f32>) -> Self {
        values.retain(|v| v.is_finite());
        if values.is_empty() {
            return Self::default();
        }
        values.sort_by(f32::total_cmp);
        let at = |p: f32| values[((values.len() - 1) as f32 * p).round() as usize];
        Self {
            n: values.len(),
            min: values[0],
            p50: at(0.5),
            p90: at(0.9),
            p99: at(0.99),
            max: values[values.len() - 1],
            mean: values.iter().sum::<f32>() / values.len() as f32,
        }
    }
}

/// Rings and sectors per ring of the checked-in baseline (`threat_baseline.txt`).
pub const BASELINE_RINGS: [i32; 14] = [0, 1, 2, 3, 4, 5, 6, 8, 10, 14, 20, 30, 50, 80];
pub const BASELINE_PER_RING: usize = 8;

/// Hostile and armed (or biting): the organisms the budgets are about.
pub fn is_hostile(o: &Organism) -> bool {
    o.hostility >= 0.3 && (o.armed_parts > 0 || o.contact_hit > 0.0)
}

/// What a ship of `tier` is up against in `report`'s worst organism: the largest expected
/// window burst over the tier's pool, and the same for one volley with every shot landing.
pub fn alpha(report: &SectorReport, tier: &Tier) -> (f32, f32) {
    let mut window = 0.0f32;
    let mut volley = 0.0f32;
    for o in report.organisms.iter().filter(|o| is_hostile(o)) {
        let pool = tier.pool(o.pith);
        window = window.max(o.burst_expected / pool);
        volley = volley.max(o.volley_damage * o.armed_parts as f32 / pool);
    }
    (window, volley)
}

/// The baseline table: one line per ring of the sectors `BASELINE_RINGS` x `BASELINE_PER_RING`
/// of the master seed, rounded so it is stable. `SSC_BLESS=1 cargo test --no-default-features
/// threat_baseline` rewrites `src/threat_baseline.txt` after a deliberate change.
pub fn baseline_text() -> String {
    use std::fmt::Write;
    let bare = tier_bare();
    let mut out = String::from(
        "# ring sectors hostile | danger p50 max | power p99 max | window/bare-pool max | volley/bare-pool max | max-hit max | speed/460 p50 p90 max\n",
    );
    for r in BASELINE_RINGS {
        let reports: Vec<SectorReport> = ring_sectors(r, BASELINE_PER_RING)
            .into_iter()
            .map(|id| assess_sector(crate::config::MASTER_SEED, id))
            .collect();
        let all: Vec<&Organism> = reports.iter().flat_map(|s| &s.organisms).collect();
        let hostile: Vec<&Organism> = all.iter().copied().filter(|o| is_hostile(o)).collect();
        let danger = Dist::of(reports.iter().map(|s| s.danger).collect());
        let power = Dist::of(hostile.iter().map(|o| o.power).collect());
        let hit = Dist::of(hostile.iter().map(|o| o.max_hit).collect());
        let speed = Dist::of(
            all.iter()
                .filter(|o| o.class != Class::Structure)
                .map(|o| o.speed / 460.0)
                .collect(),
        );
        let (mut window, mut volley) = (0.0f32, 0.0f32);
        for s in &reports {
            let (w, v) = alpha(s, &bare);
            window = window.max(w);
            volley = volley.max(v);
        }
        let _ = writeln!(
            out,
            "{r} {} {} | {:.2} {:.2} | {:.2} {:.2} | {:.2} | {:.2} | {:.1} | {:.2} {:.2} {:.2}",
            reports.len(),
            hostile.len(),
            danger.p50,
            danger.max,
            power.p99,
            power.max,
            window,
            volley,
            hit.max,
            speed.p50,
            speed.p90,
            speed.max
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MASTER_SEED;
    use bevy::prelude::Vec2;

    #[test]
    fn the_bare_ship_rates_one_and_tiers_rise_with_kit_and_grade() {
        let bare = tier_bare();
        assert!((bare.power - 1.0).abs() < 1e-4);
        let typical = tier_at(MASTER_SEED, 6.0, false, false);
        let maxed = tier_at(MASTER_SEED, 6.0, true, false);
        let graded = tier_at(MASTER_SEED, 6.0, true, true);
        assert!(typical.power >= bare.power);
        assert!(maxed.power > typical.power);
        assert!(graded.power > maxed.power);
        assert!(graded.pool(0.0) > maxed.pool(0.0));
    }

    #[test]
    fn a_needle_blaster_is_held_to_the_burst_budget() {
        let mut species = Species::bogey();
        species.genome.weapon = Weapon::Needles;
        species.genome.volley = 96;
        species.genome.weapon_range = 600.0;
        let mut spawn = Spawn::creature(species, Vec2::ZERO);
        spawn.index = 1;
        let organisms = assess_spawn(&spawn, &DEFAULT);
        assert_eq!(organisms.len(), 1);
        let needle = &organisms[0];
        assert!(needle.shot_damage < 3.0, "each needle is weak alone");
        let bare = tier_bare();
        // Still a spray (every needle lighter), but a window never beats the budget.
        assert_eq!(needle.shots, 96);
        let volley = needle.volley_damage * needle.armed_parts as f32 / bare.pool(0.0);
        assert!(volley <= DEFAULT.balance_volley_cap + 1e-3, "{volley}");
        assert!(
            needle.potential_ratio(&bare) <= DEFAULT.balance_window_cap * 3.0,
            "{}",
            needle.potential_ratio(&bare)
        );
        assert!(needle.burst_ratio(&bare) <= DEFAULT.balance_window_cap + 1e-3);
        assert!(
            needle.max_hit < 10.0,
            "no single hit comes close to the pool"
        );
    }

    #[test]
    fn home_holds_no_hunter_that_can_alpha_the_bare_ship() {
        let report = assess_sector(MASTER_SEED, SectorId::ORIGIN);
        assert!(!report.lethal_for(&tier_bare(), false));
        assert_eq!(
            report.danger,
            assess_sector(MASTER_SEED, SectorId::ORIGIN).danger
        );
    }

    #[test]
    fn danger_is_deterministic_and_rings_enumerate_exactly() {
        assert_eq!(ring_sectors(0, 5), vec![SectorId::ORIGIN]);
        assert_eq!(ring_sectors(1, 100).len(), 8);
        assert_eq!(ring_sectors(3, 100).len(), 24);
        assert_eq!(ring_sectors(7, 10).len(), 10);
        let a = assess_sector(MASTER_SEED, SectorId { x: 4, y: -2 });
        let b = assess_sector(MASTER_SEED, SectorId { x: 4, y: -2 });
        assert_eq!(a.danger, b.danger);
        assert_eq!(a.ring, 4);
    }

    #[test]
    fn distributions_summarize_in_order() {
        let d = Dist::of(vec![5.0, 1.0, 3.0, 2.0, 4.0]);
        assert_eq!((d.n, d.min, d.p50, d.max), (5, 1.0, 3.0, 5.0));
        assert!(d.p90 <= d.p99 && d.p99 <= d.max);
        assert_eq!(Dist::of(Vec::new()), Dist::default());
    }

    // ---- capability cover (docs/CAPABILITIES.md 5.3) -------------------------------------

    fn sampled() -> Vec<SectorReport> {
        [0, 2, 5, 8, 14, 30]
            .into_iter()
            .flat_map(|r| ring_sectors(r, 6))
            .map(|id| assess_sector(MASTER_SEED, id))
            .collect()
    }

    #[test]
    fn cover_zero_prices_exactly_as_before() {
        let bare = tier_bare();
        assert!(bare.cover.iter().all(|&c| c == 0));
        for s in sampled() {
            assert_eq!(s.danger_for(&bare), s.danger);
            for o in &s.organisms {
                assert_eq!(o.power_for(&bare), o.power, "{}", o.name);
                if o.pith == 0.0 {
                    assert_eq!(o.burst_ratio_for(&bare), o.burst_ratio(&bare));
                }
            }
        }
    }

    #[test]
    fn cover_cuts_flair_and_shares_stay_bounded() {
        let bare = tier_bare();
        let warded = bare.warded();
        let mut total_cut = 0;
        for s in sampled() {
            let sum: f32 = s.share.iter().sum();
            assert!(sum <= 1.0 + 1e-3, "{:?} shares sum to {sum}", s.id);
            assert!(s.danger_for(&warded) <= s.danger + 1e-4);
            if s.danger_for(&warded) < s.danger - 1e-4 {
                total_cut += 1;
            }
            for o in &s.organisms {
                assert!(o.power_for(&warded) <= o.power + 1e-4);
                assert!(o.power_for(&warded) >= o.core - 1e-4);
            }
            for c in Channel::ALL {
                if s.gate_burst[c.index()] > 0.0 {
                    assert!(
                        s.organisms
                            .iter()
                            .any(|o| o.flair.iter().any(|&(k, _)| k == Some(c)))
                    );
                }
            }
        }
        assert!(total_cut > 0, "wards should cheapen some sampled sector");
        let mut full = bare.clone();
        full.cover = [3; CHANNELS];
        for s in sampled() {
            for o in &s.organisms {
                // Degree 3 on every channel removes every mapped flair term.
                let mapped_only_core_and_loose: f32 = o
                    .flair
                    .iter()
                    .filter(|(c, _)| c.is_none())
                    .fold(1.0, |a, &(_, t)| a * (1.0 + t));
                let want = o.core * mapped_only_core_and_loose;
                assert!((o.power_for(&full) - want).abs() <= want * 1e-4 + 1e-6);
            }
        }
    }

    #[test]
    fn a_disabling_power_exposes_what_parry_and_dash_cover() {
        let mut species = Species::bogey();
        species.genome.weapon = Weapon::Projectile;
        species.genome.volley = 3;
        Power::Emp.set(&mut species.genome, 1.0);
        let mut spawn = Spawn::creature(species, Vec2::ZERO);
        spawn.index = 1;
        let o = assess_spawn(&spawn, &DEFAULT).remove(0);
        assert!(
            o.disables
                .iter()
                .any(|d| d.power == Power::Emp && d.parry && d.dash && d.duty > 0.0),
            "{:?}",
            o.disables
        );
        let skilled = tier_bare().skilled();
        let mut plain = o.clone();
        plain.disables.clear();
        let with_edge = o.burst_ratio_for(&skilled);
        let without = plain.burst_ratio_for(&skilled);
        assert!(with_edge > without, "{with_edge} vs {without}");
        // The skills still help against the unjammed volley, and never beyond the full cut.
        assert!(without < o.burst_ratio(&skilled));
        let m = skilled.mitigation(0.0, 0.0);
        assert!(m > 0.0 && m < 1.0);
        assert!(with_edge <= o.burst_ratio(&skilled) + 1e-6);
    }

    // ---- coverage: adding a feature must update the model -------------------------------

    use crate::genome::Genome;
    use crate::simulation::tunables::Registry;
    use crate::simulation::tuning::Tunables as Tun;

    /// Every `Genome::genes()` entry as of the last review of the threat model. A new gene
    /// (an attack parameter, a cloak, a status effect) changes the count and fails
    /// `new_genes_force_a_model_review`: decide whether it feeds `assess_genome`, then bump.
    const REVIEWED_GENES: usize = 149;

    /// The enemy-damage channels in the tunables registry and whether `assess_*` models them.
    /// A new entry matching `DAMAGE_WORDS` fails `new_damage_tunables_force_a_model_review`.
    /// `Gap` entries are known unmodelled channels, listed in docs/BALANCE.md; closing one
    /// means changing it to `Modelled` in the same commit as the model.
    const DAMAGE_CHANNELS: &[(&str, Coverage)] = &[
        ("weapon_pellet_damage", Coverage::Modelled),
        ("weapon_needle_damage", Coverage::Modelled),
        ("weapon_missile_damage", Coverage::Modelled),
        ("weapon_orb_damage", Coverage::Modelled),
        ("weapon_spiral_damage", Coverage::Modelled),
        ("weapon_mine_damage", Coverage::Modelled),
        ("barrage_shots_calm", Coverage::Modelled),
        ("barrage_shots_enraged", Coverage::Modelled),
        ("barrage_share", Coverage::Modelled),
        ("lunge_speed", Coverage::Modelled),
        ("elder_charge_speed", Coverage::Modelled),
        ("elder_enrage_sting", Coverage::Modelled),
        ("world_ram_damage", Coverage::NotEnemyOfTheShip),
        ("strike_damage", Coverage::NotEnemyOfTheShip),
        ("food_bite_damage", Coverage::NotEnemyOfTheShip),
        ("food_bite_period", Coverage::NotEnemyOfTheShip),
        ("fauna_bite", Coverage::NotEnemyOfTheShip),
        ("fauna_bite_per_contact", Coverage::NotEnemyOfTheShip),
        ("fauna_bite_period", Coverage::NotEnemyOfTheShip),
        ("fauna_bite_reach", Coverage::NotEnemyOfTheShip),
        ("flock_sting_cap", Coverage::Modelled),
        ("flock_sting_rate", Coverage::Modelled),
        ("tether_cord_bullet_damage", Coverage::NotEnemyOfTheShip),
        ("tether_link_damage", Coverage::Modelled),
        ("gen_well_dps_lo", Coverage::Modelled),
        ("gen_well_dps_hi", Coverage::Modelled),
        ("gen_well_maw_dps_lo", Coverage::Modelled),
        ("gen_well_maw_dps_hi", Coverage::Modelled),
        ("pad_siege_dps", Coverage::NotEnemyOfTheShip),
        ("pad_reload_raid_damage_min", Coverage::NotEnemyOfTheShip),
        ("pad_reload_raid_damage_max", Coverage::NotEnemyOfTheShip),
        ("fleet_weapon_damage", Coverage::NotEnemyOfTheShip),
        ("recoil_per_damage", Coverage::NotEnemyOfTheShip),
    ];
    const DAMAGE_WORDS: &[&str] = &[
        "damage",
        "sting",
        "bite",
        "dps",
        "barrage_shots",
        "barrage_share",
        "lunge_speed",
        "charge_speed",
    ];

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Coverage {
        Modelled,
        /// Kept for the next unmodelled channel (docs/BALANCE.md 3.7 lists the ones open).
        #[allow(dead_code)]
        Gap,
        NotEnemyOfTheShip,
    }

    #[test]
    fn every_weapon_and_power_is_classified() {
        for &weapon in Weapon::ALL.iter() {
            let (damage, shots, pace) = weapon_numbers(weapon, 3, &DEFAULT);
            assert!(pace > 0.0 && flight_speed(weapon, 300.0) > 0.0);
            let fired = !matches!(weapon, Weapon::None | Weapon::Tether);
            assert_eq!(
                fired,
                damage > 0.0 && shots > 0,
                "{weapon:?} has no damage numbers"
            );
            assert!(hit_fraction(weapon, 3, 300.0, SHIP_RADIUS) >= 0.0);
        }
        for power in Power::ALL {
            assert!(role(power).weight() > 0.0, "{power:?} has no role");
        }
    }

    #[test]
    fn new_genes_force_a_model_review() {
        let genes = Genome::default().genes().len();
        assert_eq!(
            genes, REVIEWED_GENES,
            "a gene was added or removed: decide whether it feeds threat::assess_genome \
             (attack, speed, range, durability, status effect, cloak), then update REVIEWED_GENES"
        );
    }

    #[test]
    fn new_damage_tunables_force_a_model_review() {
        let names: Vec<&str> = <Tun as Registry>::table().iter().map(|i| i.name).collect();
        for name in &names {
            if DAMAGE_WORDS.iter().any(|w| name.contains(w)) {
                assert!(
                    DAMAGE_CHANNELS.iter().any(|(n, _)| n == name),
                    "{name} looks like a damage channel: model it in threat.rs or list it in DAMAGE_CHANNELS"
                );
            }
        }
        for (name, _) in DAMAGE_CHANNELS {
            assert!(
                names.contains(name),
                "{name} is gone or renamed: update DAMAGE_CHANNELS"
            );
        }
        assert!(
            DAMAGE_CHANNELS
                .iter()
                .filter(|(_, c)| *c == Coverage::Modelled)
                .count()
                >= 9
        );
    }

    // ---- baseline and budgets ------------------------------------------------------------

    const BASELINE_FILE: &str = "src/threat_baseline.txt";

    #[test]
    fn threat_baseline_matches_the_checked_in_table() {
        let now = baseline_text();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(BASELINE_FILE);
        if std::env::var_os("SSC_BLESS").is_some() {
            std::fs::write(&path, &now).expect("write the baseline");
            return;
        }
        let pinned = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            now, pinned,
            "threat baseline changed. If the generation or balance change is deliberate, \
             run `SSC_BLESS=1 cargo test --no-default-features threat_baseline` and explain why"
        );
    }

    /// Ratchets: limits the current game meets, to be tightened as balance slices land. The
    /// `target_*` tests below hold the intended limits and are ignored until they are met.
    #[test]
    fn inner_rings_never_alpha_the_bare_ship_and_speeds_stay_bounded() {
        let bare = tier_bare();
        for r in 0..=4 {
            for id in ring_sectors(r, 12) {
                let report = assess_sector(MASTER_SEED, id);
                let (window, volley) = alpha(&report, &bare);
                assert!(
                    window <= 0.75,
                    "ring {r} {id:?}: window burst is {window:.2} of the bare pool"
                );
                assert!(
                    volley <= 1.5,
                    "ring {r} {id:?}: one volley is {volley:.2} of the bare pool"
                );
            }
        }
        let mut speeds = Vec::new();
        for r in 1..=30 {
            for id in ring_sectors(r, 4) {
                for o in assess_sector(MASTER_SEED, id).organisms {
                    if o.class != Class::Structure {
                        speeds.push(o.speed / 460.0);
                    }
                }
            }
        }
        let d = Dist::of(speeds);
        assert!(
            d.p50 > 0.15 && d.p50 < 0.6,
            "median speed {:.2} of the ship",
            d.p50
        );
        assert!(d.max < 1.5, "fastest organism {:.2} of the ship", d.max);
    }

    /// The budget of docs/BALANCE.md 5.4 holds for every hostile organism of every sampled
    /// sector (rings 0 to 80): one volley within the volley cap and the expected window within
    /// the window cap of the reference pool at the organism's own level.
    #[test]
    fn bursts_stay_within_the_caps_of_the_reference_pool() {
        for r in (0..=14).chain([20, 30, 50, 80]) {
            for id in ring_sectors(r, 8) {
                for o in assess_sector(MASTER_SEED, id).organisms {
                    if !is_hostile(&o) || o.class == Class::Apex {
                        continue;
                    }
                    let pool = crate::simulation::burst::pool_ref(o.threat, o.pith, &DEFAULT);
                    let volley = o.volley_damage * o.armed_parts as f32;
                    assert!(
                        volley <= DEFAULT.balance_volley_cap * pool * 1.001,
                        "ring {r} {id:?} {}: volley {volley:.0} of {pool:.0}",
                        o.name
                    );
                    assert!(
                        o.burst_expected <= DEFAULT.balance_window_cap * pool * 1.001,
                        "ring {r} {id:?} {}: window {:.0} of {pool:.0}",
                        o.name,
                        o.burst_expected
                    );
                }
            }
        }
    }

    /// Target: no wild hunter ends a typical kit in one window anywhere up to ring 14.
    #[test]
    #[ignore = "waits on docs/BALANCE.md change 2: the typical kit's pool is 0.15 to 1.0 of pool_ref, so a capped burst still beats it deep down"]
    fn target_no_wild_alpha_against_a_typical_kit() {
        for r in 0..=14 {
            let tier = tier_at(MASTER_SEED, r as f32, false, false);
            for id in ring_sectors(r, 12) {
                let report = assess_sector(MASTER_SEED, id);
                let (window, volley) = alpha(&report, &tier);
                assert!(
                    window <= 1.0 && volley <= 0.5,
                    "ring {r} {id:?}: window {window:.2} volley {volley:.2}"
                );
            }
        }
    }

    /// Target: speed diversity, a real tail of fast organisms next to the slow.
    #[test]
    #[ignore = "target of docs/BALANCE.md change 3: speed diversity"]
    fn target_speeds_are_diverse() {
        let mut speeds = Vec::new();
        for r in 2..=30 {
            for id in ring_sectors(r, 6) {
                for o in assess_sector(MASTER_SEED, id).organisms {
                    if o.class == Class::Wild && is_hostile(&o) {
                        speeds.push(o.speed / 460.0);
                    }
                }
            }
        }
        let fast = speeds.iter().filter(|s| **s >= 0.75).count() as f32 / speeds.len() as f32;
        assert!(
            fast >= 0.10,
            "only {fast:.2} of hostile wild organisms reach 0.75 of ship speed"
        );
    }

    // ---- the K8 buffs (docs/CAPABILITIES.md section 6) ---------------------------------

    fn lone(genome: crate::genome::Genome) -> Organism {
        let mut species = Species::bogey();
        species.genome = genome;
        let mut spawn = Spawn::creature(species, Vec2::ZERO);
        spawn.index = 1;
        assess_spawn(&spawn, &DEFAULT).remove(0)
    }

    #[test]
    fn glare_repel_and_latch_price_their_new_verb_as_a_second_term_on_the_same_channel() {
        use crate::genome::Genome;
        for (genome, power) in [
            (Genome::argus(), Power::Glare),
            (Genome::pushwhale(), Power::Repel),
            (Genome::hullworm(), Power::Latch),
        ] {
            let o = lone(genome);
            let channel = capability::power::channel(power);
            let on: Vec<f32> = o
                .flair
                .iter()
                .filter(|(c, _)| *c == channel)
                .map(|&(_, t)| t)
                .collect();
            assert!(!on.is_empty(), "{power:?} has a term on its channel");
            assert!(o.power > o.core, "{power:?} carries flair");
            let mut off = DEFAULT;
            off.buff_extra_flair = 0.0;
            let mut species = Species::bogey();
            species.genome = genome;
            let mut spawn = Spawn::creature(species, Vec2::ZERO);
            spawn.index = 1;
            let plain = assess_spawn(&spawn, &off).remove(0);
            assert!(
                o.power > plain.power,
                "{power:?}: {} > {}",
                o.power,
                plain.power
            );
            // A ward at degree 3 on the channel removes both terms.
            let mut tier = tier_bare();
            tier.cover[channel.unwrap().index()] = 3;
            assert!(o.power_for(&tier) < o.power);
        }
    }

    #[test]
    fn a_cloud_prices_higher_against_a_needle_kit_and_nothing_else_moves() {
        let o = lone(crate::genome::Genome::murmur());
        assert!(o.cloud.is_some());
        let bare = tier_bare();
        assert_eq!(o.power_for(&bare), o.power);
        let needles = bare.needled();
        assert!(o.power_for(&needles) > o.power_for(&bare));
        // A kit that covers the channel cuts the extra with the rest.
        let mut warded = needles.clone();
        warded.cover = [3; CHANNELS];
        assert!(o.power_for(&warded) <= o.power_for(&needles));
        let plain = lone(crate::genome::Genome::bogey());
        assert_eq!(plain.power_for(&needles), plain.power_for(&bare));
    }

    #[test]
    fn a_haste_bubble_is_a_partner_term_that_speeds_armed_kin_and_a_slow_one_is_not() {
        use crate::genome::Genome;
        let hasty = lone(Genome {
            warp: 0.8,
            ..Genome::tarbloom()
        });
        assert!(hasty.haste > 0.0);
        let slow = lone(Genome::tarbloom());
        assert_eq!(slow.haste, 0.0);
        let gunner = lone(Genome::bogey());
        assert!(gunner.dps > 0.0);
        let mut kin = vec![gunner.clone(), hasty.clone()];
        pair_partners(&mut kin);
        let rate = 1.0 + crate::power::WARP_HASTE * hasty.haste;
        assert!((kin[0].dps / gunner.dps - rate).abs() < 1e-4);
        assert!(kin[0].power > gunner.power);
        assert!(kin[0].burst_expected > gunner.burst_expected);
        assert!(kin[0].period < gunner.period);
        assert_eq!(kin[1].power, hasty.power, "the carrier itself is unchanged");
        assert_eq!(kin[0].power, kin[0].power_for(&tier_bare()));
        let mut calm = vec![gunner.clone(), slow];
        pair_partners(&mut calm);
        assert_eq!(calm[0].power, gunner.power);
        assert_eq!(calm[0].dps, gunner.dps);
    }
}
