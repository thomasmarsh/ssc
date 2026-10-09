//! Flora: what plants are made of and who can eat them.
//!
//! Edibility is a relation, not a property of the plant. Every plant species carries a
//! `Chemistry` (how much sugar, fibre, resin, mineral and toxin it holds) and every consumer
//! carries a `Palate` (how much it values each of those). Nutrition is the dot product,
//! clamped to [0, 1], and a meal counts when it reaches `EDIBLE`. So the same plant can be a
//! crop for the ship and invisible to creatures, a creature forage the ship ignores (plankton
//! is the classic: all sugar), good for both, or good for neither, and nothing in the rules
//! names a plant or a creature kind. The ship's palate is a constant; a creature's palate is
//! a pure function of the master seed and its lineage (`creature_palate`), so a species keeps
//! its taste, relatives do not share it, and nothing wild learns it.
//!
//! The species table (`species`) is a pure function of the master seed: a fixed number of
//! species per universe, each with a chemistry, a plant grammar genome (its shape), a growth
//! time and a name. Which species grow on which planetoid is decided in
//! `simulation::farm`, not here.

use crate::grammar::{Domain, GrammarSpecimen, stream};
use crate::world::Rng;

/// The chemical axes. Index with `Axis as usize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Sugar,
    Fibre,
    Resin,
    Mineral,
    Toxin,
}

pub const AXES: usize = 5;
pub const AXIS_NAMES: [&str; AXES] = ["SUGAR", "FIBRE", "RESIN", "MINERAL", "TOXIN"];

/// A meal must reach this nutrition to count as food to a consumer.
pub const EDIBLE: f32 = 0.3;

/// How many plant species a universe has.
pub const SPECIES: u16 = 12;

const FLORA_SALT: u64 = 0xF10A_5EED_0000_0001;
const PALATE_SALT: u64 = 0xF10A_7A57_0000_0002;

/// What a plant is made of, each axis in [0, 1].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Chemistry(pub [f32; AXES]);

impl Chemistry {
    /// Chemistry is the plant's second gene set (the first is its `GrammarGenome`), so it
    /// breeds the same way: a point mutation nudges one axis, crossover picks per axis.
    pub fn mutate(mut self, rng: &mut Rng) -> Self {
        let axis = rng.int(0, AXES as u32 - 1) as usize;
        self.0[axis] = (self.0[axis] + rng.range(-0.25, 0.25)).clamp(0.0, 1.0);
        self
    }

    pub fn crossover(a: Self, b: Self, rng: &mut Rng) -> Self {
        let mut out = a;
        for (slot, other) in out.0.iter_mut().zip(b.0) {
            if rng.chance(0.5) {
                *slot = other;
            }
        }
        out
    }
}

/// What a consumer values, each axis in [-2, 1]. A negative weight is a poison or a dislike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palate(pub [f32; AXES]);

impl Palate {
    /// Nutrition of a plant of this chemistry, in [0, 1].
    pub fn nutrition(&self, chemistry: &Chemistry) -> f32 {
        self.0
            .iter()
            .zip(chemistry.0.iter())
            .map(|(w, c)| w * c)
            .sum::<f32>()
            .clamp(0.0, 1.0)
    }

    pub fn eats(&self, chemistry: &Chemistry) -> bool {
        self.nutrition(chemistry) >= EDIBLE
    }
}

/// Plankton is a plant too: almost pure sugar. Every grazing palate accepts it (grazers are
/// built to), the ship's does not.
pub const PLANKTON: Chemistry = Chemistry([0.8, 0.0, 0.0, 0.05, 0.0]);

/// The ship and its farm: fibre and resin make biomass, a little mineral helps, toxin ruins
/// it, and sugar alone is ignored.
pub const SHIP_PALATE: Palate = Palate([0.0, 0.8, 0.6, 0.2, -2.0]);

/// The palate of the creatures of one lineage. Grazers always value sugar (they live on
/// plankton), the rest of their taste is drawn per lineage: some relish fibre, some resin,
/// some shrug off toxin, some find mineral sweet.
pub fn creature_palate(master: u64, lineage: u64) -> Palate {
    let mut rng = Rng::new(master ^ PALATE_SALT ^ lineage.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    rng.next_u64();
    Palate([
        rng.range(0.5, 1.0),
        rng.range(-1.0, 0.7),
        rng.range(-1.0, 0.7),
        rng.range(-0.6, 0.6),
        rng.range(-1.0, 0.4),
    ])
}

/// One plant species of the universe.
#[derive(Clone, Debug, PartialEq)]
pub struct Flora {
    pub id: u16,
    pub name: String,
    pub chemistry: Chemistry,
    /// The shape: every plant of the species shares the genome; its own seed varies the
    /// stochastic choices.
    pub genome: crate::grammar::GrammarGenome,
    /// Seconds from seedling to ripe.
    pub grow_secs: f32,
    /// Tint of the leaves (linear-ish sRGB), shifted by chemistry so kinds read at a glance.
    pub tint: [f32; 3],
}

impl Flora {
    /// Nutrition for the ship (what a harvest is worth).
    pub fn ship_nutrition(&self) -> f32 {
        SHIP_PALATE.nutrition(&self.chemistry)
    }

    /// Whether the ship's farm counts it as a crop (beam harvest, seeds, biomass).
    pub fn is_crop(&self) -> bool {
        self.ship_nutrition() >= EDIBLE
    }

    pub fn specimen(&self, seed: u64) -> GrammarSpecimen {
        GrammarSpecimen {
            genome: self.genome,
            seed,
        }
    }
}

/// The role each species slot is drawn to fill, so every universe has crops, shared crops,
/// forage and weeds in good measure.
const ROLES: [Role; SPECIES as usize] = [
    Role::Shared,
    Role::CropOnly,
    Role::Forage,
    Role::Weed,
    Role::CropOnly,
    Role::Forage,
    Role::Shared,
    Role::Forage,
    Role::CropOnly,
    Role::Weed,
    Role::Forage,
    Role::Shared,
];

const SYLLABLES: [&str; 12] = [
    "AL", "BRA", "CO", "DUN", "EL", "FEN", "GRI", "HOL", "IV", "KEL", "LUM", "MOS",
];
const ENDINGS: [&str; 6] = ["ROOT", "WORT", "BLOOM", "FROND", "VINE", "CAP"];

/// Species `id` of a universe (ids wrap into `0..SPECIES`).
pub fn species(master: u64, id: u16) -> Flora {
    let id = id % SPECIES;
    let mut rng = stream(master ^ FLORA_SALT, Domain::Plant, u64::from(id));
    // Each axis is either rich or poor, so the table has real contrast rather than a grey
    // average: that is what makes crops, forage and weeds separate.
    // The table is built so a universe always has every role (see `ROLES`): chemistries are
    // drawn until the species lands in the role its slot asks for, against the reference
    // crowd of palates, so the draw count is still a pure function of the seed and the id.
    let crowd = sample_palates(master, 64);
    let want = ROLES[id as usize % ROLES.len()];
    let mut chemistry = Chemistry([0.0; AXES]);
    let specimen = GrammarSpecimen::for_entity(master ^ FLORA_SALT, Domain::Plant, u64::from(id));
    for _ in 0..200 {
        // Each axis is either rich or poor, so the table has real contrast rather than a
        // grey average: that is what makes crops, forage and weeds separate.
        let mut axis = |rich: f32| {
            if rng.chance(rich) {
                rng.range(0.55, 1.0)
            } else {
                rng.range(0.0, 0.2)
            }
        };
        chemistry = Chemistry([axis(0.45), axis(0.5), axis(0.4), axis(0.3), axis(0.2)]);
        let candidate = Flora {
            id,
            name: String::new(),
            chemistry,
            genome: specimen.genome,
            grow_secs: 0.0,
            tint: [0.0; 3],
        };
        if role(&candidate, &crowd) == want {
            break;
        }
    }
    let grow_secs = rng.range(240.0, 600.0);
    // The first syllable walks the table from a per-universe offset, so names never repeat.
    let offset = (stream(master ^ FLORA_SALT, Domain::Gallery, 0).next_u64()
        % SYLLABLES.len() as u64) as usize;
    let name = format!(
        "{}{}",
        SYLLABLES[(usize::from(id) + offset) % SYLLABLES.len()],
        ENDINGS[rng.int(0, ENDINGS.len() as u32 - 1) as usize]
    );
    let c = chemistry.0;
    let tint = [
        (0.25 + 0.5 * c[4] + 0.2 * c[2]).min(1.0),
        (0.55 + 0.35 * c[1] - 0.15 * c[4]).clamp(0.2, 1.0),
        (0.25 + 0.5 * c[3] + 0.2 * c[0]).min(1.0),
    ];
    Flora {
        id,
        name,
        chemistry,
        genome: specimen.genome,
        grow_secs,
        tint,
    }
}

/// Who a species feeds, among a set of palates: the share of them that eat it.
pub fn eaten_by_share(flora: &Flora, palates: &[Palate]) -> f32 {
    if palates.is_empty() {
        return 0.0;
    }
    palates.iter().filter(|p| p.eats(&flora.chemistry)).count() as f32 / palates.len() as f32
}

/// A reference crowd of creature palates (for tests and tools).
pub fn sample_palates(master: u64, count: u64) -> Vec<Palate> {
    (0..count)
        .map(|n| creature_palate(master, n.wrapping_mul(0x1234_5678_9ABC_DEF1) ^ 0xAB))
        .collect()
}

/// The four relations a species can have to the ship and to creatures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Ship crop that creatures also eat.
    Shared,
    /// Ship crop that creatures ignore.
    CropOnly,
    /// Creature forage that the ship ignores (plankton-like).
    Forage,
    /// Nobody eats it: scenery, or poison.
    Weed,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Self::Shared => "CROP + FORAGE",
            Self::CropOnly => "CROP",
            Self::Forage => "FORAGE",
            Self::Weed => "WEED",
        }
    }
}

/// Classifies a species against a crowd of creature palates: eaten by at least a fifth of
/// them counts as forage.
pub fn role(flora: &Flora, palates: &[Palate]) -> Role {
    let forage = eaten_by_share(flora, palates) >= 0.2;
    match (flora.is_crop(), forage) {
        (true, true) => Role::Shared,
        (true, false) => Role::CropOnly,
        (false, true) => Role::Forage,
        (false, false) => Role::Weed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_deterministic_and_seed_sensitive() {
        for id in 0..SPECIES {
            assert_eq!(species(42, id), species(42, id));
        }
        assert!((0..SPECIES).any(|id| species(42, id).chemistry != species(43, id).chemistry));
        assert_eq!(species(42, 3), species(42, 3 + SPECIES));
    }

    #[test]
    fn nutrition_is_bounded_and_edibility_is_a_relation() {
        let tox = Chemistry([0.0, 1.0, 1.0, 0.0, 1.0]);
        assert_eq!(SHIP_PALATE.nutrition(&tox), 0.0, "toxin ruins a rich plant");
        let sweet = PLANKTON;
        assert!(!SHIP_PALATE.eats(&sweet), "the ship ignores plankton");
        for seed in 0..40 {
            for lineage in 0..40 {
                let palate = creature_palate(seed, lineage);
                assert!(palate.eats(&PLANKTON), "every grazer eats plankton");
                for id in 0..SPECIES {
                    let n = palate.nutrition(&species(seed, id).chemistry);
                    assert!((0.0..=1.0).contains(&n));
                }
            }
        }
    }

    #[test]
    fn palates_are_stable_per_lineage_and_differ_between_them() {
        assert_eq!(creature_palate(7, 99), creature_palate(7, 99));
        assert_ne!(creature_palate(7, 99), creature_palate(7, 100));
        assert_ne!(creature_palate(7, 99), creature_palate(8, 99));
    }

    #[test]
    fn every_universe_has_all_four_roles() {
        for seed in [1u64, 7, 42, 99, 12345, 0x53_5343] {
            let crowd = sample_palates(seed, 64);
            let mut seen = [false; 4];
            for id in 0..SPECIES {
                let r = role(&species(seed, id), &crowd);
                seen[r as usize] = true;
            }
            assert!(seen.iter().all(|s| *s), "seed {seed}: roles {seen:?}");
        }
    }

    #[test]
    fn crops_are_a_minority_and_species_names_differ() {
        let names: std::collections::HashSet<String> =
            (0..SPECIES).map(|id| species(42, id).name).collect();
        assert_eq!(names.len(), usize::from(SPECIES));
        let crowd = sample_palates(42, 64);
        let roles: Vec<Role> = (0..SPECIES)
            .map(|id| role(&species(42, id), &crowd))
            .collect();
        let crops = roles
            .iter()
            .filter(|r| matches!(r, Role::Shared | Role::CropOnly))
            .count();
        assert!((2..=9).contains(&crops), "{crops} crops of {SPECIES}");
    }
}
