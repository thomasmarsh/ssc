//! Organs: parts of other creatures the ship can grow. An organ is an owned strain (a kind, a
//! level 1 to 3 and a magnitude taken from the donor's genes), kept for the run through death,
//! cleared on restart and never lowered: the same promise as the arsenal and the skills. A
//! lesser find (one that cannot raise what is owned) pays biomass instead.
//!
//! Strains come from three places: **bond** (a Kindling Remora groomed calmly, see `parasite`),
//! **harvest** (a special carrier's first kill leaves a specimen one time in four, rolled from
//! the loot stream so a route always pays the same) and **relic** (a sealed specimen lying in
//! some sectors). A strain does nothing until it is fitted: the SYMBIOSIS skill opens slots
//! (one a level), a graft costs crystal and fuel the first time, each fitted organ draws a
//! little biomass a minute and sleeps at an empty hold. A fresh bond works at once for a few
//! minutes without a slot.
//!
//! Four organs, each from a built power: the Remora (hull regeneration, from the symbiote), the
//! Faraday organ (jams and glitches run shorter, none at all at the top, from the Stormcap's
//! emp), the Veil (a dash leaves the ship intangible for a moment, from the Veilwing's phase)
//! and the Skipjack node (a dash hops a thin obstacle, from the blink). Numbers live in
//! `tuning`. Nothing here is random: magnitudes come from genomes and a hash of the spawn.

use super::tuning as t;
use super::*;
use crate::genome::Genome;
use crate::power::Power;

/// The kinds of organ. Every per-organ fact (label, donor power, aspect, whether an elder pays
/// it) is a row of `ORGANS`; adding an organ is a variant here plus a row there (and its effect).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Organ {
    Remora,
    Faraday,
    Veil,
    Skipjack,
}

/// What an organ is to the capability poset (`docs/CAPABILITIES.md` section 4.5): a ward is a
/// counter to a power, a gland is the power itself as an organ the ship uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Ward,
    Gland,
}

/// One row of the organ table.
#[derive(Clone, Copy, Debug)]
pub struct OrganKind {
    pub organ: Organ,
    /// The power whose carrier is the donor.
    pub power: Power,
    pub aspect: Aspect,
    pub label: &'static str,
    /// Whether a carrier or an elder of `power` may leave a specimen. The Remora is bonded,
    /// never harvested (shooting it is the moral joke).
    pub harvest: bool,
}

/// The organ table, in `Organ` declaration order (a test pins it). Slices K6 adds rows here.
pub const ORGANS: [OrganKind; 4] = [
    OrganKind {
        organ: Organ::Remora,
        power: Power::Symbiote,
        aspect: Aspect::Gland,
        label: "REMORA",
        harvest: false,
    },
    OrganKind {
        organ: Organ::Faraday,
        power: Power::Emp,
        aspect: Aspect::Ward,
        label: "FARADAY",
        harvest: true,
    },
    OrganKind {
        organ: Organ::Veil,
        power: Power::Phase,
        aspect: Aspect::Gland,
        label: "VEIL",
        harvest: true,
    },
    OrganKind {
        organ: Organ::Skipjack,
        power: Power::Blink,
        aspect: Aspect::Gland,
        label: "SKIP NODE",
        harvest: true,
    },
];

/// Number of organ kinds (sizes the saved arrays).
pub const ORGAN_COUNT: usize = ORGANS.len();

/// The organs a carrier of `power` can leave behind (a specimen), in table order.
pub fn harvestable(power: Power) -> impl Iterator<Item = &'static OrganKind> {
    ORGANS.iter().filter(move |k| k.harvest && k.power == power)
}

impl Organ {
    pub const ALL: [Organ; ORGAN_COUNT] = {
        let mut all = [Organ::Remora; ORGAN_COUNT];
        let mut i = 0;
        while i < ORGAN_COUNT {
            all[i] = ORGANS[i].organ;
            i += 1;
        }
        all
    };

    pub fn index(self) -> usize {
        ORGANS.iter().position(|k| k.organ == self).unwrap_or(0)
    }

    pub fn kind(self) -> &'static OrganKind {
        &ORGANS[self.index()]
    }

    pub fn label(self) -> &'static str {
        self.kind().label
    }

    /// The power whose carrier is the donor.
    pub fn power(self) -> Power {
        self.kind().power
    }

    pub fn aspect(self) -> Aspect {
        self.kind().aspect
    }

    /// What the organ answers, one line from the capability table (the same `reason` strings
    /// the readout uses), so a find says why it matters: "ANSWERS JAM: shortens jams ...".
    pub fn answers(self) -> String {
        crate::capability::organ_covers(self)
            .iter()
            .map(|c| format!("{}: {}", c.channel.label(), c.reason))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn tint(self) -> [f32; 3] {
        self.power().tint()
    }

    /// What the organ does at level 1 and magnitude 1, for the bench and the details.
    pub fn summary(self, tune: &Tunables) -> String {
        match self {
            Self::Remora => format!("mends {:.1} hull a second when quiet", tune.remora_regen),
            Self::Faraday => format!(
                "jams and glitches {:.0}% shorter, no HUD jam from level {}, none at level 3",
                tune.faraday_cut * 100.0,
                tune.faraday_hud_level
            ),
            Self::Veil => format!(
                "intangible {:.2}s after a dash, shots hit phased bodies meanwhile",
                tune.veil_time
            ),
            Self::Skipjack => format!("a dash hops walls under {:.0} thick", tune.skip_thick),
        }
    }
}

/// An owned strain: a kind, a level and a magnitude from the donor's genes.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Strain {
    pub organ: Organ,
    pub level: u8,
    /// 0.6 to 1.6.
    pub magnitude: f32,
}

impl Strain {
    /// The strain a donor of `organ` carries, from its genes: how strong its power runs, how
    /// well shielded and how big it is.
    pub fn from_donor(organ: Organ, donor: &Genome) -> Strain {
        let s = organ.power().strength(donor);
        let magnitude = 0.6
            + 0.6 * s
            + 0.2 * (donor.shield / 40.0).min(1.0)
            + 0.2 * (donor.radius / 20.0).min(1.0);
        Strain {
            organ,
            level: 1,
            magnitude: magnitude.clamp(0.6, 1.6),
        }
    }

    /// The perk's strength: magnitude times the level's gain.
    pub fn strength(&self, tune: &Tunables) -> f32 {
        let gain = match self.level.clamp(1, 3) {
            1 => tune.organ_level_gain_1,
            2 => tune.organ_level_gain_2,
            _ => tune.organ_level_gain_3,
        };
        self.magnitude * gain
    }
}

/// What finding a strain did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Found {
    New,
    Raised {
        from: u8,
        to: u8,
    },
    /// Same level, better genes.
    Improved,
    /// Nothing to gain: paid in volatiles.
    Lesser(f32),
}

/// The organs a run owns, which are fitted, and the bond running without a slot.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Organs {
    owned: [Option<Strain>; ORGAN_COUNT],
    /// Fitted kinds, oldest first, at most the slots open.
    slots: Vec<Organ>,
    /// Whether the first graft has been paid.
    paid: [bool; ORGAN_COUNT],
    /// A fresh bond working at once, and its seconds left.
    loan: Option<(Organ, f32)>,
    /// The hold is dry: fitted organs sleep.
    pub dormant: bool,
}

impl Organs {
    pub fn strain(&self, organ: Organ) -> Option<Strain> {
        self.owned[organ.index()]
    }

    pub fn owns(&self, organ: Organ) -> bool {
        self.owned[organ.index()].is_some()
    }

    /// Whether the first graft of this organ has been paid.
    pub fn grafted(&self, organ: Organ) -> bool {
        self.paid[organ.index()]
    }

    pub fn fitted(&self) -> &[Organ] {
        &self.slots
    }

    pub fn is_fitted(&self, organ: Organ) -> bool {
        self.slots.contains(&organ)
    }

    pub fn loan(&self) -> Option<(Organ, f32)> {
        self.loan
    }

    /// Takes a strain aboard under the no-downgrade rule.
    pub fn acquire(&mut self, found: Strain, tune: &Tunables) -> Found {
        let slot = &mut self.owned[found.organ.index()];
        match slot {
            None => {
                *slot = Some(found);
                Found::New
            }
            Some(have) if have.level < t::ORGAN_LEVELS => {
                let from = have.level;
                have.level += 1;
                have.magnitude = have.magnitude.max(found.magnitude);
                Found::Raised {
                    from,
                    to: have.level,
                }
            }
            Some(have) if found.magnitude > have.magnitude + 1e-3 => {
                have.magnitude = found.magnitude;
                Found::Improved
            }
            Some(have) => Found::Lesser(tune.lesser_biomass * f32::from(have.level)),
        }
    }

    /// The strain at work now: fitted and awake, or on the bond's loan.
    pub fn active(&self, organ: Organ) -> Option<Strain> {
        let strain = self.strain(organ)?;
        let on_loan = self.loan.is_some_and(|(o, _)| o == organ);
        ((self.is_fitted(organ) && !self.dormant) || on_loan).then_some(strain)
    }

    /// The perk strength of an active organ, or None.
    pub fn perk(&self, organ: Organ, tune: &Tunables) -> Option<f32> {
        self.active(organ).map(|s| s.strength(tune))
    }

    /// The best strain owned (highest level, then magnitude), for the legacy.
    pub fn best(&self) -> Option<Strain> {
        self.owned.iter().flatten().copied().max_by(|a, b| {
            (a.level, a.magnitude)
                .partial_cmp(&(b.level, b.magnitude))
                .unwrap()
        })
    }

    /// Fits `organ` into a slot (the oldest fitted one gives way when all `open` are taken) and
    /// says whether the graft was paid for already. None if not owned, already fitted or no
    /// slot is open at all.
    fn fit(&mut self, organ: Organ, open: usize) -> Option<bool> {
        if !self.owns(organ) || self.is_fitted(organ) || open == 0 {
            return None;
        }
        while self.slots.len() >= open {
            self.slots.remove(0);
        }
        self.slots.push(organ);
        Some(self.paid[organ.index()])
    }

    /// The organ a relic or specimen of `picked` should be: what the ship lacks, so a find
    /// fills a hole instead of duplicating (a seeded choice among organs not yet owned by
    /// `salt`). When everything is owned the pick stands and raises that organ a level.
    pub fn gap_for(&self, picked: Organ, salt: u64) -> Organ {
        if !self.owns(picked) {
            return picked;
        }
        let lacking: Vec<Organ> = Organ::ALL.into_iter().filter(|&o| !self.owns(o)).collect();
        match lacking.len() {
            0 => picked,
            n => lacking[(salt % n as u64) as usize],
        }
    }

    fn unfit(&mut self, organ: Organ) -> bool {
        let before = self.slots.len();
        self.slots.retain(|&o| o != organ);
        self.slots.len() < before
    }
}

/// Graft price of a strain at its level.
pub fn graft_price(strain: &Strain, tune: &Tunables) -> [(Material, f32); 2] {
    let k = f32::from(strain.level);
    [
        (Material::Crystal, tune.graft_crystal * k),
        (Material::Fuel, tune.graft_fuel * k),
    ]
}

/// The relic of a sector, if it has one: the organ and the strain, from a hash of the seed and
/// the sector alone (so a route always finds the same).
pub fn relic_of(
    seed: u64,
    sector: SectorId,
    depth: f32,
    tune: &Tunables,
) -> Option<(Strain, Vec2)> {
    if depth < tune.relic_from {
        return None;
    }
    let h = world::hash2(seed ^ RELIC_SALT, sector.x, sector.y);
    if !h.is_multiple_of(tune.relic_one_in) {
        return None;
    }
    let organ = Organ::ALL[((h >> 8) % ORGAN_COUNT as u64) as usize];
    let magnitude = 0.8 + 0.6 * ((h >> 16) % 1000) as f32 / 1000.0;
    let angle = ((h >> 28) % 6283) as f32 / 1000.0;
    let reach = 400.0 + ((h >> 40) % 1200) as f32;
    let at = sector.center() + Vec2::from_angle(angle) * reach;
    Some((
        Strain {
            organ,
            level: 1,
            magnitude,
        },
        at,
    ))
}

const RELIC_SALT: u64 = 0x0126_A117_0000_00B1;

impl Game {
    /// Takes a strain aboard (a specimen picked up, a relic, a bond) and tells the player.
    pub(super) fn take_strain(&mut self, found: Strain, source: &str) -> Found {
        let result = self.loadout.organs.acquire(found, &self.tune);
        let name = found.organ.label();
        match result {
            Found::New => {
                self.run.organs += 1;
                self.notify(
                    format!(
                        "{source}  {name} ORGAN  {}",
                        found.organ.summary(&self.tune)
                    ),
                    upgrades::Rarity::Epic,
                );
                // Additive and self-explanatory: say what it answers (fit it to use it).
                let answers = found.organ.answers();
                if !answers.is_empty() {
                    self.notify(format!("{name} ANSWERS {answers}"), upgrades::Rarity::Rare);
                }
            }
            Found::Raised { from, to } => self.notify(
                format!("{source}  {name} ORGAN {from} -> {to}"),
                upgrades::Rarity::Epic,
            ),
            Found::Improved => self.notify(
                format!("{source}  {name} ORGAN  a stronger strain"),
                upgrades::Rarity::Rare,
            ),
            Found::Lesser(volatiles) => {
                let taken = self.cargo.add(Material::Biomass, volatiles);
                self.notify(
                    format!("{name} ORGAN AT MAX  BIOMASS +{taken:.0}"),
                    upgrades::Rarity::Common,
                );
            }
        }
        result
    }

    /// A bond: the strain is owned and works at once for `bond_loan` seconds; a free slot takes
    /// it for good without the graft cost.
    pub(super) fn bond(&mut self, strain: Strain) {
        self.take_strain(strain, "BONDED");
        let organ = strain.organ;
        self.loadout.organs.loan = Some((organ, self.tune.bond_loan));
        let open = self.loadout.skills.organ_slots();
        let organs = &mut self.loadout.organs;
        if organs.slots.len() < open && organs.fit(organ, open).is_some() {
            organs.paid[organ.index()] = true;
        }
    }

    /// Ticks the organs: upkeep, sleep, the bond's loan and the Remora's mending.
    pub(super) fn update_organs(&mut self, dt: f32) {
        let organs = &mut self.loadout.organs;
        if let Some((_, left)) = organs.loan.as_mut() {
            *left -= dt;
        }
        if organs.loan.is_some_and(|(_, left)| left <= 0.0) {
            organs.loan = None;
        }
        let fitted = organs.slots.len();
        if fitted > 0 {
            let want = self.tune.organ_upkeep / 60.0 * fitted as f32 * dt;
            let taken = self.cargo.take(Material::Biomass, want);
            let dry = taken + 1e-6 < want || self.cargo.amount(Material::Biomass) <= 0.0;
            let organs = &mut self.loadout.organs;
            if dry {
                organs.dormant = true;
            } else if self.cargo.amount(Material::Biomass) >= 1.0 {
                organs.dormant = false;
            }
        } else {
            self.loadout.organs.dormant = false;
        }
        let grade = self.equipment_grade();
        if let Some(rate) = self.loadout.organs.perk(Organ::Remora, &self.tune)
            && let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player)
            && ship.health > 0.0
            && ship.since_hit >= self.tune.remora_quiet
        {
            ship.health =
                (ship.health + self.tune.remora_regen * rate * grade * dt).min(ship.max_health);
        }
    }

    /// Multiple of a jam's or glitch's length under the Faraday organ and a hardened casing
    /// (one without either, zero when the organ makes the ship immune). The two add: a casing
    /// only ever makes things better, and never lifts the organ's immunity.
    pub(super) fn jam_scale(&self) -> f32 {
        let organ = match self.loadout.organs.active(Organ::Faraday) {
            None => 1.0,
            Some(strain) if strain.level >= t::ORGAN_LEVELS => return 0.0,
            Some(strain) => {
                (1.0 - self.tune.faraday_cut * strain.strength(&self.tune)).clamp(0.0, 1.0)
            }
        };
        let hardening = f32::from(self.stats.hardening);
        let casing = (1.0 - self.tune.hardening_cut * hardening).clamp(0.0, 1.0);
        organ * casing
    }

    /// Fits or unfits the organ of a bench row. Returns what to say.
    pub fn bench_organ(&mut self, organ: Organ) -> Result<String, String> {
        let name = organ.label();
        if !self.loadout.organs.owns(organ) {
            return Err(format!("{name} IS NOT OWNED: BOND OR HARVEST ONE"));
        }
        if self.loadout.organs.is_fitted(organ) {
            self.loadout.organs.unfit(organ);
            return Ok(format!("{name} REMOVED (kept for the run)"));
        }
        let open = self.loadout.skills.organ_slots();
        if open == 0 {
            return Err("NO ORGAN SLOT: BUY SYMBIOSIS".into());
        }
        let Some(strain) = self.loadout.organs.strain(organ) else {
            return Err(format!("{name} IS NOT OWNED"));
        };
        if !self.loadout.organs.paid[organ.index()] {
            let price = graft_price(&strain, &self.tune);
            if !self.cargo.spend(&price) {
                return Err(format!(
                    "{name} GRAFT NEEDS {:.0} CRYSTAL {:.0} FUEL",
                    price[0].1, price[1].1
                ));
            }
        }
        if self.loadout.organs.fit(organ, open).is_some() {
            self.loadout.organs.paid[organ.index()] = true;
        }
        Ok(format!("{name} GRAFTED  level {}", strain.level))
    }

    /// The Veil's intangible seconds after a dash, if the organ is working.
    pub(super) fn veil_time(&self) -> Option<f32> {
        self.loadout
            .organs
            .perk(Organ::Veil, &self.tune)
            .map(|s| self.tune.veil_time * s)
    }

    /// The thickest obstacle a dash hops, if the Skipjack node is working.
    pub(super) fn skip_thickness(&self) -> Option<f32> {
        self.loadout
            .organs
            .perk(Organ::Skipjack, &self.tune)
            .map(|s| self.tune.skip_thick * s)
    }

    /// The organ the relic of sector `id` holds for this ship: the generated one, or the organ
    /// it lacks (a gap fill, seeded by the sector).
    pub(super) fn relic_organ(&self, id: SectorId, generated: Organ) -> Organ {
        let salt = world::hash2(self.seed ^ RELIC_SALT ^ 0x6A9, id.x, id.y);
        self.loadout.organs.gap_for(generated, salt)
    }

    /// Lays a sector's relic as a pickup the first time it loads and nothing like it lies there.
    pub(super) fn place_relic(&mut self, id: SectorId) {
        if self.relics_taken.contains(&id) {
            return;
        }
        let depth = world::latent(self.seed, id).depth;
        let Some((mut strain, at)) = relic_of(self.seed, id, depth, &self.tune) else {
            return;
        };
        if self.pickups.iter().any(|p| p.relic == Some(id)) {
            return;
        }
        strain.organ = self.relic_organ(id, strain.organ);
        self.drop_item(at, Vec2::ZERO, Item::Specimen(strain));
        if let Some(p) = self.pickups.last_mut() {
            p.remaining = 900.0;
            p.relic = Some(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::skills::Skill;
    use crate::simulation::tests::{DT, add, empty_game};

    fn strain(organ: Organ, level: u8, magnitude: f32) -> Strain {
        Strain {
            organ,
            level,
            magnitude,
        }
    }

    fn stocked() -> Game {
        let mut game = empty_game();
        game.player_invulnerability = 0.0;
        game.cargo = Cargo {
            metal: 100.0,
            volatiles: 100.0,
            fuel: 100.0,
            biomass: 100.0,
            crystal: 100.0,
            ..Default::default()
        };
        game
    }

    #[test]
    fn the_organ_table_is_the_single_source_of_per_organ_facts() {
        for (i, k) in ORGANS.iter().enumerate() {
            assert_eq!(Organ::ALL[i], k.organ);
            assert_eq!(k.organ.index(), i);
            assert_eq!(k.organ.label(), k.label);
            assert_eq!(k.organ.power(), k.power);
            assert!(!k.organ.answers().is_empty(), "{} answers nothing", k.label);
            assert!(!k.organ.answers().contains('\u{2014}'));
        }
        assert_eq!(
            harvestable(Power::Symbiote).count(),
            0,
            "bonded, never harvested"
        );
        let emp: Vec<_> = harvestable(Power::Emp).map(|k| k.organ).collect();
        assert_eq!(emp, [Organ::Faraday]);
        assert_eq!(Organ::Faraday.aspect(), Aspect::Ward);
        assert_eq!(Organ::Skipjack.aspect(), Aspect::Gland);
    }

    #[test]
    fn a_relic_fills_a_gap_and_only_duplicates_when_nothing_is_lacking() {
        let mut organs = Organs::default();
        organs.acquire(strain(Organ::Veil, 1, 1.0), &DEFAULT_TUNING);
        for salt in 0..40 {
            let organ = organs.gap_for(Organ::Veil, salt);
            assert!(!organs.owns(organ), "{organ:?} is already owned");
            assert_eq!(organs.gap_for(Organ::Veil, salt), organ, "deterministic");
        }
        assert_eq!(organs.gap_for(Organ::Faraday, 3), Organ::Faraday);
        for o in Organ::ALL {
            organs.acquire(strain(o, 1, 1.0), &DEFAULT_TUNING);
        }
        assert_eq!(organs.gap_for(Organ::Veil, 9), Organ::Veil);
    }

    #[test]
    fn a_hardened_casing_shortens_jams_and_adds_to_the_organ_without_immunity() {
        let mut game = stocked();
        assert_eq!(game.jam_scale(), 1.0);
        let cut = DEFAULT_TUNING.hardening_cut;
        let mut last = 1.0;
        for level in 1..=3_u8 {
            game.stats.hardening = level;
            let scale = game.jam_scale();
            assert!((scale - (1.0 - cut * f32::from(level))).abs() < 1e-5);
            assert!(scale < last && scale > 0.0, "{level}: {scale}");
            last = scale;
        }
        // Beside a level 1 Faraday the two add (multiply) and stay short of immunity.
        game.loadout
            .organs
            .acquire(strain(Organ::Faraday, 1, 1.0), &DEFAULT_TUNING);
        game.loadout.skills.raise(Skill::Symbiosis);
        assert!(game.bench_organ(Organ::Faraday).is_ok());
        let both = game.jam_scale();
        assert!(both < last && both > 0.0, "{both} against {last}");
        // The organ's own top level still makes the ship immune whatever the casing.
        game.loadout
            .organs
            .acquire(strain(Organ::Faraday, 1, 1.0), &DEFAULT_TUNING);
        game.loadout
            .organs
            .acquire(strain(Organ::Faraday, 1, 1.0), &DEFAULT_TUNING);
        assert_eq!(game.jam_scale(), 0.0);
    }

    #[test]
    fn finding_an_organ_says_what_it_answers() {
        let mut game = stocked();
        game.take_strain(strain(Organ::Faraday, 1, 1.0), "SPECIMEN");
        assert!(
            game.notices
                .iter()
                .any(|n| n.text.starts_with("FARADAY ANSWERS JAM: "))
        );
    }

    #[test]
    fn strains_only_rise_and_a_lesser_find_pays_volatiles() {
        let mut organs = Organs::default();
        assert_eq!(
            organs.acquire(strain(Organ::Veil, 1, 1.0), &DEFAULT_TUNING),
            Found::New
        );
        assert_eq!(
            organs.acquire(strain(Organ::Veil, 1, 0.7), &DEFAULT_TUNING),
            Found::Raised { from: 1, to: 2 }
        );
        assert_eq!(
            organs.acquire(strain(Organ::Veil, 1, 0.9), &DEFAULT_TUNING),
            Found::Raised { from: 2, to: 3 }
        );
        // Raised levels keep the better genes.
        assert_eq!(organs.strain(Organ::Veil).unwrap().magnitude, 1.0);
        // At the top: a weaker sample pays biomass and changes nothing; a stronger one improves.
        let before = organs.strain(Organ::Veil);
        let Found::Lesser(v) = organs.acquire(strain(Organ::Veil, 1, 0.6), &DEFAULT_TUNING) else {
            panic!("expected a lesser find");
        };
        assert_eq!(v, DEFAULT_TUNING.lesser_biomass * 3.0);
        assert_eq!(organs.strain(Organ::Veil), before);
        assert_eq!(
            organs.acquire(strain(Organ::Veil, 1, 1.4), &DEFAULT_TUNING),
            Found::Improved
        );
        assert_eq!(organs.strain(Organ::Veil).unwrap().level, 3);
        assert_eq!(organs.strain(Organ::Veil).unwrap().magnitude, 1.4);
        assert!(!organs.owns(Organ::Faraday));
    }

    #[test]
    fn magnitude_comes_from_the_donor_genes_inside_its_bounds() {
        let mut big = Genome::stormcap();
        big.emp = 1.0;
        big.shield = 60.0;
        big.radius = 40.0;
        let mut small = Genome::stormcap();
        small.emp = 0.35;
        small.shield = 0.0;
        small.radius = 6.0;
        let (a, b) = (
            Strain::from_donor(Organ::Faraday, &big),
            Strain::from_donor(Organ::Faraday, &small),
        );
        assert!(a.magnitude > b.magnitude + 0.3, "{a:?} {b:?}");
        for s in [a, b] {
            assert!((0.6..=1.6).contains(&s.magnitude));
        }
        // The same genome gives the same strain.
        assert_eq!(a, Strain::from_donor(Organ::Faraday, &big));
        // Levels scale the perk by the doc's 1, 1.5 and 2.
        let one = strain(Organ::Veil, 1, 1.0).strength(&DEFAULT_TUNING);
        assert_eq!(
            strain(Organ::Veil, 2, 1.0).strength(&DEFAULT_TUNING),
            one * 1.5
        );
        assert_eq!(
            strain(Organ::Veil, 3, 1.0).strength(&DEFAULT_TUNING),
            one * 2.0
        );
    }

    #[test]
    fn a_graft_needs_symbiosis_pays_once_and_a_swap_is_free() {
        let mut game = stocked();
        game.loadout
            .organs
            .acquire(strain(Organ::Veil, 2, 1.0), &DEFAULT_TUNING);
        game.loadout
            .organs
            .acquire(strain(Organ::Faraday, 1, 1.0), &DEFAULT_TUNING);
        assert!(game.bench_organ(Organ::Veil).is_err(), "no slot yet");
        game.loadout.skills.raise(Skill::Symbiosis);
        assert_eq!(game.loadout.skills.organ_slots(), 1);
        // A level 2 graft: 2 x (8 crystal, 20 volatiles).
        assert!(game.bench_organ(Organ::Veil).is_ok());
        assert_eq!(
            game.cargo.crystal,
            100.0 - DEFAULT_TUNING.graft_crystal * 2.0
        );
        assert_eq!(game.cargo.fuel, 100.0 - DEFAULT_TUNING.graft_fuel * 2.0);
        assert!(game.loadout.organs.is_fitted(Organ::Veil));
        // One slot: a second organ bumps the first, never destroying it.
        assert!(game.bench_organ(Organ::Faraday).is_ok());
        assert!(game.loadout.organs.is_fitted(Organ::Faraday));
        assert!(!game.loadout.organs.is_fitted(Organ::Veil));
        assert!(game.loadout.organs.owns(Organ::Veil));
        // Swapping back costs nothing: the graft was paid.
        let (c, v) = (game.cargo.crystal, game.cargo.biomass);
        assert!(game.bench_organ(Organ::Veil).is_ok());
        assert_eq!((game.cargo.crystal, game.cargo.biomass), (c, v));
        // Short of the price: refused, nothing spent.
        let mut poor = stocked();
        poor.loadout.skills.raise(Skill::Symbiosis);
        poor.loadout
            .organs
            .acquire(strain(Organ::Skipjack, 3, 1.0), &DEFAULT_TUNING);
        poor.cargo.crystal = 10.0;
        assert!(poor.bench_organ(Organ::Skipjack).is_err());
        assert_eq!(poor.cargo.crystal, 10.0);
        assert!(!poor.loadout.organs.is_fitted(Organ::Skipjack));
        // Unowned organs cannot be grafted.
        assert!(game.bench_organ(Organ::Remora).is_err());
        // Slots never exceed the skill.
        for _ in 0..5 {
            game.loadout.skills.raise(Skill::Symbiosis);
        }
        assert_eq!(game.loadout.skills.organ_slots(), t::SYMBIOSIS_SLOTS);
    }

    #[test]
    fn symbiosis_needs_a_rare_core_and_starts_locked() {
        use crate::simulation::upgrades::{Rarity, Slot};
        assert!(Skill::Symbiosis.starts_locked());
        assert_eq!(
            Skill::Symbiosis.requirement(),
            Some((Slot::Core, Rarity::Rare))
        );
        let mut game = stocked();
        assert!(game.skill_gate(Skill::Symbiosis).is_some());
        assert_eq!(game.loadout.skills.organ_slots(), 0);
        let _ = &mut game;
    }

    #[test]
    fn fitted_organs_cost_biomass_by_the_minute_and_sleep_when_the_hold_is_dry() {
        let mut game = stocked();
        game.loadout.skills.raise(Skill::Symbiosis);
        game.loadout.skills.raise(Skill::Symbiosis);
        game.loadout
            .organs
            .acquire(strain(Organ::Remora, 1, 1.0), &DEFAULT_TUNING);
        game.loadout
            .organs
            .acquire(strain(Organ::Veil, 1, 1.0), &DEFAULT_TUNING);
        game.bench_organ(Organ::Remora).unwrap();
        game.bench_organ(Organ::Veil).unwrap();
        game.cargo.biomass = 50.0;
        for _ in 0..(60.0 / DT) as usize {
            game.update_organs(DT);
        }
        let spent = 50.0 - game.cargo.biomass;
        assert!(
            (spent - 2.0 * DEFAULT_TUNING.organ_upkeep).abs() < 0.05,
            "two organs for a minute: {spent}"
        );
        assert!(
            game.loadout
                .organs
                .perk(Organ::Veil, &DEFAULT_TUNING)
                .is_some()
        );
        // A dry hold puts them to sleep, nothing is lost, and feeding it wakes them.
        game.cargo.biomass = 0.0;
        game.update_organs(DT);
        assert!(game.loadout.organs.dormant);
        assert!(
            game.loadout
                .organs
                .perk(Organ::Veil, &DEFAULT_TUNING)
                .is_none()
        );
        assert!(
            game.loadout.organs.owns(Organ::Veil) && game.loadout.organs.is_fitted(Organ::Veil)
        );
        game.cargo.biomass = 5.0;
        game.update_organs(DT);
        assert!(!game.loadout.organs.dormant);
        assert!(
            game.loadout
                .organs
                .perk(Organ::Veil, &DEFAULT_TUNING)
                .is_some()
        );
        // No fitted organ, no upkeep.
        let mut idle = stocked();
        idle.loadout
            .organs
            .acquire(strain(Organ::Veil, 1, 1.0), &DEFAULT_TUNING);
        for _ in 0..600 {
            idle.update_organs(DT);
        }
        assert_eq!(idle.cargo.volatiles, 100.0);
    }

    fn fit(game: &mut Game, organ: Organ, level: u8, magnitude: f32) {
        game.loadout.skills.raise(Skill::Symbiosis);
        game.loadout
            .organs
            .acquire(strain(organ, 1, magnitude), &DEFAULT_TUNING);
        for _ in 1..level {
            game.loadout
                .organs
                .acquire(strain(organ, 1, magnitude), &DEFAULT_TUNING);
        }
        game.bench_organ(organ).unwrap();
    }

    #[test]
    fn the_remora_mends_the_hull_only_when_quiet_and_more_at_higher_levels() {
        let mend = |level: u8, quiet: bool| {
            let mut game = stocked();
            fit(&mut game, Organ::Remora, level, 1.0);
            game.bodies[0].health = 40.0;
            game.bodies[0].since_hit = if quiet { 5.0 } else { 0.0 };
            for _ in 0..60 {
                game.update_organs(DT);
            }
            game.bodies[0].health - 40.0
        };
        assert!(mend(1, true) > 0.7, "{}", mend(1, true));
        assert_eq!(mend(1, false), 0.0);
        assert!(mend(2, true) > mend(1, true) && mend(3, true) > mend(2, true));
        let mut capped = stocked();
        fit(&mut capped, Organ::Remora, 3, 1.6);
        capped.bodies[0].since_hit = 9.0;
        for _ in 0..600 {
            capped.update_organs(DT);
        }
        assert!(capped.bodies[0].health <= capped.bodies[0].max_health);
    }

    #[test]
    fn the_faraday_organ_shortens_jams_and_makes_level_three_immune() {
        let jam_for = |level: Option<u8>| {
            let mut game = stocked();
            if let Some(level) = level {
                fit(&mut game, Organ::Faraday, level, 1.0);
            }
            let ok = game.apply_jam(&[JamSystem::Weapons], 1.4);
            (ok, game.jam_view().weapons)
        };
        let (ok0, bare) = jam_for(None);
        let (ok1, one) = jam_for(Some(1));
        let (ok2, two) = jam_for(Some(2));
        assert!(ok0 && ok1 && ok2);
        assert!(one < bare && two < one, "{bare} {one} {two}");
        let (ok3, _) = jam_for(Some(3));
        assert!(!ok3, "level 3 is immune");
        let mut glitch = stocked();
        fit(&mut glitch, Organ::Faraday, 3, 1.0);
        assert!(!glitch.apply_glitch(1.5, 3));
        assert!(!glitch.apply_confuse(0.4, false, 1.0, 1.0));
        // A sleeping organ does nothing.
        let mut asleep = stocked();
        fit(&mut asleep, Organ::Faraday, 3, 1.0);
        asleep.loadout.organs.dormant = true;
        assert!(asleep.apply_jam(&[JamSystem::Weapons], 1.0));
    }

    #[test]
    fn the_veil_makes_a_dash_leave_the_ship_intangible_for_a_moment() {
        let mut game = stocked();
        game.loadout.skills.raise(Skill::Dash);
        game.bodies[0].shield = 60.0;
        game.bodies[0].max_shield = 60.0;
        fit(&mut game, Organ::Veil, 1, 1.0);
        let wall = add(&mut game, BodyKind::Creature, Vec2::new(300.0, 0.0));
        assert!(game.dash(Some(Vec2::X)));
        assert!(game.player().unwrap().phased);
        // Nothing touches a phased ship, and the effect ends.
        game.bodies
            .iter_mut()
            .find(|b| b.id == wall)
            .unwrap()
            .position = game.player().unwrap().position;
        let before = game.player().unwrap().velocity;
        game.resolve_contacts();
        assert_eq!(game.player().unwrap().velocity, before);
        let len = game.veil;
        assert!(len > 0.3 && len < 0.5, "{len}");
        for _ in 0..40 {
            game.update_dash(DT);
        }
        assert!(!game.player().unwrap().phased);
        // No organ, no phase.
        let mut plain = stocked();
        plain.loadout.skills.raise(Skill::Dash);
        plain.bodies[0].shield = 60.0;
        assert!(plain.dash(Some(Vec2::X)));
        assert!(!plain.player().unwrap().phased);
    }

    #[test]
    fn the_skip_node_hops_a_thin_wall_and_lands_clear_but_not_a_thick_one() {
        let setup = |skip: bool, radius: f32| {
            let mut game = stocked();
            game.loadout.skills.raise(Skill::Dash);
            game.bodies[0].shield = 60.0;
            game.bodies[0].max_shield = 60.0;
            if skip {
                fit(&mut game, Organ::Skipjack, 1, 1.0);
            }
            let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(120.0, 0.0));
            let r = game.bodies.iter_mut().find(|b| b.id == rock).unwrap();
            r.radius = radius;
            r.pinned = true;
            assert!(game.dash(Some(Vec2::X)));
            game.player().unwrap().position.x
        };
        let blocked = setup(false, 20.0);
        let hopped = setup(true, 20.0);
        assert!(blocked < 100.0, "{blocked}");
        assert!(hopped > 150.0, "{hopped}");
        // A body wider than the node's reach still stops the dash.
        let thick = setup(true, 60.0);
        assert!(thick < 70.0, "{thick}");
        // The landing is never inside a body: put a rock right where the full dash would end.
        let mut game = stocked();
        game.loadout.skills.raise(Skill::Dash);
        game.bodies[0].shield = 60.0;
        fit(&mut game, Organ::Skipjack, 1, 1.0);
        for (x, r) in [(100.0, 15.0), (240.0, 25.0)] {
            let id = add(&mut game, BodyKind::Asteroid, Vec2::new(x, 0.0));
            let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            b.radius = r;
            b.pinned = true;
        }
        assert!(game.dash(Some(Vec2::X)));
        let at = game.player().unwrap().position;
        for b in game.bodies.iter().filter(|b| b.kind == BodyKind::Asteroid) {
            assert!(
                b.position.distance(at) > b.radius + 14.0,
                "{at:?} inside {b:?}"
            );
        }
    }

    fn carrier_drops(genome: Genome, index: u32) -> usize {
        let mut game = empty_game();
        let id = add(&mut game, BodyKind::Creature, Vec2::new(0.0, 5000.0));
        let mut body = game.bodies.iter().find(|b| b.id == id).unwrap().clone();
        body.genome = genome;
        body.origin = Some((SectorId { x: 3, y: 4 }, index));
        game.pickups.clear();
        game.drop_loot(&body);
        game.pickups
            .iter()
            .filter(|p| matches!(p.item, Item::Specimen(_)))
            .count()
    }

    #[test]
    fn a_carriers_first_kill_leaves_a_specimen_about_one_time_in_four_the_same_every_time() {
        let hits: usize = (0..400)
            .map(|i| usize::from(carrier_drops(Genome::skipjack(), i) > 0))
            .sum();
        assert!((70..=130).contains(&hits), "{hits} of 400");
        for i in 0..40 {
            assert_eq!(
                carrier_drops(Genome::veilwing(), i),
                carrier_drops(Genome::veilwing(), i),
                "the same spawn pays the same"
            );
        }
        // Not a carrier, the remora (bonded, never harvested) or a raised creature: nothing.
        assert_eq!(
            (0..200)
                .map(|i| carrier_drops(Genome::default(), i))
                .sum::<usize>(),
            0
        );
        assert_eq!(
            (0..200)
                .map(|i| carrier_drops(Genome::remora(), i))
                .sum::<usize>(),
            0
        );
        let mut game = empty_game();
        let id = add(&mut game, BodyKind::Creature, Vec2::new(0.0, 5000.0));
        let mut body = game.bodies.iter().find(|b| b.id == id).unwrap().clone();
        body.genome = Genome::stormcap();
        for _ in 0..200 {
            game.drop_loot(&body);
        }
        assert!(
            game.pickups
                .iter()
                .all(|p| !matches!(p.item, Item::Specimen(_)))
        );
    }

    #[test]
    fn a_specimen_pickup_is_owned_and_a_second_raises_it() {
        let mut game = stocked();
        let donor = Strain::from_donor(Organ::Skipjack, &Genome::skipjack());
        game.collect(Item::Specimen(donor));
        assert_eq!(
            game.loadout.organs.strain(Organ::Skipjack).unwrap().level,
            1
        );
        assert_eq!(game.run.organs, 1);
        game.collect(Item::Specimen(donor));
        assert_eq!(
            game.loadout.organs.strain(Organ::Skipjack).unwrap().level,
            2
        );
        game.collect(Item::Specimen(donor));
        let v = game.cargo.biomass;
        game.collect(Item::Specimen(donor));
        assert_eq!(
            game.loadout.organs.strain(Organ::Skipjack).unwrap().level,
            3
        );
        assert_eq!(game.cargo.biomass, v + DEFAULT_TUNING.lesser_biomass * 3.0);
    }

    #[test]
    fn relics_are_deterministic_rare_and_pay_once() {
        let seed = 7;
        let found: Vec<_> = (-30..30)
            .flat_map(|x| (-30..30).map(move |y| SectorId { x, y }))
            .filter_map(|id| relic_of(seed, id, 5.0, &DEFAULT_TUNING).map(|r| (id, r)))
            .collect();
        let share = found.len() as f32 / 3600.0;
        assert!((0.04..0.11).contains(&share), "{share}");
        for (id, (strain, at)) in &found {
            assert_eq!(
                relic_of(seed, *id, 5.0, &DEFAULT_TUNING),
                Some((*strain, *at))
            );
            assert_eq!(SectorId::containing(*at), *id, "inside its own sector");
            assert!((0.8..=1.4).contains(&strain.magnitude));
            assert_eq!(
                relic_of(seed, *id, 1.0, &DEFAULT_TUNING),
                None,
                "not near home"
            );
        }
        let (id, (strain, at)) = found[0];
        let mut game = Game::new(seed);
        game.pickups.clear();
        game.place_relic(id);
        // Depth gates it: only if the generator puts this sector deep enough.
        let deep = world::latent(seed, id).depth >= DEFAULT_TUNING.relic_from;
        assert_eq!(
            game.pickups
                .iter()
                .any(|p| p.item == Item::Specimen(strain) && p.position == at),
            deep
        );
        if deep {
            game.place_relic(id);
            assert_eq!(game.pickups.len(), 1, "never twice");
            game.relics_taken.insert(id);
            game.pickups.clear();
            game.place_relic(id);
            assert!(game.pickups.is_empty(), "taken relics stay taken");
        }
    }

    #[test]
    fn organs_survive_death_clear_on_restart_and_one_rides_the_legacy() {
        let mut game = stocked();
        game.loadout.skills.raise(Skill::Symbiosis);
        game.loadout
            .organs
            .acquire(strain(Organ::Veil, 3, 1.2), &DEFAULT_TUNING);
        game.loadout
            .organs
            .acquire(strain(Organ::Faraday, 1, 1.0), &DEFAULT_TUNING);
        game.bench_organ(Organ::Veil).unwrap();
        game.player_invulnerability = 0.0;
        game.bodies[0].health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.lives, 2);
        assert!(
            game.loadout.organs.is_fitted(Organ::Veil),
            "kept through death"
        );
        assert_eq!(game.loadout.organs.strain(Organ::Veil).unwrap().level, 3);
        // The legacy carries the best one at level 1 (insured runs only), and a restart clears
        // everything else.
        game.pad.insured = true;
        let b = game.bequest(Vec2::ZERO, None);
        assert_eq!(b.organ.map(|s| (s.organ, s.level)), Some((Organ::Veil, 1)));
        game.pad.insured = false;
        assert_eq!(game.bequest(Vec2::ZERO, None).organ, None);
        game.pad.insured = true;
        game.lives = 0;
        game.game_over = true;
        game.seal_bequest(Vec2::ZERO, None);
        let next = game.next_run();
        assert_eq!(next.loadout.organs.strain(Organ::Veil).unwrap().level, 1);
        assert!(!next.loadout.organs.owns(Organ::Faraday));
        assert!(!next.loadout.organs.is_fitted(Organ::Veil));
        assert_eq!(next.loadout.skills.organ_slots(), 0);
        let mut reset = stocked();
        reset
            .loadout
            .organs
            .acquire(strain(Organ::Veil, 2, 1.0), &DEFAULT_TUNING);
        reset.reset();
        assert!(!reset.loadout.organs.owns(Organ::Veil));
    }

    #[test]
    fn organs_are_deterministic() {
        let run = || {
            let mut game = stocked();
            fit(&mut game, Organ::Remora, 2, 1.1);
            game.collect(Item::Specimen(Strain::from_donor(
                Organ::Faraday,
                &Genome::stormcap(),
            )));
            game.bodies[0].health = 30.0;
            game.bodies[0].since_hit = 5.0;
            for _ in 0..600 {
                game.step(DT, Input::default());
            }
            (
                game.bodies[0].health,
                game.cargo.biomass,
                game.loadout.organs.clone(),
            )
        };
        assert_eq!(run(), run());
    }
}
