//! Creature genomes, species and gene pools.
//!
//! A `Genome` is a flat bundle of bounded genes (continuous, integer or categorical) that
//! fully describes a creature: body plan, size, senses, social life, temperament,
//! weapons and ecology. The simulation reads genes and never asks what "kind" of
//! creature something is. A `Species` is a lineage (a stable identity that recurs across
//! sectors with mutation) plus its genome. A `GenePool` is the set of species alive in
//! one sector, built from founders on a coarse lattice so neighboring sectors share
//! ancestry and drift gradually, the way the latent parameters do.
//!
//! Everything here is a pure function of the master seed and sector coordinates, and it
//! draws from its own salted streams, never from the stream that places the original
//! population.

use crate::bodyplan;
use crate::grammar::{GRAMMAR_SALT, GrammarSpecimen};
use crate::power::{self, POWER_GENES};
use crate::world::{Rng, SectorParams, hash2};

/// No creature has more bodies than this, however its genes combine.
pub const MAX_PARTS: u32 = 28;
/// Largest fractional jitter of an ordinary individual, and of the wider 4% tail.
pub const JITTER_SMALL: f32 = 0.05;
pub const JITTER_WIDE: f32 = 0.25;
/// Chance an individual also carries one outlier gene.
pub const OUTLIER_CHANCE: f32 = 0.01;
/// Offspring mutation: ordinary jitter, the rare wider jitter and its chance per birth, and
/// the chance per birth that a social, trigger or fear category flips.
pub const MUTATION_SMALL: f32 = 0.01;
pub const MUTATION_RARE: f32 = 0.08;
pub const MUTATION_RARE_CHANCE: f32 = 0.03;
pub const MUTATION_FLIP_CHANCE: f32 = 0.002;
/// Separates individual variation from every other stream.
pub const INDIVIDUAL_SALT: u64 = 0x1D1F_A11E_0000_0011;
/// Extra fling strength a negative-mass body has on top of its fling gene.
pub const NEGATIVE_MASS_FLING: f32 = 0.6;

pub enum Gene<'a> {
    Real { v: &'a mut f32, lo: f32, hi: f32 },
    Int { v: &'a mut u8, lo: u8, hi: u8 },
    Cat { v: &'a mut dyn Categorical },
}

pub trait Categorical {
    fn count(&self) -> u8;
    fn get(&self) -> u8;
    fn set(&mut self, index: u8);
}

macro_rules! categorical {
    ($(#[$m:meta])* $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }
        impl Categorical for $name {
            fn count(&self) -> u8 { Self::ALL.len() as u8 }
            fn get(&self) -> u8 { Self::ALL.iter().position(|v| v == self).unwrap_or(0) as u8 }
            fn set(&mut self, index: u8) {
                *self = Self::ALL[usize::from(index) % Self::ALL.len()];
            }
        }
    };
}

categorical! {
    /// How a creature lives among its own kind.
    Social { Solitary, School, Pack, Brood, Dweller }
}
categorical! {
    /// What first sets a creature off.
    Trigger { Sight, Proximity, Harm }
}
categorical! {
    /// Ranged attack, if any. Contact fling is the continuous `fling` gene. The `volley`
    /// gene sizes each pattern: shots per fan, mines per drop, missiles per salvo, needles
    /// per burst, bullets per ring, arms of a spiral.
    Weapon { None, Projectile, Tether, Mine, Missile, Needles, Nova, Spiral }
}
categorical! {
    /// What it eats or harvests. `Graze` browses drifting plankton; `Hunt` eats smaller
    /// creatures of other lineages it touches. New diets are appended so older indices hold.
    Diet { None, Rocks, Siphon, Dust, Graze, Hunt }
}
categorical! {
    /// What it flees from.
    Fear { None, Player, Bullets, Wells }
}
categorical! {
    /// Where it makes its home.
    Nest { None, Rocks, Base }
}
categorical! {
    /// How it reproduces: a live juvenile beside the parent, or a drifting egg that hatches.
    /// Appended after every older gene, so earlier genes and their draws are unchanged.
    Birth { Live, Egg }
}
categorical! {
    /// How often an adult reproduces when conditions allow.
    Fecundity { Rare, Steady, Prolific }
}

macro_rules! genome {
    (
        real { $($rf:ident: $lo:expr, $hi:expr, $rd:expr;)* }
        int { $($if_:ident: $ilo:expr, $ihi:expr, $id:expr;)* }
        cat { $($cf:ident: $ct:ident = $cd:ident::$cv:ident;)* }
        tail { $($tf:ident: $tlo:expr, $thi:expr, $td:expr;)* }
        nested { $($nf:ident: $nt:ty = $nd:expr;)* }
    ) => {
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Genome {
            $(pub $rf: f32,)*
            $(pub $if_: u8,)*
            $(pub $cf: $ct,)*
            $(pub $tf: f32,)*
            $(pub $nf: $nt,)*
        }
        impl Default for Genome {
            /// An inert drifter; every authored or sampled genome starts from here.
            fn default() -> Self {
                Self { $($rf: $rd,)* $($if_: $id,)* $($cf: $cd::$cv,)* $($tf: $td,)* $($nf: $nd,)* }
            }
        }
        impl Genome {
            /// Every gene with its bounds, in a fixed order.
            pub fn genes(&mut self) -> Vec<Gene<'_>> {
                vec![
                    $(Gene::Real { v: &mut self.$rf, lo: $lo, hi: $hi },)*
                    $(Gene::Int { v: &mut self.$if_, lo: $ilo, hi: $ihi },)*
                    $(Gene::Cat { v: &mut self.$cf },)*
                    $(Gene::Real { v: &mut self.$tf, lo: $tlo, hi: $thi },)*
                ]
            }
        }
    };
}

genome! {
    real {
        // Body plan.
        stiffness: 30.0, 900.0, 200.0;
        wave: 0.0, 2.5, 0.0;
        rhythm: 0.5, 7.0, 3.0;
        lag: 0.2, 1.8, 0.9;
        taper: 0.4, 1.2, 1.0;
        aspect: 0.6, 1.8, 1.0;
        // Size and mass. Mass may be negative.
        radius: 6.0, 70.0, 14.0;
        mass: -60.0, 300.0, 8.0;
        hull: 10.0, 400.0, 30.0;
        shield: 0.0, 60.0, 0.0;
        // Locomotion and sensing.
        speed: 30.0, 450.0, 120.0;
        cruise: 10.0, 200.0, 50.0;
        lead: 0.0, 1.2, 0.0;
        flocking: 0.0, 2.0, 1.0;
        sight: 150.0, 2200.0, 1000.0;
        lose: 150.0, 2400.0, 1500.0;
        mass_affinity: -1.0, 1.0, 0.0;
        standoff: 0.0, 500.0, 0.0;
        strafe: 0.0, 1.0, 0.0;
        // Social and temperament.
        bond: 0.0, 1.0, 0.0;
        rage: 0.0, 0.8, 0.0;
        alarm: 0.0, 700.0, 300.0;
        // Weapons.
        fire_period: 0.8, 8.0, 3.0;
        shot_speed: 150.0, 700.0, 300.0;
        weapon_range: 200.0, 1000.0, 600.0;
        fling: 0.0, 2.0, 0.0;
        fling_chaos: 0.0, 1.5, 1.0;
        contact_damage: 0.0, 40.0, 6.0;
        reel: 10.0, 150.0, 55.0;
        // Identity.
        bounty: 20.0, 400.0, 100.0;
        hue: 0.0, 1.0, 0.5;
        pale: 0.0, 0.95, 0.3;
        bright: 0.5, 1.0, 0.9;
    }
    int {
        segments: 1, 16, 1;
        limbs: 0, 6, 0;
        limb_len: 1, 3, 1;
        sides: 0, 8, 0;
        hardpoint_every: 0, 6, 0;
        voice0: 0, 23, 0;
        voice1: 0, 11, 0;
        voice2: 0, 19, 0;
        volley: 1, 160, 1;
    }
    cat {
        social: Social = Social::Solitary;
        trigger: Trigger = Trigger::Sight;
        weapon: Weapon = Weapon::None;
        diet: Diet = Diet::None;
        fear: Fear = Fear::None;
        nest: Nest = Nest::None;
        birth: Birth = Birth::Live;
        fecundity: Fecundity = Fecundity::Steady;
    }
    // Real genes appended after every older gene (categoricals included), so the index of
    // each earlier gene, and the noise channel it reads, never moves.
    tail {
        // How much a creature trusts what it has learned of the player's movement over a
        // plain lead (0 = does not learn), and how fast its brain trains.
        learner: 0.0, 1.0, 0.0;
        learn_rate: 0.0, 1.0, 0.5;
        // Rooting. `root` is the habit: below `ROOT_JUVENILE` the creature roams free, up to
        // `ROOT_LIFE` it clings to a rock while young and lets go, and above that it never
        // leaves (see `Genome::habit`). `root_defense` is how hard a rooted creature fights
        // for its place (stinging contact, and armed young keep their weapons). The three
        // detach genes decide what makes a young one let go early: the share of its growth
        // it has reached, hunger at a poor host, or a crowded host.
        root: 0.0, 1.0, 0.0;
        root_defense: 0.0, 1.0, 0.0;
        detach_size: 0.4, 1.0, 1.0;
        detach_hunger: 0.0, 0.5, 0.0;
        detach_crowd: 0.3, 1.0, 1.0;
        // Cords (only tether weapons use them). Strength scales the pull's stiffness and
        // ceiling, slack is how far past its rest length the ship may stretch a cord before it
        // snaps, hardness is the hits it takes to cut, drag the share of the ship's speed
        // away from the owner that it bleeds. The defaults are the classic weak cord.
        cord_strength: 1.0, 8.0, 1.0;
        cord_slack: 200.0, 3000.0, 200.0;
        cord_hardness: 1.0, 10.0, 2.0;
        cord_drag: 0.0, 1.0, 0.0;
        // The rare-power block (see `power`): three shared parameters, then twenty-one
        // intensities, all dormant at zero (below `power::GATE` nothing happens).
        power_period: 1.5, 14.0, 5.0;
        power_reach: 80.0, 900.0, 300.0;
        power_hold: 0.2, 4.0, 1.0;
        phase: 0.0, 1.0, 0.0;
        repel: 0.0, 1.0, 0.0;
        warp: -1.0, 1.0, 0.0;
        lens: 0.0, 1.0, 0.0;
        blink: 0.0, 1.0, 0.0;
        bypass: 0.0, 1.0, 0.0;
        emp: 0.0, 1.0, 0.0;
        glare: 0.0, 1.0, 0.0;
        mimic: 0.0, 1.0, 0.0;
        latch: 0.0, 1.0, 0.0;
        symbiote: 0.0, 1.0, 0.0;
        cloud: 0.0, 1.0, 0.0;
        devour: 0.0, 1.0, 0.0;
        weave: 0.0, 1.0, 0.0;
        song: -1.0, 1.0, 0.0;
        dim: 0.0, 1.0, 0.0;
        rift: 0.0, 1.0, 0.0;
        sling: 0.0, 1.0, 0.0;
        rune: 0.0, 1.0, 0.0;
        split: 0.0, 1.0, 0.0;
        confuse: 0.0, 1.0, 0.0;
    }
    // Structured sections that are not flat genes: not in `genes()`, never drawn by
    // `sample`, and only crossed or mutated when present, so a genome without one keeps
    // every existing draw.
    nested {
        // An optional grammar-grown body (see `bodyplan`): the grammar genome and its plan
        // seed. `None` for every creature that exists in the wild today.
        grammar: Option<GrammarSpecimen> = None;
    }
}

/// `root` below this roams free; from here to `ROOT_LIFE` it is rooted while young.
pub const ROOT_JUVENILE: f32 = 0.4;
pub const ROOT_LIFE: f32 = 0.75;
/// A rooted creature's contact sting is multiplied by one plus its defense gene times this,
/// and young ones keep their weapon from this much defense.
pub const ROOT_STING: f32 = 1.5;
pub const ROOT_ARMED: f32 = 0.4;

/// How a creature relates to the rocks it may cling to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Habit {
    /// Roams free always.
    Free,
    /// Clings to a rock while young and lets go at an age, a hunger or a crowd its genes set.
    Juvenile,
    /// Clings to its rock for life; only the loss of the rock frees it.
    Life,
}

/// The role a creature plays in a sector's population, read from its genes alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Niche {
    /// Passive schoolers that only react to a close approach or an injury.
    School,
    /// Contact flingers.
    Fling,
    /// Hunters that notice the player from afar.
    Hunter,
    /// Slow heavies.
    Heavy,
    /// Creatures that fire cords.
    Tether,
}

const VOICE_FIRST: [&str; 24] = [
    "Bo", "Lu", "Smar", "Fat", "Lee", "Ser", "Vyr", "Sli", "Nax", "Kra", "Spi", "Zor", "Mog",
    "Dar", "Thu", "Quil", "Ish", "Gra", "Wex", "Pyr", "Osk", "Jun", "Eld", "Tor",
];
const VOICE_MIDDLE: [&str; 12] = [
    "", "na", "ra", "mo", "li", "ke", "zu", "ba", "phi", "do", "ve", "sa",
];
const VOICE_LAST: [&str; 20] = [
    "gey", "tic", "ty", "so", "ch", "pent", "thra", "lisk", "dron", "mite", "ling", "ax", "oid",
    "gor", "bat", "wyrm", "fly", "ox", "ine", "urk",
];

/// How big a pattern the weapon gene fires: shots per fan, mines per drop, missiles per
/// salvo, needles per burst, bullets per ring, arms of a spiral.
pub fn volley_for(weapon: Weapon, rng: &mut Rng) -> u8 {
    let count = match weapon {
        Weapon::Projectile if rng.chance(0.25) => rng.int(2, 4),
        Weapon::Mine | Weapon::Missile | Weapon::Spiral => rng.int(1, 3),
        Weapon::Needles => rng.int(48, 128),
        Weapon::Nova => rng.int(8, 18),
        _ => 1,
    };
    count as u8
}

impl Genome {
    /// Bodies in the creature: the spine plus every limb joint, or the stems of its grammar
    /// body when it has one (see `bodyplan`, at most `bodyplan::BODY_PARTS`).
    pub fn parts(&self) -> u32 {
        match &self.grammar {
            Some(spec) => bodyplan::part_count(spec),
            None => self.chain_parts(),
        }
    }

    /// The spine and limb joints alone, which is all `parts` counts without a grammar.
    fn chain_parts(&self) -> u32 {
        u32::from(self.segments) + u32::from(self.limbs) * u32::from(self.limb_len)
    }

    pub fn is_jointed(&self) -> bool {
        self.parts() > 1
    }

    /// Contact fling strength: the gene, plus a push for being negative mass.
    pub fn fling_strength(&self) -> f32 {
        self.fling
            + if self.mass < 0.0 {
                NEGATIVE_MASS_FLING
            } else {
                0.0
            }
    }

    /// How much energy the body can store: it scales with size.
    pub fn energy_capacity(&self) -> f32 {
        30.0 + 2.5 * self.radius
    }

    /// Seconds between this species' reproductions at best.
    pub fn breeding_period(&self) -> f32 {
        match self.fecundity {
            Fecundity::Rare => 300.0,
            Fecundity::Steady => 150.0,
            Fecundity::Prolific => 75.0,
        }
    }

    /// True for diets that must eat to live. Everything else (nothing, siphon, dust) draws
    /// on ambient energy and never tires or starves.
    pub fn forages(&self) -> bool {
        matches!(self.diet, Diet::Rocks | Diet::Graze | Diet::Hunt)
    }

    /// How the creature relates to rocks. A jointed body cannot cling, so a lifelong
    /// rooter that grows a spine is rooted only while young.
    pub fn habit(&self) -> Habit {
        if self.root < ROOT_JUVENILE {
            Habit::Free
        } else if self.root < ROOT_LIFE || self.is_jointed() {
            Habit::Juvenile
        } else {
            Habit::Life
        }
    }

    /// True when a rooting species' young keep their weapon: they have a defense to use.
    pub fn defended_young(&self) -> bool {
        self.habit() != Habit::Free && self.root_defense >= ROOT_ARMED
    }

    /// Physical mass: always positive; the sign is expressed through repulsion.
    pub fn body_mass(&self) -> f32 {
        self.mass.abs().max(2.0)
    }

    /// True when body part `part` (0 is the head) carries a gun or cord launcher.
    pub fn armed(&self, part: u8) -> bool {
        if self.weapon == Weapon::None {
            return false;
        }
        if !self.is_jointed() || self.hardpoint_every == 0 {
            return part == 0;
        }
        part % self.hardpoint_every == self.hardpoint_every - 1
    }

    pub fn niche(&self) -> Niche {
        if self.weapon == Weapon::Tether {
            Niche::Tether
        } else if self.fling_strength() >= 0.5 {
            Niche::Fling
        } else if self.mass.abs() >= 80.0 {
            Niche::Heavy
        } else if matches!(self.social, Social::School | Social::Pack)
            && self.trigger != Trigger::Sight
        {
            Niche::School
        } else {
            Niche::Hunter
        }
    }

    /// Display name, spelled by the three syllable genes.
    pub fn name(&self) -> String {
        let first = VOICE_FIRST[usize::from(self.voice0) % VOICE_FIRST.len()];
        let middle = VOICE_MIDDLE[usize::from(self.voice1) % VOICE_MIDDLE.len()];
        let last = VOICE_LAST[usize::from(self.voice2) % VOICE_LAST.len()];
        format!("{first}{middle}{last}")
    }

    /// Body color from the pigment genes, as sRGB in [0, 1].
    pub fn color(&self) -> [f32; 3] {
        let (h, s, v) = (
            self.hue.rem_euclid(1.0) * 6.0,
            (1.0 - self.pale).clamp(0.0, 1.0),
            self.bright.clamp(0.0, 1.0),
        );
        let channel = |offset: f32| {
            let k = (offset + h).rem_euclid(6.0);
            v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
        };
        [channel(5.0), channel(3.0), channel(1.0)]
    }

    /// Forces every gene into its range and the body count under the cap.
    pub fn limited(mut self) -> Self {
        for gene in self.genes() {
            match gene {
                Gene::Real { v, lo, hi } => *v = if v.is_finite() { v.clamp(lo, hi) } else { lo },
                Gene::Int { v, lo, hi } => *v = (*v).clamp(lo, hi),
                Gene::Cat { .. } => {}
            }
        }
        while self.chain_parts() > MAX_PARTS && self.limbs > 0 {
            self.limbs -= 1;
        }
        while self.chain_parts() > MAX_PARTS && self.segments > 1 {
            self.segments -= 1;
        }
        if let Some(spec) = &mut self.grammar {
            spec.genome = spec.genome.limited();
        }
        self
    }

    /// Every gene scaled to [0, 1] (categoricals by index), for measuring distance.
    pub fn normalized(&self) -> Vec<f32> {
        let mut copy = *self;
        copy.genes()
            .into_iter()
            .map(|gene| match gene {
                Gene::Real { v, lo, hi } => (*v - lo) / (hi - lo),
                Gene::Int { v, lo, hi } => f32::from(*v - lo) / f32::from(hi - lo).max(1.0),
                Gene::Cat { v } => f32::from(v.get()) / f32::from(v.count() - 1).max(1.0),
            })
            .collect()
    }

    /// Mean absolute difference between two genomes in normalized gene space.
    pub fn distance(&self, other: &Self) -> f32 {
        let (a, b) = (self.normalized(), other.normalized());
        a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len() as f32
    }

    /// The genome with every gene nudged by `signed(gene index)`, a value in [-1, 1],
    /// scaled by `amplitude` (zero leaves it untouched). The machinery behind the species'
    /// clines and the offsets of isolated populations (see `range`): real and integer genes shift by a share of their span, categories flip
    /// only at the extremes, and everything passes through `limited()`.
    pub(crate) fn drifted(
        mut self,
        lineage: u64,
        amplitude: f32,
        signed: impl Fn(usize) -> f32,
    ) -> Self {
        if amplitude <= 0.0 {
            return self;
        }
        // The power block never drifts: a species keeps its power (or its lack of one)
        // wherever it lives, and a dormant gene cannot be pushed over its gate.
        let first_power = self.genes().len() - POWER_GENES;
        for (index, gene) in self.genes().into_iter().enumerate() {
            if index >= first_power {
                continue;
            }
            let signed = signed(index).clamp(-1.0, 1.0);
            match gene {
                Gene::Real { v, lo, hi } => {
                    *v = (*v + signed * amplitude * 0.3 * (hi - lo)).clamp(lo, hi);
                }
                Gene::Int { v, lo, hi } => {
                    let shifted = f32::from(*v) + signed * amplitude * 0.3 * f32::from(hi - lo);
                    *v = shifted.round().clamp(f32::from(lo), f32::from(hi)) as u8;
                }
                Gene::Cat { v } => {
                    let count = v.count();
                    if count > 1 && signed.abs() > 1.0 - amplitude * 0.6 {
                        let step =
                            1 + (hash2(lineage, index as i32, 7) % u64::from(count - 1)) as u8;
                        v.set((v.get() + step) % count);
                    }
                }
            }
        }
        self.limited()
    }

    /// One individual of a species: the same genome with per-creature variation, so no two
    /// members of a flock are identical. About 95% of draws are a tiny jitter (up to
    /// `JITTER_SMALL` of a gene's value), about 4% spread wider (up to `JITTER_WIDE`), and
    /// about 1% add an outlier on top of a small jitter: one gene pushed 2 to 3 times up or down, or one
    /// social or temperament category flipped. The caller supplies a stream derived from
    /// the individual's stable identity, so the draw is deterministic and never touches
    /// the streams that place the population. Always passes through `limited()`.
    pub fn individual(self, rng: &mut Rng) -> Self {
        let roll = rng.f32();
        self.individual_from(rng, roll)
    }

    /// `individual` inside a ring: from `power::AWAKEN_RING` on, about 1 in 400 individuals
    /// awaken a Mild or Strange power from the roll the outlier band already took (no extra
    /// draws; below that ring this is exactly `individual`).
    pub fn individual_in(self, rng: &mut Rng, ring: u32) -> Self {
        let roll = rng.f32();
        power::awaken(self.individual_from(rng, roll), roll, ring)
    }

    /// `individual` for a roll already made: below 0.95 a tiny jitter, up to 0.99 a wide one,
    /// above that one outlier gene. Apex elders force the wide end.
    pub fn individual_from(self, rng: &mut Rng, roll: f32) -> Self {
        let outlier = roll >= 1.0 - OUTLIER_CHANCE;
        let spread = if (0.95..1.0 - OUTLIER_CHANCE).contains(&roll) {
            JITTER_WIDE
        } else {
            JITTER_SMALL
        };
        let mut g = self;
        let triangle = |rng: &mut Rng| (rng.f32() + rng.f32() - 1.0) * spread;
        // Scale-like genes vary by a fraction of their value.
        for v in [
            &mut g.speed,
            &mut g.cruise,
            &mut g.sight,
            &mut g.lose,
            &mut g.fire_period,
            &mut g.radius,
            &mut g.flocking,
            &mut g.lead,
            &mut g.hull,
            &mut g.shot_speed,
            &mut g.weapon_range,
            &mut g.alarm,
        ] {
            *v *= 1.0 + triangle(rng);
        }
        // Genes that rest at zero for creatures without the trait only vary where present.
        for v in [&mut g.standoff, &mut g.strafe] {
            if *v > 0.0 {
                *v *= 1.0 + triangle(rng);
            }
        }
        // Pigment shifts by a fraction of its whole range.
        g.hue = (g.hue + triangle(rng) * 0.5).rem_euclid(1.0);
        g.pale += triangle(rng) * 0.5;
        g.bright += triangle(rng) * 0.5;
        if outlier {
            let pick = rng.int(0, 11);
            if pick < 8 {
                let factor = rng.range(2.0, 3.0);
                let factor = if rng.chance(0.5) {
                    factor
                } else {
                    1.0 / factor
                };
                match pick {
                    0 => g.speed *= factor,
                    1 => g.sight *= factor,
                    2 => g.fire_period *= factor,
                    3 => g.radius *= factor,
                    4 => g.flocking = (g.flocking.max(0.2)) * factor,
                    5 => g.hull *= factor,
                    6 => g.standoff = g.standoff.max(80.0) * factor,
                    _ => g.alarm *= factor,
                }
            } else {
                match pick {
                    8 => g.social.set(rng.int(0, 4) as u8),
                    9 => g.trigger.set(rng.int(0, 2) as u8),
                    10 => g.fear.set(rng.int(0, 3) as u8),
                    _ => g.diet.set(rng.int(0, 3) as u8),
                }
            }
        }
        if g.weapon == Weapon::Tether {
            // Cords vary a little between individuals, from values already drawn so this
            // adds no draws (HOME's population must not move).
            let wobble = |k: f32| ((roll * k).fract() * 2.0 - 1.0) * spread;
            g.cord_strength *= 1.0 + wobble(131.0);
            g.cord_slack *= 1.0 + wobble(173.0);
            g.cord_hardness *= 1.0 + wobble(211.0);
        }
        let mut g = g.limited();
        // A creature never loses a target it can see.
        g.lose = g.lose.max(g.sight);
        g.limited()
    }

    /// Heritable mutation for offspring: far milder than `individual`, so that variety
    /// accumulates slowly and lineages stay recognizable over many generations. Scale-like
    /// genes move by up to `MUTATION_SMALL` of their value, about 3% of births move them by
    /// up to `MUTATION_RARE`, and the social, trigger and fear categories almost never flip.
    /// Body plan, weapon and diet are untouched. Always passes through `limited()`.
    pub fn mutate(self, rng: &mut Rng) -> Self {
        let rare = rng.chance(MUTATION_RARE_CHANCE);
        let spread = if rare { MUTATION_RARE } else { MUTATION_SMALL };
        let mut g = self;
        let triangle = |rng: &mut Rng| (rng.f32() + rng.f32() - 1.0) * spread;
        for v in [
            &mut g.speed,
            &mut g.cruise,
            &mut g.sight,
            &mut g.lose,
            &mut g.fire_period,
            &mut g.radius,
            &mut g.flocking,
            &mut g.lead,
            &mut g.hull,
            &mut g.shot_speed,
            &mut g.weapon_range,
            &mut g.alarm,
        ] {
            *v *= 1.0 + triangle(rng);
        }
        for v in [&mut g.standoff, &mut g.strafe] {
            if *v > 0.0 {
                *v *= 1.0 + triangle(rng);
            }
        }
        g.hue = (g.hue + triangle(rng) * 0.5).rem_euclid(1.0);
        g.pale += triangle(rng) * 0.5;
        g.bright += triangle(rng) * 0.5;
        if rng.chance(MUTATION_FLIP_CHANCE) {
            match rng.int(0, 2) {
                0 => g.social.set(rng.int(0, 3) as u8),
                1 => g.trigger.set(rng.int(0, 2) as u8),
                _ => g.fear.set(rng.int(0, 3) as u8),
            }
        }
        if g.root >= ROOT_JUVENILE {
            // Rooters drift in how hard they fight and when the young let go.
            g.root_defense += triangle(rng);
            g.detach_size *= 1.0 + triangle(rng);
            g.detach_hunger *= 1.0 + triangle(rng);
            g.detach_crowd *= 1.0 + triangle(rng);
        }
        if g.weapon == Weapon::Tether {
            // Cord throwers drift in how hard their cords are to shake or cut.
            g.cord_strength *= 1.0 + triangle(rng);
            g.cord_slack *= 1.0 + triangle(rng);
            g.cord_hardness *= 1.0 + triangle(rng);
            if g.cord_drag > 0.0 {
                g.cord_drag += triangle(rng);
            }
        }
        if g.learner > 0.0 {
            // Learners drift in how much they trust their brain and how fast it trains.
            g.learner += triangle(rng);
            g.learn_rate *= 1.0 + triangle(rng);
        }
        power::mutate(&mut g, || triangle(rng));
        if let Some(spec) = &mut g.grammar {
            // Its own stream, drawn from only here: genomes without a grammar skip this.
            let mut own = Rng::new(rng.next_u64() ^ GRAMMAR_SALT);
            spec.genome = spec.genome.mutate(&mut own);
        }
        let mut g = g.limited();
        g.lose = g.lose.max(g.sight);
        g.limited()
    }

    /// Recombination of two parents of the same lineage. Body plan (segments, limbs, their
    /// proportions and gait), weapon (kind, volley and shot genes), temperament and
    /// ecology (diet, nest, birth, fecundity) each travel whole from one parent, so a child
    /// never gets a serpent's gait on a rock's body or a spiral without its volley. Other
    /// continuous genes blend by a random weight, other integers and categories pick a
    /// parent. A `mutate` follows, then `limited()`: the result is always valid.
    pub fn crossover(a: Self, b: Self, rng: &mut Rng) -> Self {
        let (mut a, mut b) = (a, b);
        let mut child = a;
        {
            let (ga, gb) = (a.genes(), b.genes());
            // The power block draws nothing here: it travels whole below.
            let blended = ga.len() - POWER_GENES;
            for (slot, (x, y)) in child
                .genes()
                .into_iter()
                .zip(ga.into_iter().zip(gb))
                .take(blended)
            {
                match (slot, x, y) {
                    (Gene::Real { v, .. }, Gene::Real { v: x, .. }, Gene::Real { v: y, .. }) => {
                        *v = *x + (*y - *x) * rng.f32();
                    }
                    (Gene::Int { v, .. }, Gene::Int { v: x, .. }, Gene::Int { v: y, .. }) => {
                        *v = if rng.chance(0.5) { *x } else { *y };
                    }
                    (Gene::Cat { v }, Gene::Cat { v: x }, Gene::Cat { v: y }) => {
                        v.set(if rng.chance(0.5) { x.get() } else { y.get() });
                    }
                    _ => {}
                }
            }
        }
        let pick = |rng: &mut Rng| if rng.chance(0.5) { a } else { b };
        let body = pick(rng);
        child.segments = body.segments;
        child.limbs = body.limbs;
        child.limb_len = body.limb_len;
        child.sides = body.sides;
        child.stiffness = body.stiffness;
        child.wave = body.wave;
        child.rhythm = body.rhythm;
        child.lag = body.lag;
        child.taper = body.taper;
        child.aspect = body.aspect;
        let arms = pick(rng);
        child.weapon = arms.weapon;
        child.volley = arms.volley;
        child.hardpoint_every = arms.hardpoint_every;
        child.fire_period = arms.fire_period;
        child.shot_speed = arms.shot_speed;
        child.weapon_range = arms.weapon_range;
        child.fling = arms.fling;
        child.fling_chaos = arms.fling_chaos;
        child.contact_damage = arms.contact_damage;
        child.cord_strength = arms.cord_strength;
        child.cord_slack = arms.cord_slack;
        child.cord_hardness = arms.cord_hardness;
        child.cord_drag = arms.cord_drag;
        // A habit travels whole: the rooting genes come from the body-plan parent.
        child.root = body.root;
        child.root_defense = body.root_defense;
        child.detach_size = body.detach_size;
        child.detach_hunger = body.detach_hunger;
        child.detach_crowd = body.detach_crowd;
        // So does a power: whole from the body-plan parent, never a feeble half.
        child.take_powers_from(&body);
        // A grammar body is part of the body plan: it comes whole (plan seed included) from
        // the body-plan parent, and recombines with the other parent's only when both have one.
        child.grammar = body.grammar;
        if let (Some(mine), Some(other)) = (a.grammar, b.grammar) {
            let mut own = Rng::new(rng.next_u64() ^ GRAMMAR_SALT);
            if let Some(spec) = &mut child.grammar {
                spec.genome =
                    crate::grammar::GrammarGenome::crossover(mine.genome, other.genome, &mut own);
            }
        }
        let temper = pick(rng);
        child.social = temper.social;
        child.trigger = temper.trigger;
        child.fear = temper.fear;
        child.bond = temper.bond;
        child.rage = temper.rage;
        let ecology = pick(rng);
        child.diet = ecology.diet;
        child.nest = ecology.nest;
        child.birth = ecology.birth;
        child.fecundity = ecology.fecundity;
        child.limited().mutate(rng)
    }

    /// Draws a fresh genome from a distribution biased by sector parameters: tech
    /// favors chains, guns and keen senses; distortion negative mass and waves; swarm
    /// schooling; aggression rage and flinging; danger toughness.
    pub fn sample(rng: &mut Rng, params: &SectorParams) -> Self {
        let above = |p: f32| (p - 0.5).max(0.0) * 2.0;
        let (tech, distortion, swarm, aggression, danger) = (
            params.tech,
            params.distortion,
            params.swarm,
            params.aggression,
            params.danger,
        );
        let mut g = Genome::default();

        // Body plan: chains are rare but favored by tech and distortion; waves are
        // independent of length, so only some chains slither.
        let chain_chance = (0.12 + 0.4 * above(distortion) + 0.3 * above(tech)).min(0.75);
        if rng.chance(chain_chance) {
            g.segments = rng.int(3, (5.0 + 9.0 * tech) as u32) as u8;
            g.wave = rng.range(0.2, 2.2);
        } else {
            g.wave = rng.range(0.0, 0.8);
        }
        g.rhythm = rng.range(1.5, 6.5);
        g.lag = rng.range(0.4, 1.4);
        g.stiffness = (rng.range(40.0_f32.ln(), 800.0_f32.ln())).exp() * (0.7 + 0.6 * distortion);
        g.taper = rng.range(0.55, 1.1);
        if rng.chance(0.25) {
            g.limbs = rng.int(2, 6) as u8;
            g.limb_len = rng.int(1, 3) as u8;
        }
        g.sides = if rng.chance(0.3) {
            0
        } else {
            rng.int(3, 8) as u8
        };
        g.aspect = rng.range(0.7, 1.5);

        // Size, mass and toughness.
        let size = rng.f32().powf(1.6);
        g.radius = 8.0 + 52.0 * size;
        if g.segments > 1 || g.limbs > 0 {
            g.radius = g.radius.min(28.0);
        }
        let negative = rng.chance(0.08 + 0.35 * above(distortion));
        let magnitude = (g.radius * g.radius * rng.range(0.03, 0.09)).max(2.0);
        g.mass = if negative { -magnitude } else { magnitude };
        g.hull = g.radius * rng.range(1.5, 4.5) * (1.0 + 0.5 * danger);
        g.shield = if rng.chance(0.35) {
            rng.range(8.0, 40.0)
        } else {
            0.0
        };

        // Locomotion and sensing.
        g.speed = rng.range(50.0, 280.0) * (0.8 + 0.5 * tech) * (1.0 - 0.5 * size);
        g.cruise = g.speed * rng.range(0.3, 0.5);
        g.lead = if rng.chance(0.4 + 0.4 * tech) {
            tech * rng.range(0.2, 1.0)
        } else {
            0.0
        };
        g.flocking = (rng.range(0.0, 1.0) + 0.5 * swarm).min(1.8);
        g.sight = rng.range(300.0, 1300.0) * (0.6 + 0.8 * tech);
        g.lose = g.sight * rng.range(1.3, 2.2);
        g.mass_affinity = rng.range(-1.0, 1.0) * 0.7;
        if rng.chance(0.4) {
            g.standoff = rng.range(150.0, 420.0);
            g.strafe = rng.range(0.0, 0.7);
        }

        // Social structure and temperament.
        g.social = pick(
            rng,
            &[
                (Social::Solitary, 1.0),
                (Social::School, 1.0 + 2.0 * swarm),
                (Social::Pack, 0.6 + aggression),
                (Social::Brood, 0.5),
                (Social::Dweller, 0.5 + danger),
            ],
        );
        g.bond = if rng.chance(0.25) {
            rng.range(0.3, 1.0)
        } else {
            0.0
        };
        g.trigger = pick(
            rng,
            &[
                (Trigger::Sight, 0.5 + 0.5 * aggression),
                (Trigger::Proximity, 0.35),
                (Trigger::Harm, 0.15),
            ],
        );
        g.rage = if rng.chance(0.4 + 0.3 * aggression) {
            rng.range(0.2, 0.6)
        } else {
            0.0
        };
        g.alarm = rng.range(100.0, 600.0) * (0.7 + 0.6 * swarm);

        // Weapons.
        g.weapon = pick(
            rng,
            &[
                (Weapon::None, 1.2 - tech),
                (Weapon::Projectile, 0.6 + tech),
                (Weapon::Tether, 0.2 + 0.5 * tech),
                (Weapon::Mine, 0.25 + 0.6 * above(distortion) + 0.2 * danger),
                (Weapon::Missile, 0.15 + 0.7 * above(tech) + 0.3 * danger),
                (Weapon::Needles, 0.1 + 0.6 * above(tech) * (0.4 + danger)),
                (Weapon::Nova, 0.15 + 0.6 * above(aggression)),
                (
                    Weapon::Spiral,
                    0.1 + 0.6 * above(aggression) * (0.3 + danger),
                ),
            ],
        );
        g.volley = volley_for(g.weapon, rng);
        g.fire_period = rng.range(1.2, 5.5);
        g.shot_speed = rng.range(200.0, 520.0);
        g.weapon_range = if g.weapon == Weapon::Tether {
            rng.range(400.0, 700.0)
        } else {
            rng.range(350.0, 950.0)
        };
        if g.is_jointed() && rng.chance(0.6) {
            g.hardpoint_every = rng.int(2, 5) as u8;
        }
        if rng.chance(0.2 + 0.4 * above(aggression)) {
            g.fling = rng.range(0.4, 1.5);
        }
        g.fling_chaos = rng.range(0.2, 1.4);
        g.contact_damage = rng.range(4.0, 24.0) * (0.7 + 0.6 * danger) + 8.0 * g.fling;
        g.reel = rng.range(30.0, 100.0);

        // Ecology.
        g.diet = pick(
            rng,
            &[
                (Diet::None, 1.5),
                (Diet::Rocks, 0.8),
                (
                    Diet::Siphon,
                    if g.weapon == Weapon::Tether { 1.5 } else { 0.3 },
                ),
                (Diet::Dust, 0.5),
            ],
        );
        g.fear = pick(
            rng,
            &[
                (Fear::None, 1.6),
                (Fear::Player, 0.6 * (1.0 - aggression) + 0.2),
                (Fear::Bullets, 0.5),
                (Fear::Wells, 0.4),
            ],
        );
        g.nest = pick(
            rng,
            &[
                (Nest::None, 1.5),
                (Nest::Rocks, 0.6),
                (Nest::Base, 0.5 + danger),
            ],
        );

        // Identity: the bounty tracks power; the name and pigment follow the body.
        let power = 30.0
            + g.hull * 0.6
            + g.shield
            + g.speed * 0.2
            + g.fling_strength() * 30.0
            + if g.weapon == Weapon::None { 0.0 } else { 30.0 }
            + 4.0 * g.parts() as f32;
        g.bounty = (power / 5.0).round() * 5.0;
        let theme: &[u8] = if g.segments >= 4 {
            &[5, 6, 7, 8]
        } else if g.limbs > 0 {
            &[9, 10, 11]
        } else if g.weapon == Weapon::Tether {
            &[4, 14, 15]
        } else if g.fling_strength() >= 0.5 {
            &[1, 12, 22]
        } else if g.mass.abs() >= 80.0 {
            &[3, 12, 13]
        } else {
            &[0, 2, 16, 17, 18, 19, 20, 21, 23]
        };
        g.voice0 = theme[rng.int(0, theme.len() as u32 - 1) as usize];
        g.voice1 = rng.int(0, 11) as u8;
        g.voice2 = rng.int(0, 19) as u8;
        g.hue = rng.f32();
        g.pale = rng.range(0.0, 0.7);
        g.bright = rng.range(0.65, 1.0);

        // Foraging diets come last, from the final draw, so every earlier gene of every
        // sampled species stays where it was. Calm, crowded places breed grazers; wild,
        // aggressive ones breed the predators that eat them (and only bodies big enough to).
        let roll = rng.f32();
        if g.diet == Diet::None {
            let hunt = 0.04 + 0.2 * above(aggression) + 0.12 * danger;
            let graze = 0.3 + 0.3 * swarm;
            if roll < hunt && g.body_mass() >= 8.0 {
                g.diet = Diet::Hunt;
            } else if roll < hunt + graze {
                g.diet = Diet::Graze;
            }
        }

        // Reproduction comes last, from one more draw: two in five lay eggs, and the rest
        // of the draw picks how fecund the species is.
        let roll = rng.f32();
        g.birth = if roll < 0.4 { Birth::Egg } else { Birth::Live };
        g.fecundity = match (roll * 37.0).fract() {
            f if f < 0.3 => Fecundity::Rare,
            f if f < 0.8 => Fecundity::Steady,
            _ => Fecundity::Prolific,
        };

        // Learning used to come from this draw. Wild life no longer learns (only civilization
        // members think, see `territory`), but the draw stays so later genes keep their stream.
        let _learning = rng.f32();

        // Rooting comes last, from one more draw: a minority of species cling to rocks, more
        // in crowded, calm places. Most rooters only cling while young; a body without a
        // spine may cling for life. Danger sharpens their defenses.
        let roll = rng.f32();
        let chance = 0.07 + 0.12 * swarm + 0.07 * (1.0 - danger);
        if roll < chance {
            let part = (roll / chance * 61.0).fract();
            let (a, b, c) = (
                (part * 37.0).fract(),
                (part * 71.0).fract(),
                (part * 113.0).fract(),
            );
            g.root = if part < 0.55 || g.is_jointed() {
                0.45 + 0.25 * a
            } else {
                0.78 + 0.2 * a
            };
            g.root_defense = (0.2 + 0.6 * b + 0.3 * danger).min(1.0);
            g.detach_size = 0.7 + 0.3 * c;
            g.detach_hunger = 0.1 + 0.2 * (b * 7.0).fract();
            g.detach_crowd = 0.5 + 0.5 * (c * 5.0).fract();
        }

        // Cords come last, from one more draw. Most cord throwers keep the classic weak
        // cord, a minority are strong, and a rare few (about 3 in 100) grip: the ship cannot
        // out-thrust them and must shoot, shear or kill. Danger and tech make strong ones
        // a little likelier.
        let roll = rng.f32();
        if g.weapon == Weapon::Tether {
            let bias = 0.04 * danger + 0.04 * tech;
            let part = (roll * 53.0).fract();
            let lerp = |lo: f32, hi: f32, t: f32| lo + (hi - lo) * t;
            let (a, b, c) = (
                (part * 17.0).fract(),
                (part * 29.0).fract(),
                (part * 43.0).fract(),
            );
            if roll >= 1.0 - 0.03 - bias * 0.25 {
                g.cord_strength = lerp(6.0, 8.0, a);
                g.cord_slack = lerp(2000.0, 3000.0, b);
                g.cord_hardness = lerp(6.0, 9.0, c);
                g.cord_drag = lerp(0.5, 0.9, part);
            } else if roll >= 0.88 - bias {
                g.cord_strength = lerp(3.0, 5.5, a);
                g.cord_slack = lerp(500.0, 1200.0, b);
                g.cord_hardness = lerp(4.0, 6.0, c);
                g.cord_drag = lerp(0.2, 0.5, part);
            } else if roll >= 0.6 {
                g.cord_strength = lerp(1.2, 2.2, a);
                g.cord_slack = lerp(220.0, 330.0, b);
                g.cord_hardness = lerp(2.4, 3.6, c);
                g.cord_drag = lerp(0.0, 0.2, part);
            } else {
                g.cord_strength = lerp(1.0, 1.25, a);
                g.cord_slack = lerp(200.0, 260.0, b);
                g.cord_hardness = lerp(2.0, 2.5, c);
            }
        }

        // Rare powers come last, from one more draw (see `power`): about 7 percent of species
        // far from home carry one, none inside ring 3.
        let roll = rng.f32();
        power::sample(&mut g, roll, params);
        g.limited()
    }

    // The hand-authored HOME genomes. Each reproduces one of the original enemy kinds.

    pub fn bogey() -> Self {
        Self {
            radius: 15.0,
            hull: 35.0,
            shield: 18.0,
            mass: 8.0,
            speed: 160.0,
            cruise: 75.0,
            lead: 0.15,
            sight: 320.0,
            lose: 650.0,
            standoff: 220.0,
            strafe: 0.45,
            rage: 0.4,
            alarm: 220.0,
            fire_period: 2.4,
            shot_speed: 300.0,
            weapon_range: 800.0,
            contact_damage: 6.0,
            bounty: 100.0,
            sides: 0,
            hue: 0.604,
            pale: 0.28,
            bright: 1.0,
            voice0: 0,
            voice1: 0,
            voice2: 0,
            social: Social::School,
            trigger: Trigger::Proximity,
            weapon: Weapon::Projectile,
            diet: Diet::Graze,
            nest: Nest::Rocks,
            birth: Birth::Egg,
            fecundity: Fecundity::Rare,
            ..Self::default()
        }
    }

    pub fn lunatic() -> Self {
        Self {
            radius: 18.0,
            hull: 45.0,
            mass: 6.0,
            speed: 230.0,
            cruise: 95.0,
            alarm: 380.0,
            fling: 1.0,
            contact_damage: 18.0,
            bounty: 125.0,
            sides: 4,
            aspect: 1.25,
            hue: 0.667,
            pale: 0.94,
            bright: 1.0,
            voice0: 1,
            voice1: 1,
            voice2: 1,
            social: Social::School,
            ..Self::default()
        }
    }

    pub fn smarty() -> Self {
        Self {
            radius: 18.0,
            hull: 70.0,
            mass: 12.0,
            speed: 285.0,
            cruise: 110.0,
            lead: 0.55,
            learner: 0.6,
            learn_rate: 0.5,
            alarm: 380.0,
            contact_damage: 6.0,
            bounty: 200.0,
            sides: 3,
            hue: 0.595,
            pale: 0.816,
            bright: 0.76,
            voice0: 2,
            voice1: 0,
            voice2: 2,
            social: Social::School,
            ..Self::default()
        }
    }

    pub fn fatso() -> Self {
        Self {
            radius: 48.0,
            hull: 180.0,
            mass: 200.0,
            speed: 60.0,
            cruise: 25.0,
            alarm: 380.0,
            contact_damage: 20.0,
            bounty: 250.0,
            sides: 0,
            hue: 0.096,
            pale: 0.386,
            bright: 0.88,
            voice0: 3,
            voice1: 0,
            voice2: 3,
            social: Social::School,
            ..Self::default()
        }
    }

    pub fn leech() -> Self {
        Self {
            radius: 16.0,
            hull: 40.0,
            shield: 20.0,
            mass: 7.0,
            speed: 190.0,
            cruise: 80.0,
            lead: 0.2,
            standoff: 300.0,
            strafe: 0.45,
            bond: 0.6,
            alarm: 380.0,
            fire_period: 5.0,
            weapon_range: 650.0,
            contact_damage: 6.0,
            reel: 55.0,
            bounty: 150.0,
            sides: 0,
            hue: 0.758,
            pale: 0.45,
            bright: 1.0,
            voice0: 4,
            voice1: 0,
            voice2: 4,
            social: Social::School,
            weapon: Weapon::Tether,
            diet: Diet::Siphon,
            ..Self::default()
        }
    }

    /// What a newborn is before it matures: a small, unarmed, single-bodied relative that
    /// schools. It grows into the genome it came from (`Body::adult`).
    pub fn juvenile(&self) -> Self {
        Self {
            radius: (self.radius * 0.55).max(6.0),
            hull: (self.hull * 0.5).max(10.0),
            shield: 0.0,
            mass: self.mass * 0.3,
            speed: self.speed * 1.1,
            fling: self.fling * 0.5,
            bounty: (self.bounty * 0.3).max(20.0),
            segments: 1,
            limbs: 0,
            // A young one that clings to a rock fights for its place.
            weapon: if self.defended_young() {
                self.weapon
            } else {
                Weapon::None
            },
            social: Social::School,
            bond: 0.0,
            ..*self
        }
        .limited()
    }

    /// The serpent of the previous design, as a point in genome space.
    pub fn serpent() -> Self {
        Self {
            segments: 8,
            stiffness: 200.0,
            wave: 1.0,
            rhythm: 3.0,
            lag: 0.9,
            radius: 11.0,
            hull: 28.0,
            mass: 5.0,
            speed: 210.0,
            cruise: 90.0,
            lead: 0.3,
            alarm: 380.0,
            hardpoint_every: 3,
            weapon: Weapon::Projectile,
            fire_period: 3.0,
            shot_speed: 270.0,
            weapon_range: 650.0,
            contact_damage: 6.0,
            bounty: 60.0,
            hue: 0.23,
            pale: 0.67,
            bright: 0.92,
            voice0: 5,
            voice1: 0,
            voice2: 5,
            social: Social::School,
            ..Self::default()
        }
    }
}

fn pick<T: Copy>(rng: &mut Rng, options: &[(T, f32)]) -> T {
    let total: f32 = options.iter().map(|(_, w)| w.max(0.0)).sum();
    let mut roll = rng.f32() * total;
    for &(value, weight) in options {
        roll -= weight.max(0.0);
        if roll < 0.0 {
            return value;
        }
    }
    options[0].0
}

/// A lineage and its genome. The lineage is the stable identity; the genome is how that
/// lineage looks in the place it was drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Species {
    pub lineage: u64,
    /// How many sectors the lineage has drifted from its founding node.
    pub generation: u16,
    pub genome: Genome,
}

/// Distinguishes the lineage of a sessile cousin from the species it was grown from.
const SESSILE_SALT: u64 = 0x5E55_1100_0000_0019;

/// Lineage ids of the five hand-authored HOME species.
const HOME_LINEAGES: [u64; 5] = [
    0x484F_4D45_0000_0001,
    0x484F_4D45_0000_0003,
    0x484F_4D45_0000_0005,
    0x484F_4D45_0000_0007,
    0x484F_4D45_0000_0009,
];

impl Species {
    pub fn name(&self) -> String {
        self.genome.name()
    }

    pub fn bogey() -> Self {
        Self::home(0, Genome::bogey())
    }
    pub fn lunatic() -> Self {
        Self::home(1, Genome::lunatic())
    }
    pub fn smarty() -> Self {
        Self::home(2, Genome::smarty())
    }
    pub fn fatso() -> Self {
        Self::home(3, Genome::fatso())
    }
    pub fn leech() -> Self {
        Self::home(4, Genome::leech())
    }

    fn home(slot: usize, genome: Genome) -> Self {
        Self {
            lineage: HOME_LINEAGES[slot],
            generation: 0,
            genome,
        }
    }

    /// The same lineage as one individual: the genome varied by `Genome::individual`.
    pub fn individual(self, rng: &mut Rng) -> Self {
        Self {
            genome: self.genome.individual(rng),
            ..self
        }
    }

    /// `individual` inside a ring (see `Genome::individual_in`).
    pub fn individual_in(self, rng: &mut Rng, ring: u32) -> Self {
        Self {
            genome: self.genome.individual_in(rng, ring),
            ..self
        }
    }

    /// The child of two members of one lineage: their genomes recombined, a generation on
    /// from the older parent. Mates of different lineages cannot cross; the child is then
    /// simply a mutated copy of `self`.
    pub fn crossover(self, mate: Self, rng: &mut Rng) -> Self {
        if self.lineage != mate.lineage {
            return Self {
                genome: self.genome.mutate(rng),
                generation: self.generation.saturating_add(1),
                ..self
            };
        }
        Self {
            genome: Genome::crossover(self.genome, mate.genome, rng),
            generation: self.generation.max(mate.generation).saturating_add(1),
            ..self
        }
    }

    /// A serpent as a one-off species, for tests and experiments.
    pub fn serpent() -> Self {
        Self {
            lineage: 0x484F_4D45_0000_000B,
            generation: 0,
            genome: Genome::serpent(),
        }
    }

    /// A sessile cousin of this species, for places with no native rooters: the same creature
    /// without a spine or limbs, sized to fit a host `bound` in radius, with a rooted habit
    /// (for life when `life`) and its own lineage, so it never interbreeds with the free
    /// relatives it came from.
    pub fn sessile(self, life: bool, bound: f32) -> Self {
        let mut genome = self.genome;
        genome.segments = 1;
        genome.limbs = 0;
        genome.hardpoint_every = 0;
        let radius = genome.radius.min(bound).max(6.0);
        genome.hull *= (radius / genome.radius).clamp(0.4, 1.0);
        genome.radius = radius;
        genome.root = if life { 0.9 } else { 0.6 };
        genome.root_defense = 0.6;
        genome.detach_size = 0.85;
        genome.detach_hunger = 0.2;
        genome.detach_crowd = 0.8;
        Self {
            lineage: (self.lineage ^ SESSILE_SALT) | 1,
            genome: genome.limited(),
            ..self
        }
    }

    /// Wraps a bare genome as a one-off species with a lineage derived from its genes.
    pub fn of(genome: Genome) -> Self {
        let bits = genome.normalized().iter().fold(0x9E37_79B9_u64, |h, x| {
            h.wrapping_mul(0x100_0000_01B3) ^ u64::from(x.to_bits())
        });
        Self {
            lineage: bits | 1,
            generation: 0,
            genome,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoolEntry {
    pub species: Species,
    /// Relative abundance in this sector.
    pub weight: f32,
}

/// The species alive in one sector.
#[derive(Clone, Debug, PartialEq)]
pub struct GenePool {
    pub entries: Vec<PoolEntry>,
}

impl GenePool {
    /// The hand-authored pool of the starting sector.
    pub fn home() -> Self {
        Self {
            entries: [
                Species::bogey(),
                Species::lunatic(),
                Species::smarty(),
                Species::fatso(),
                Species::leech(),
            ]
            .into_iter()
            .map(|species| PoolEntry {
                species,
                weight: 1.0,
            })
            .collect(),
        }
    }

    /// A weighted draw among the species of one niche; when the pool has none, among all.
    pub fn fill(&self, niche: Niche, rng: &mut Rng) -> Species {
        let of_niche: Vec<&PoolEntry> = self
            .entries
            .iter()
            .filter(|e| e.species.genome.niche() == niche)
            .collect();
        if of_niche.is_empty() {
            self.any(rng)
        } else {
            Self::weighted(&of_niche, rng)
        }
    }

    pub fn any(&self, rng: &mut Rng) -> Species {
        let all: Vec<&PoolEntry> = self.entries.iter().collect();
        Self::weighted(&all, rng)
    }

    /// A species that makes its home in rock nests, else a passive schooler.
    pub fn nesting(&self, rng: &mut Rng) -> Species {
        let nesters: Vec<&PoolEntry> = self
            .entries
            .iter()
            .filter(|e| e.species.genome.nest == Nest::Rocks)
            .collect();
        if nesters.is_empty() {
            self.fill(Niche::School, rng)
        } else {
            Self::weighted(&nesters, rng)
        }
    }

    /// A species that is bred by bases, else the best-fitting niche.
    pub fn bred(&self, niche: Niche, rng: &mut Rng) -> Species {
        let of_niche: Vec<&PoolEntry> = self
            .entries
            .iter()
            .filter(|e| e.species.genome.niche() == niche && e.species.genome.nest == Nest::Base)
            .collect();
        if of_niche.is_empty() {
            self.fill(niche, rng)
        } else {
            Self::weighted(&of_niche, rng)
        }
    }

    fn weighted(entries: &[&PoolEntry], rng: &mut Rng) -> Species {
        let total: f32 = entries.iter().map(|e| e.weight).sum();
        let mut roll = rng.f32() * total;
        for entry in entries {
            roll -= entry.weight;
            if roll < 0.0 {
                return entry.species;
            }
        }
        entries[entries.len() - 1].species
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::SectorId;

    const GOLDEN_NO_GRAMMAR: u64 = 0xc128e765da6c6cdf;

    #[test]
    fn home_species_are_named_and_colored_from_their_genes() {
        let names: Vec<String> = GenePool::home()
            .entries
            .iter()
            .map(|e| e.species.name())
            .collect();
        assert_eq!(names, ["Bogey", "Lunatic", "Smarty", "Fatso", "Leech"]);
        assert_eq!(Species::serpent().name(), "Serpent");
        let [r, g, b] = Genome::bogey().color();
        assert!((r - 0.28).abs() < 0.01 && (g - 0.55).abs() < 0.01 && (b - 1.0).abs() < 0.01);
        let [r, g, b] = Genome::fatso().color();
        assert!((r - 0.88).abs() < 0.01 && (g - 0.65).abs() < 0.01 && (b - 0.34).abs() < 0.01);
    }

    #[test]
    fn home_species_occupy_their_original_niches() {
        let niche = |g: Genome| g.niche();
        assert_eq!(niche(Genome::bogey()), Niche::School);
        assert_eq!(niche(Genome::lunatic()), Niche::Fling);
        assert_eq!(niche(Genome::smarty()), Niche::Hunter);
        assert_eq!(niche(Genome::fatso()), Niche::Heavy);
        assert_eq!(niche(Genome::leech()), Niche::Tether);
    }

    #[test]
    fn individuals_stay_valid_and_mostly_close_to_their_species() {
        let mut rng = Rng::new(11);
        let (mut small, mut wide, mut far) = (0, 0, 0);
        let samples = 4000;
        for i in 0..samples {
            let base = match i % 5 {
                0 => Genome::bogey(),
                1 => Genome::lunatic(),
                2 => Genome::smarty(),
                3 => Genome::fatso(),
                _ => Genome::serpent(),
            };
            let mut g = base.individual(&mut rng);
            assert_eq!(g, g.limited());
            assert!(g.parts() <= MAX_PARTS && g.lose >= g.sight);
            for gene in g.genes() {
                if let Gene::Real { v, lo, hi } = gene {
                    assert!(v.is_finite() && *v >= lo && *v <= hi);
                }
            }
            // The species stays recognizable: body plan and weapon are never touched.
            assert_eq!(
                (g.segments, g.limbs, g.weapon),
                (base.segments, base.limbs, base.weapon)
            );
            // The rooting genes (five), the cord genes (four, which only tetherers vary) and the
            // power block do not vary here; measure over the rest.
            let n = base.normalized().len() as f32;
            let still = 9.0 + POWER_GENES as f32;
            let d = base.distance(&g) * n / (n - still);
            if d < 0.01 {
                small += 1;
            } else if d < 0.028 {
                wide += 1;
            } else {
                far += 1;
            }
        }
        assert!(small > samples * 85 / 100, "most are tiny jitters: {small}");
        assert!(wide + far > 0, "some spread wider");
        assert!(far > 0 && far < samples / 20, "a few outliers: {far}");
    }

    fn parents(i: usize, rng: &mut Rng) -> (Genome, Genome) {
        let base = match i % 6 {
            0 => Genome::bogey(),
            1 => Genome::lunatic(),
            2 => Genome::smarty(),
            3 => Genome::fatso(),
            4 => Genome::serpent(),
            _ => Genome::leech(),
        };
        // Two relatives of one lineage, one of them quite a different individual.
        (
            base.individual(rng),
            Genome::individual(base, rng).individual(rng),
        )
    }

    #[test]
    fn crossover_children_are_valid_and_inherit_within_the_parents_range() {
        let mut rng = Rng::new(21);
        for i in 0..4000 {
            let (mut a, mut b) = parents(i, &mut rng);
            let sp = SectorParams {
                depth: 0.0,
                danger: rng.f32(),
                aggression: rng.f32(),
                density: rng.f32(),
                distortion: rng.f32(),
                tech: rng.f32(),
                swarm: rng.f32(),
            };
            if i % 4 == 0 {
                // Fully unrelated random genomes still cross into valid ones.
                a = Genome::sample(&mut rng, &sp);
                b = Genome::sample(&mut rng, &sp);
            }
            let mut child = Genome::crossover(a, b, &mut rng);
            assert_eq!(child, child.limited());
            assert!(child.parts() <= MAX_PARTS && child.lose >= child.sight);
            let (ga, gb) = (a.genes(), b.genes());
            for (c, (x, y)) in child.genes().into_iter().zip(ga.into_iter().zip(gb)) {
                match (c, x, y) {
                    (
                        Gene::Real { v, lo, hi },
                        Gene::Real { v: x, .. },
                        Gene::Real { v: y, .. },
                    ) => {
                        assert!(v.is_finite() && *v >= lo && *v <= hi);
                        let slack = (hi - lo) * 0.05 + 0.2 * x.abs().max(y.abs());
                        // Pigment wraps around at the ends of its range.
                        let wrapped = *v < lo + slack || *v > hi - slack;
                        assert!(
                            wrapped || (*v >= x.min(*y) - slack && *v <= x.max(*y) + slack),
                            "{v} {x} {y}"
                        );
                    }
                    (Gene::Int { v, lo, hi }, ..) => assert!((lo..=hi).contains(v)),
                    _ => {}
                }
            }
            // Gene groups travel whole: the child's body plan is one parent's.
            assert!(child.segments == a.segments || child.segments == b.segments);
            assert!(child.weapon == a.weapon || child.weapon == b.weapon);
            assert!(child.diet == a.diet || child.diet == b.diet);
        }
    }

    /// A fingerprint of every gene (as bits) of a genome.
    fn fingerprint(g: &Genome, h: &mut u64) {
        let mut copy = *g;
        for gene in copy.genes() {
            let bits = match gene {
                Gene::Real { v, .. } => u64::from(v.to_bits()),
                Gene::Int { v, .. } => u64::from(*v),
                Gene::Cat { v } => u64::from(v.get()),
            };
            *h = hash2(*h, (bits & 0xFFFF_FFFF) as i32, (bits >> 32) as i32);
        }
    }

    /// Pins sampling, individuals, crossover and mutation (genes and the stream position
    /// after each) over many seeds, so a genome without a grammar can never drift.
    #[test]
    fn genomes_without_a_grammar_keep_their_draws() {
        let mut h = 0x5EED_u64;
        for seed in 0..300u64 {
            let mut rng = Rng::new(seed * 7919 + 13);
            let sp = SectorParams {
                depth: (seed % 9) as f32,
                danger: rng.f32(),
                aggression: rng.f32(),
                density: rng.f32(),
                distortion: rng.f32(),
                tech: rng.f32(),
                swarm: rng.f32(),
            };
            let a = Genome::sample(&mut rng, &sp);
            let b = Genome::sample(&mut rng, &sp).individual(&mut rng);
            let c = Genome::crossover(a, b, &mut rng);
            let m = c.mutate(&mut rng);
            for g in [&a, &b, &c, &m] {
                assert!(g.grammar.is_none());
                fingerprint(g, &mut h);
            }
            h = hash2(h, (rng.next_u64() & 0x7FFF_FFFF) as i32, 1);
        }
        assert_eq!(h, GOLDEN_NO_GRAMMAR, "{h:#x}");
    }

    #[test]
    fn grammar_genomes_cross_and_mutate_to_valid_bounded_bodies() {
        let mut rng = Rng::new(404);
        let gram = [Genome::ribwyrm(), Genome::corallid(), Genome::colossus()];
        let plain = [Genome::bogey(), Genome::serpent(), Genome::fatso()];
        for i in 0..3000 {
            let a = gram[i % 3];
            let b = match i % 4 {
                0 => gram[(i / 3) % 3],
                1 => plain[(i / 3) % 3],
                2 => a,
                _ => Genome::default(),
            };
            let (x, y) = if i % 2 == 0 { (a, b) } else { (b, a) };
            let mut child = Genome::crossover(x, y, &mut rng);
            for _ in 0..(i % 5) {
                child = child.mutate(&mut rng);
            }
            assert_eq!(child, child.limited());
            // The grammar comes whole from the body-plan parent: present only if a parent has one.
            if let Some(spec) = child.grammar {
                assert!(x.grammar.is_some() || y.grammar.is_some());
                assert!(spec.genome == spec.genome.limited());
                let n = child.parts();
                assert!((1..=crate::bodyplan::BODY_PARTS as u32).contains(&n));
                if let Some(body) = crate::bodyplan::express(&spec, child.radius, 1.0) {
                    assert_eq!(body.nodes.len() as u32, n);
                }
            } else {
                assert!(child.parts() <= MAX_PARTS);
            }
            assert!(child.lose >= child.sight);
        }
        // Without a grammar on either side, none appears; with only one side it may.
        let mut seen = [false; 2];
        for _ in 0..200 {
            let none = Genome::crossover(plain[0], plain[1], &mut rng).mutate(&mut rng);
            assert!(none.grammar.is_none());
            seen[usize::from(
                Genome::crossover(gram[0], plain[0], &mut rng)
                    .grammar
                    .is_some(),
            )] = true;
        }
        assert_eq!(seen, [true, true]);
        // Deterministic.
        let one = Genome::crossover(gram[0], gram[1], &mut Rng::new(5));
        assert_eq!(one, Genome::crossover(gram[0], gram[1], &mut Rng::new(5)));
        assert_eq!(
            gram[2].mutate(&mut Rng::new(8)),
            gram[2].mutate(&mut Rng::new(8))
        );
    }

    #[test]
    fn mutation_keeps_a_grammar_body_recognizable_and_its_seed() {
        let mut rng = Rng::new(9);
        let base = Genome::corallid();
        let seed = base.grammar.unwrap().seed;
        let mut g = base;
        for _ in 0..40 {
            g = g.mutate(&mut rng);
            assert_eq!(g.grammar.unwrap().seed, seed);
        }
        assert!(
            g.grammar
                .unwrap()
                .genome
                .normalized()
                .iter()
                .all(|v| v.is_finite())
        );
    }

    #[test]
    fn crossover_is_deterministic_and_mixes_the_parents() {
        let a = Genome::bogey();
        let mut b = Genome::bogey();
        b.speed = 400.0;
        b.hull = 300.0;
        b.hue = 0.1;
        let one = Genome::crossover(a, b, &mut Rng::new(5));
        assert_eq!(one, Genome::crossover(a, b, &mut Rng::new(5)));
        let mut rng = Rng::new(6);
        let speeds: Vec<f32> = (0..200)
            .map(|_| Genome::crossover(a, b, &mut rng).speed)
            .collect();
        assert!(speeds.iter().any(|s| *s < 200.0) && speeds.iter().any(|s| *s > 250.0));
        assert!(
            speeds
                .iter()
                .all(|s| (a.speed * 0.9..=b.speed * 1.1).contains(s))
        );
    }

    #[test]
    fn a_lineage_bred_for_thirty_generations_stays_itself_but_varies() {
        let mut rng = Rng::new(31);
        let (mut kept, mut varied, mut worst) = (0, 0, 0.0f32);
        let founders = [
            Genome::bogey(),
            Genome::lunatic(),
            Genome::smarty(),
            Genome::fatso(),
            Genome::leech(),
            Genome::serpent(),
        ];
        let runs = 300;
        for run in 0..runs {
            let founder = founders[run % founders.len()];
            // A small mixed population; each generation is bred from two random members,
            // sexually or not.
            let mut pop: Vec<Genome> = (0..6).map(|_| founder.individual(&mut rng)).collect();
            for _ in 0..30 {
                let next: Vec<Genome> = (0..6)
                    .map(|k| {
                        let a = pop[rng.int(0, 5) as usize];
                        if k % 3 == 0 {
                            a.mutate(&mut rng)
                        } else {
                            Genome::crossover(a, pop[rng.int(0, 5) as usize], &mut rng)
                        }
                    })
                    .collect();
                pop = next;
            }
            for g in &pop {
                let d = founder.distance(g);
                worst = worst.max(d);
                assert!(d < 0.12, "drifted too far: {d}");
                assert_eq!(*g, g.limited());
            }
            let g = pop[0];
            if (g.social, g.trigger, g.fear, g.diet)
                == (founder.social, founder.trigger, founder.fear, founder.diet)
                && g.weapon == founder.weapon
            {
                kept += 1;
            }
            varied += (pop.iter().any(|x| x.speed != pop[0].speed)) as usize;
        }
        assert!(kept > runs * 95 / 100, "categories intact in {kept}/{runs}");
        assert!(varied > runs * 95 / 100, "still varied: {varied}");
        assert!(worst > 0.0);
    }

    #[test]
    fn single_parent_descent_does_not_compound_outliers() {
        let mut rng = Rng::new(32);
        let founder = Genome::bogey();
        let mut far = 0;
        for _ in 0..200 {
            let mut g = founder;
            for _ in 0..30 {
                g = g.mutate(&mut rng);
            }
            far += (founder.distance(&g) > 0.05) as usize;
        }
        assert!(far < 10, "{far} of 200 lines wandered off");
    }

    #[test]
    fn individuals_are_deterministic_and_actually_vary() {
        let base = Genome::bogey();
        let a = base.individual(&mut Rng::new(3));
        assert_eq!(a, base.individual(&mut Rng::new(3)));
        assert_ne!(a, base.individual(&mut Rng::new(4)));
        assert_ne!(a, base);
        let sp = Species::bogey().individual(&mut Rng::new(3));
        assert_eq!(sp.lineage, Species::bogey().lineage);
    }

    #[test]
    fn sampled_genomes_are_valid_and_diverse_across_the_parameter_space() {
        let mut rng = Rng::new(5);
        let mut chains = 0;
        let mut negative = 0;
        let mut slithering = 0;
        let mut names = std::collections::HashSet::new();
        for i in 0..2000 {
            let params = SectorParams {
                depth: 0.0,
                danger: rng.f32(),
                aggression: rng.f32(),
                density: rng.f32(),
                distortion: rng.f32(),
                tech: rng.f32(),
                swarm: rng.f32(),
            };
            let mut g = Genome::sample(&mut rng, &params);
            assert!(g.parts() <= MAX_PARTS, "{i}");
            assert_eq!(g, g.limited());
            assert!(g.normalized().iter().all(|x| (0.0..=1.0).contains(x)));
            chains += (g.segments >= 3) as u32;
            negative += (g.mass < 0.0) as u32;
            slithering += (g.segments >= 3 && g.wave > 0.8) as u32;
            names.insert(g.name());
            // Every gene stays inside its bounds.
            for gene in g.genes() {
                if let Gene::Real { v, lo, hi } = gene {
                    assert!(v.is_finite() && *v >= lo && *v <= hi);
                }
            }
        }
        assert!(chains > 100 && negative > 80 && slithering > 40);
        assert!(names.len() > 300);
    }

    #[test]
    fn tech_and_distortion_bias_the_distribution() {
        let share = |tech: f32, distortion: f32| {
            let params = SectorParams {
                tech,
                distortion,
                ..SectorParams::HOME
            };
            let mut rng = Rng::new(9);
            (0..1500)
                .filter(|_| {
                    let g = Genome::sample(&mut rng, &params);
                    g.segments >= 3 && g.wave > 0.6
                })
                .count()
        };
        assert!(share(0.95, 0.95) > share(0.05, 0.05) * 2);
    }

    /// Mean normalized distance between genomes of the lineages two pools share.
    fn drift(a: &GenePool, b: &GenePool) -> (usize, f32, f32) {
        let mut shared = 0;
        let mut total = 0.0;
        let mut worst = 0.0_f32;
        for x in &a.entries {
            for y in &b.entries {
                if x.species.lineage == y.species.lineage {
                    let d = x.species.genome.distance(&y.species.genome);
                    shared += 1;
                    total += d;
                    worst = worst.max(d);
                }
            }
        }
        (shared, total / shared.max(1) as f32, worst)
    }

    #[test]
    fn neighbouring_sectors_hold_close_relatives_of_the_same_species() {
        let seed = 31;
        let (mut checked, mut shared_total) = (0, 0);
        for x in -12..=12 {
            for y in -12..=12 {
                let here = SectorId { x, y };
                let pool = crate::range::ecology(seed, here).pool();
                for (dx, dy) in [(1, 0), (0, 1)] {
                    let next = crate::range::ecology(
                        seed,
                        SectorId {
                            x: x + dx,
                            y: y + dy,
                        },
                    )
                    .pool();
                    let (shared, mean, worst) = drift(&pool, &next);
                    // Lineages the two sectors share are close relatives: drift is gradual.
                    assert!(mean < 0.12, "{here:?} mean drift {mean}");
                    assert!(worst < 0.4, "{here:?} worst drift {worst}");
                    shared_total += shared;
                    checked += 1;
                }
            }
        }
        assert!(checked > 800 && shared_total > 400, "{shared_total}");
    }

    #[test]
    fn species_recur_across_sectors_with_mutation() {
        let seed = 12;
        // One sampled lineage across the sectors its range covers: it persists but its
        // genome drifts from place to place, never as an exact copy of itself.
        let mut by_lineage: std::collections::HashMap<u64, std::collections::HashSet<u64>> =
            Default::default();
        for x in -20..=20 {
            for y in -20..=20 {
                for p in crate::range::ecology(seed, SectorId { x, y }).presence {
                    if p.family != crate::range::Family::Wild {
                        continue;
                    }
                    by_lineage.entry(p.species.lineage).or_default().insert(
                        p.species
                            .genome
                            .normalized()
                            .iter()
                            .map(|v| v.to_bits())
                            .fold(0u64, |h, b| h.wrapping_mul(31) ^ u64::from(b)),
                    );
                }
            }
        }
        let widest = by_lineage.values().map(|v| v.len()).max().unwrap_or(0);
        assert!(widest > 3, "lineage never varied: {widest}");
    }

    #[test]
    fn foraging_diets_are_appended_and_sampled_without_moving_older_genes() {
        // Older diets keep their indices, so stored genomes read the same.
        assert_eq!(
            [Diet::None, Diet::Rocks, Diet::Siphon, Diet::Dust].map(|d| d.get()),
            [0, 1, 2, 3]
        );
        assert_eq!(Genome::bogey().diet, Diet::Graze);
        assert!(Genome::bogey().forages() && !Genome::lunatic().forages());
        let (mut graze, mut hunt, mut other) = (0, 0, 0);
        let params = SectorParams {
            aggression: 0.8,
            swarm: 0.6,
            ..SectorParams::HOME
        };
        let mut rng = Rng::new(77);
        for _ in 0..3000 {
            let g = Genome::sample(&mut rng, &params);
            match g.diet {
                Diet::Graze => graze += 1,
                Diet::Hunt => {
                    hunt += 1;
                    assert!(g.body_mass() >= 8.0, "a predator needs bulk");
                }
                _ => other += 1,
            }
        }
        assert!(
            graze > 500 && hunt > 100 && other > 1000,
            "{graze} {hunt} {other}"
        );
    }

    #[test]
    fn cord_genes_are_mostly_weak_a_minority_strong_a_few_gripping_and_valid() {
        let params = SectorParams {
            tech: 0.7,
            danger: 0.6,
            ..SectorParams::HOME
        };
        let mut rng = Rng::new(5);
        let (mut tethers, mut weak, mut strong, mut grip) = (0, 0, 0, 0);
        for _ in 0..20_000 {
            let g = Genome::sample(&mut rng, &params);
            assert_eq!(g, g.limited());
            if g.weapon != Weapon::Tether {
                // Only cord throwers carry cord genes.
                assert_eq!(
                    (g.cord_strength, g.cord_slack, g.cord_hardness),
                    (1.0, 200.0, 2.0)
                );
                continue;
            }
            tethers += 1;
            if g.cord_slack <= 260.0 && g.cord_hardness <= 2.5 && g.cord_drag == 0.0 {
                weak += 1;
                assert!(g.cord_strength <= 1.25);
            }
            if g.cord_strength >= 3.0 {
                strong += 1;
            }
            if g.cord_strength >= 6.0 {
                grip += 1;
                assert!(g.cord_slack >= 2000.0 && g.cord_drag >= 0.5);
            }
        }
        assert!(tethers > 1000, "{tethers}");
        assert!(
            weak > tethers * 55 / 100,
            "most cords are weak: {weak}/{tethers}"
        );
        assert!(
            strong > tethers / 20 && strong < tethers * 30 / 100,
            "a minority are strong: {strong}/{tethers}"
        );
        assert!(
            grip > 0 && grip < tethers / 12,
            "a few grip: {grip}/{tethers}"
        );
        // HOME's leech keeps the classic cord, individuals stay near it, and mutation and
        // crossover keep cords valid.
        let leech = Genome::leech();
        assert_eq!(
            (leech.cord_strength, leech.cord_slack, leech.cord_hardness),
            (1.0, 200.0, 2.0)
        );
        let mut rng = Rng::new(9);
        for _ in 0..300 {
            let g = leech.individual(&mut rng);
            assert!(g.cord_hardness < 2.6 && g.cord_slack < 260.0 && g.cord_strength < 1.3);
            let child = Genome::crossover(g, Genome::leech().mutate(&mut rng), &mut rng);
            assert_eq!(child, child.limited());
        }
    }

    #[test]
    fn wild_ranges_hold_grazers_and_predators() {
        let mut diets = std::collections::HashSet::new();
        for x in -25..=25 {
            for y in -25..=25 {
                for p in crate::range::ecology(5, SectorId { x, y }).presence {
                    diets.insert(p.species.genome.diet.get());
                }
            }
        }
        assert!(diets.contains(&(Diet::Graze.get())) && diets.contains(&(Diet::Hunt.get())));
    }

    #[test]
    fn nothing_sampled_in_the_wild_learns_and_the_old_learners_still_breed_true() {
        for tech in [0.2, 0.95] {
            let params = SectorParams {
                tech,
                ..SectorParams::HOME
            };
            let mut rng = Rng::new(31);
            for _ in 0..3000 {
                let g = Genome::sample(&mut rng, &params);
                assert_eq!(g, g.limited());
                assert_eq!(g.learner, 0.0, "only civilization members learn");
            }
        }
        // The old Smarty still learns (a civilization's kin), and offspring of learners stay valid learners.
        assert!(Genome::smarty().learner > 0.0);
        assert_eq!(Genome::bogey().learner + Genome::fatso().learner, 0.0);
        let mut rng = Rng::new(5);
        let child = Genome::crossover(Genome::smarty(), Genome::smarty(), &mut rng);
        assert!(child.learner > 0.3 && child == child.limited());
        // Non-learners never gain the trait through breeding.
        let plain = Genome::crossover(Genome::bogey(), Genome::bogey(), &mut rng);
        assert_eq!(plain.learner, 0.0);
    }
}
