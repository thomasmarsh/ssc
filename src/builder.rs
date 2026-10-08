//! Builder creatures (workstream 11): an optional genome section, the build-behavior gene
//! set. A builder carries what it gathers (`material`), the structure grammar it follows
//! (`plan`, a `GrammarGenome`), how big the work is (`scale`, `blocks`) and how patient it is
//! (`patience`). Nothing here is an enum branch of a creature kind: a builder is any genome
//! that has this section, and its structure is the grammar's `Plan` realized over time (see
//! `structure` and `simulation/build.rs`).
//!
//! Slice one: the section, the blueprint and a hand-authored specimen. No wild creature has
//! the section yet, so generation (and the HOME golden) is untouched; a nest builder species
//! placed by niche is slice two.

use crate::grammar::{self, Domain, GrammarGenome, Template};
use crate::structure::{MAX_SITES, StructurePlan};
use crate::world::{Rng, RockKind};
use std::f32::consts::TAU;

/// What a builder gathers and lays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Stone,
    Ice,
    Ore,
}

impl Material {
    /// The rock kind of a block of this material.
    pub fn rock(self) -> RockKind {
        match self {
            Material::Stone => RockKind::Plain,
            Material::Ice => RockKind::Ice,
            Material::Ore => RockKind::Ore,
        }
    }

    /// Health and mass multipliers, as the generator gives the same kind of rock.
    pub fn toughness_density(self) -> (f32, f32) {
        match self {
            Material::Stone => (1.0, 1.0),
            Material::Ice => (0.7, 0.8),
            Material::Ore => (1.6, 2.2),
        }
    }
}

/// Smallest and largest block, in world units.
pub const BLOCK_MIN: f32 = 7.0;
pub const BLOCK_MAX: f32 = 16.0;
/// Bounds of the genes, so every section is valid.
pub const SCALE: (f32, f32) = (30.0, 90.0);
pub const PATIENCE: (f32, f32) = (1.0, 12.0);

/// The build-behavior gene set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Builder {
    pub material: Material,
    /// The structure grammar it follows.
    pub plan: GrammarGenome,
    /// World units per plan unit: how large its structures are.
    pub scale: f32,
    /// How many blocks the finished structure has, at most `MAX_SITES`.
    pub blocks: u8,
    /// Seconds of work per block before the plan's own stem lengths weigh in.
    pub patience: f32,
}

impl Builder {
    /// The same section forced into range.
    pub fn limited(self) -> Self {
        Self {
            plan: self.plan.limited(),
            scale: if self.scale.is_finite() {
                self.scale.clamp(SCALE.0, SCALE.1)
            } else {
                SCALE.0
            },
            blocks: self.blocks.clamp(1, MAX_SITES as u8),
            patience: if self.patience.is_finite() {
                self.patience.clamp(PATIENCE.0, PATIENCE.1)
            } else {
                PATIENCE.1
            },
            ..self
        }
    }

    /// A builder's genes from a hash, for authored specimens and tests: the template, size and
    /// patience vary, the material follows the hash.
    pub fn from_hash(h: u64) -> Self {
        let mut rng = Rng::new(h);
        let template = Template::ALL[rng.int(0, Template::ALL.len() as u32 - 1) as usize];
        Self {
            material: [Material::Stone, Material::Ice, Material::Ore][rng.int(0, 2) as usize],
            plan: GrammarGenome::sample_template(&mut rng, template),
            scale: SCALE.0 + rng.f32() * (SCALE.1 - SCALE.0),
            blocks: rng.int(8, MAX_SITES as u32) as u8,
            patience: PATIENCE.0 + rng.f32() * (PATIENCE.1 - PATIENCE.0),
        }
        .limited()
    }

    /// The blueprint of the individual with identity `key` (a stable key such as
    /// `grammar::entity_key`): the genes' grammar grown from the individual's own salted
    /// stream, turned to a facing drawn from the same stream.
    pub fn blueprint(&self, master: u64, key: u64) -> StructurePlan {
        let g = self.limited();
        let mut rng = grammar::stream(master, Domain::Structure, key);
        let seed = rng.next_u64();
        let turn = rng.f32() * TAU;
        let plan = grammar::grow(&g.plan, seed, 1.0);
        StructurePlan::from_plan(&plan, g.scale, turn, usize::from(g.blocks))
    }

    /// The radius of the block for a site wanting `radius`.
    pub fn block_radius(radius: f32) -> f32 {
        if radius.is_finite() {
            radius.clamp(BLOCK_MIN, BLOCK_MAX)
        } else {
            BLOCK_MIN
        }
    }

    /// Seconds of gathering and placing one block: the patience gene, longer for a long stem
    /// (a stem of one plan unit takes half as long again).
    pub fn work(&self, reach: f32) -> f32 {
        let g = self.limited();
        let weight = if reach.is_finite() {
            (reach / g.scale).clamp(0.0, 1.0)
        } else {
            0.0
        };
        g.patience * (0.5 + weight)
    }
}

impl crate::genome::Genome {
    /// The authored builder: a slow, patient stone-layer that raises a coral-fan wall. Unarmed,
    /// harmless, grazing; its structure is the whole of its behavior.
    pub fn builder() -> Self {
        let mut rng = Rng::new(0xB01D_0000_0000_0001);
        Self {
            builder: Some(Builder {
                material: Material::Stone,
                plan: GrammarGenome::sample_template(&mut rng, Template::Coral),
                scale: 50.0,
                blocks: 24,
                patience: 3.0,
            }),
            radius: 18.0,
            hull: 90.0,
            mass: 50.0,
            speed: 40.0,
            cruise: 15.0,
            trigger: crate::genome::Trigger::Harm,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hashed_builder_is_valid_and_its_blueprint_is_capped() {
        for h in 0..400u64 {
            let b = Builder::from_hash(h.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            assert_eq!(b, b.limited());
            let plan = b.blueprint(1, h);
            plan.validate().unwrap();
            assert!(!plan.is_empty() && plan.len() <= usize::from(b.blocks));
        }
    }

    #[test]
    fn hostile_genes_are_forced_into_range() {
        let mut b = Builder::from_hash(9);
        b.scale = f32::NAN;
        b.patience = f32::INFINITY;
        b.blocks = 255;
        let b = b.limited();
        assert_eq!(b.scale, SCALE.0);
        assert_eq!(b.patience, PATIENCE.1);
        assert_eq!(usize::from(b.blocks), MAX_SITES);
        assert!(b.work(f32::NAN).is_finite());
    }

    #[test]
    fn blueprints_are_pure_functions_of_genes_master_and_key() {
        let b = Builder::from_hash(3);
        assert_eq!(b.blueprint(5, 77), b.blueprint(5, 77));
        assert_ne!(b.blueprint(5, 77), b.blueprint(5, 78));
        assert_ne!(b.blueprint(5, 77), b.blueprint(6, 77));
    }

    #[test]
    fn the_authored_builder_has_a_real_plan() {
        let g = crate::genome::Genome::builder();
        let plan = g.builder.unwrap().blueprint(1, 1);
        assert!(plan.len() >= 8, "{} blocks", plan.len());
    }
}
