//! The structure plan: a blueprint a builder realizes one block at a time (workstream 11).
//!
//! It is the shared shape between the L-system and every builder: `StructurePlan::from_plan`
//! turns a grammar `Plan` into world-space block sites, one per solid `Stem`, in the plan's
//! own build order (parents first), so a builder that walks the list with a cursor never
//! places a block before the block it hangs from. Pure data, no simulation and no streams;
//! a half-built structure is just `(plan, cursor)`. Design: `docs/PROCGEN.md`, "How each
//! consumer uses a Plan".

use crate::grammar::{PartKind, Plan};
use bevy::prelude::Vec2;

/// The most blocks any structure plan holds, whatever the plan or the builder asks for.
pub const MAX_SITES: usize = 48;

/// One block to place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    /// The earlier site this block hangs from (`None` for the foundation).
    pub parent: Option<u32>,
    /// Where it goes, in world units from the structure's origin.
    pub offset: Vec2,
    /// Block radius wanted by the plan (the stem radius, scaled), before the builder clamps it.
    pub radius: f32,
    /// Plan length of the stem, scaled: how far this block sits from its parent, and a measure
    /// of the work it takes.
    pub reach: f32,
}

/// A blueprint: block sites in build order. Every parent index is smaller than the site's own.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StructurePlan {
    pub sites: Vec<Site>,
}

impl StructurePlan {
    /// Sites from a grammar plan: each `Stem` becomes a block at its far end, scaled by
    /// `scale` world units per plan unit and turned by `turn` radians (0 grows up the +y axis
    /// as the plan does). Joints, leaves, fruit and marks are skipped, their stems' parents
    /// following the nearest stem above them. The result is cut to `limit` (at most
    /// `MAX_SITES`) sites; a prefix of a build order is still a build order.
    pub fn from_plan(plan: &Plan, scale: f32, turn: f32, limit: usize) -> Self {
        let scale = if scale.is_finite() {
            scale.max(0.0)
        } else {
            0.0
        };
        let turn = if turn.is_finite() { turn } else { 0.0 };
        let limit = limit.min(MAX_SITES);
        // Site index of each part that became one.
        let mut site_of: Vec<Option<u32>> = vec![None; plan.parts.len()];
        let mut sites: Vec<Site> = Vec::new();
        for (i, part) in plan.parts.iter().enumerate() {
            if sites.len() >= limit {
                break;
            }
            // The stem above this part, skipping parts that are not blocks.
            let mut up = part.parent;
            while let Some(p) = up {
                if site_of[p as usize].is_some() {
                    break;
                }
                up = plan.parts[p as usize].parent;
            }
            if part.kind != PartKind::Stem {
                site_of[i] = None;
                continue;
            }
            site_of[i] = Some(sites.len() as u32);
            sites.push(Site {
                parent: up.and_then(|p| site_of[p as usize]),
                offset: Vec2::from_angle(turn).rotate(part.end()) * scale,
                radius: part.radius * scale,
                reach: part.length * scale,
            });
        }
        Self { sites }
    }

    pub fn len(&self) -> usize {
        self.sites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// The farthest a site lies from the origin, for placing a structure clear of others.
    pub fn extent(&self) -> f32 {
        self.sites
            .iter()
            .map(|s| s.offset.length() + s.radius)
            .fold(0.0, f32::max)
    }

    /// Every promise `from_plan` makes: within the cap, finite, parents strictly earlier.
    pub fn validate(&self) -> Result<(), String> {
        if self.sites.len() > MAX_SITES {
            return Err(format!("{} sites exceeds the cap", self.sites.len()));
        }
        for (i, s) in self.sites.iter().enumerate() {
            if !(s.offset.is_finite() && s.radius.is_finite() && s.reach.is_finite())
                || s.radius < 0.0
                || s.reach < 0.0
            {
                return Err(format!("site {i} is not finite and non-negative"));
            }
            if s.parent.is_some_and(|p| p as usize >= i) {
                return Err(format!("site {i} has a parent that is not earlier"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::{Domain, GrammarSpecimen, Template};

    fn specimen(key: u64) -> GrammarSpecimen {
        GrammarSpecimen::for_entity(7, Domain::Builder, key)
    }

    #[test]
    fn a_plan_becomes_sites_in_build_order_with_parents_first() {
        for key in 0..300 {
            let plan = specimen(key).plan(1.0);
            let built = StructurePlan::from_plan(&plan, 50.0, 0.0, MAX_SITES);
            built.validate().unwrap();
            let stems = plan
                .parts
                .iter()
                .filter(|p| p.kind == PartKind::Stem)
                .count();
            assert_eq!(built.len(), stems.min(MAX_SITES), "key {key}");
            // The first site is the foundation, and the others hang from something earlier.
            assert_eq!(built.sites.first().and_then(|s| s.parent), None);
            assert!(built.sites.iter().skip(1).all(|s| s.parent.is_some()));
        }
    }

    #[test]
    fn site_positions_are_the_scaled_stem_ends_and_turn_with_the_plan() {
        let plan = specimen(3).plan(1.0);
        let flat = StructurePlan::from_plan(&plan, 40.0, 0.0, MAX_SITES);
        let first = plan
            .parts
            .iter()
            .find(|p| p.kind == PartKind::Stem)
            .unwrap();
        assert!((flat.sites[0].offset - first.end() * 40.0).length() < 1e-3);
        let turned = StructurePlan::from_plan(&plan, 40.0, 1.2, MAX_SITES);
        for (a, b) in flat.sites.iter().zip(&turned.sites) {
            assert!((a.offset.length() - b.offset.length()).abs() < 1e-2);
        }
        assert!((flat.extent() - turned.extent()).abs() < 1.0);
    }

    #[test]
    fn a_prefix_of_the_plan_is_a_prefix_of_the_sites() {
        let plan = specimen(11).plan(1.0);
        let full = StructurePlan::from_plan(&plan, 50.0, 0.3, MAX_SITES);
        for limit in [1, 5, 12] {
            let cut = StructurePlan::from_plan(&plan, 50.0, 0.3, limit);
            assert_eq!(cut.len(), limit.min(full.len()));
            assert_eq!(cut.sites[..], full.sites[..cut.len()]);
        }
    }

    #[test]
    fn grown_plans_prefix_the_sites_of_their_finished_plan() {
        // Plan prefix stability carries over: a saved partial build keeps its block indices.
        let spec = specimen(21);
        let small = StructurePlan::from_plan(&spec.plan(0.5), 50.0, 0.0, MAX_SITES);
        let full = StructurePlan::from_plan(&spec.plan(1.0), 50.0, 0.0, MAX_SITES);
        assert!(small.len() <= full.len());
        for (a, b) in small.sites.iter().zip(&full.sites) {
            assert_eq!(a.parent, b.parent);
        }
    }

    #[test]
    fn the_plans_are_capped_and_survive_hostile_input() {
        let mut greedy = specimen(1);
        greedy.genome.template = Template::Coral;
        greedy.genome.depth = 8;
        greedy.genome.branch_rate = 1.0;
        let plan = greedy.plan(1.0);
        assert!(plan.parts.len() > MAX_SITES, "the cap must bind");
        let built = StructurePlan::from_plan(&plan, 50.0, 0.0, 1000);
        assert_eq!(built.len(), MAX_SITES);
        built.validate().unwrap();
        for (scale, turn) in [(f32::NAN, 0.0), (f32::INFINITY, f32::NAN), (-5.0, 1.0)] {
            let s = StructurePlan::from_plan(&plan, scale, turn, MAX_SITES);
            s.validate().unwrap();
        }
        assert!(StructurePlan::from_plan(&Plan::default(), 50.0, 0.0, 10).is_empty());
    }

    #[test]
    fn it_is_deterministic() {
        let a = StructurePlan::from_plan(&specimen(5).plan(1.0), 50.0, 0.7, 30);
        let b = StructurePlan::from_plan(&specimen(5).plan(1.0), 50.0, 0.7, 30);
        assert_eq!(a, b);
    }
}
