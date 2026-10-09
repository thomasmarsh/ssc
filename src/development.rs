//! Genetic carrier identity and deterministic development of functional power organs.
//! Development adds roles to the chosen animal plan; it never substitutes a carrier body.
use crate::anatomy;
use crate::bodyplan::{self, BodyPlan, Decor, Role};
use crate::genome::Genome;
use crate::grammar::PartKind;
use crate::power::Power;
use crate::world::Rng;

const APPEARANCE_SALT: u64 = 0xA991_EA12_0000_0002;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Surface {
    #[default]
    Solid,
    Soft,
    Motes,
}

/// A heritable look, independent of which capabilities are strongest or currently active.
/// The old carrier looks are presets in this space, alongside existing body/color genes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    pub identity: Option<Power>,
    pub surface: Surface,
    /// Reach of external organs, independent of their functional power range.
    pub organ_reach: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            identity: None,
            surface: Surface::Solid,
            organ_reach: 1.0,
        }
    }
}

impl Appearance {
    pub fn carrier(power: Power) -> Self {
        Self {
            identity: Some(power),
            surface: match power {
                Power::Engulf => Surface::Soft,
                Power::Cloud => Surface::Motes,
                _ => Surface::Solid,
            },
            ..Self::default()
        }
    }
    pub fn limited(mut self) -> Self {
        self.organ_reach = if self.organ_reach.is_finite() {
            self.organ_reach.clamp(0.25, 3.0)
        } else {
            1.0
        };
        self
    }
    /// Fork the birth stream without advancing it: existing simulation draws stay fixed.
    pub(crate) fn mutate(self, rng: &Rng) -> Self {
        let mut own = Rng::new(rng.clone().next_u64() ^ APPEARANCE_SALT);
        let mut g = self;
        g.organ_reach *= 1.0 + (own.f32() + own.f32() - 1.0) * 0.08;
        if own.chance(0.01) {
            g.surface = match own.int(0, 2) {
                0 => Surface::Solid,
                1 => Surface::Soft,
                _ => Surface::Motes,
            };
        }
        g.limited()
    }
    pub(crate) fn crossover(a: Self, b: Self, rng: &Rng) -> Self {
        let mut own = Rng::new(rng.clone().next_u64() ^ APPEARANCE_SALT);
        let mut g = if own.chance(0.5) { a } else { b };
        g.organ_reach = a.organ_reach + (b.organ_reach - a.organ_reach) * own.f32();
        g.limited()
    }
}

/// One functional organ on an existing body node. Signed modes use the same organ;
/// below-gate and unbuilt powers develop none. Constraints are reported by `Power::fits`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerOrgan {
    pub power: Power,
    pub host: usize,
    pub kind: PartKind,
}

impl Genome {
    /// External functional ports follow the body genes. A limbless carrier grows one
    /// retractable port rather than being forced into a crab or a six-legged builder.
    pub fn power_ports(&self) -> usize {
        self.anatomy
            .map_or(self.limbs, |s| s.genome.limbs)
            .clamp(1, 12) as usize
    }

    /// Express the existing body grammar and then assign functional roles. This leaves
    /// node topology, joints, sizes, gait and the inherited specimen seed unchanged.
    pub fn developed_body(&self, radius: f32) -> Option<BodyPlan> {
        let spec = self.anatomy.or_else(|| anatomy::from_legacy(self))?;
        let mut plan = bodyplan::express(&spec, radius)?;
        let limbs: Vec<_> = plan
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.role == Role::Limb)
            .map(|(i, _)| i)
            .collect();
        for carried in self.live_powers().filter(|c| c.power.fits(self)) {
            let power = carried.power;
            let kind = match power {
                Power::Glare => PartKind::Eye,
                Power::Bypass | Power::Sling | Power::Weave | Power::Rune | Power::Rift => {
                    PartKind::Weapon
                }
                _ => PartKind::Organ,
            };
            let host = if matches!(power, Power::Sling | Power::Weave) {
                limbs.first().copied().unwrap_or(0)
            } else {
                0
            };
            plan.organs.push(PowerOrgan { power, host, kind });
            // Bypass needs an actual firing mount. Other ports are power actuators,
            // not extra gun hardpoints, and never change where ordinary shots fire.
            if power == Power::Bypass && !plan.nodes.iter().any(|n| n.mount) {
                plan.nodes[host].mount = true;
            }
            if plan.decor.len() >= bodyplan::MAX_DECOR {
                if let Some(i) = plan.decor.iter().rposition(|d| {
                    matches!(
                        d.kind,
                        PartKind::Fur | PartKind::Frill | PartKind::Fin | PartKind::Spine
                    )
                }) {
                    plan.decor.remove(i);
                } else {
                    continue;
                }
            }
            let angle = power as usize as f32 * std::f32::consts::TAU / Power::ALL.len() as f32;
            let r = plan.nodes[host].radius;
            plan.decor.push(Decor {
                kind,
                host,
                along: angle.cos() * r * 0.7,
                across: angle.sin() * r * 0.7,
                angle,
                length: r * 0.4 * self.appearance.organ_reach,
                radius: (r * 0.12).max(2.0),
            });
        }
        Some(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Weapon;
    use crate::power::{self, GATE};

    #[test]
    fn identity_and_topology_survive_stronger_added_or_removed_powers() {
        let mut g = Genome::slinger();
        let before = g.developed_body(g.radius).unwrap();
        power::stamp(&mut g, Power::Emp, 1.0);
        let after = g.developed_body(g.radius).unwrap();
        assert_eq!(g.appearance, Appearance::carrier(Power::Sling));
        assert_eq!(before.nodes, after.nodes);
        assert_eq!(after.organs.len(), 2);
        g.sling = 0.0;
        assert_eq!(g.appearance.identity, Some(Power::Sling));
        assert!(
            g.developed_body(g.radius)
                .unwrap()
                .organs
                .iter()
                .all(|o| o.power != Power::Sling)
        );
    }

    #[test]
    fn signed_modes_develop_organs_but_dormant_or_impossible_powers_do_not() {
        let g = Genome {
            warp: -0.8,
            song: -0.8,
            emp: GATE * 0.5,
            blink: 0.9,
            segments: 3,
            ..Genome::default()
        };
        let p = g.developed_body(g.radius).unwrap();
        assert_eq!(
            p.organs.iter().map(|o| o.power).collect::<Vec<_>>(),
            vec![Power::Warp, Power::Song]
        );
        assert!(!Power::Blink.fits(&g));
        assert_eq!(g.blink, 0.9); // Retained DNA, not silently removed or body-rewritten.
    }

    #[test]
    fn organs_supply_ports_and_firing_roles_without_forcing_the_carrier_shape() {
        let g = Genome {
            sling: 0.8,
            weave: 0.8,
            bypass: 0.8,
            weapon: Weapon::Projectile,
            ..Genome::default()
        };
        let p = g.developed_body(g.radius).unwrap();
        assert_eq!(p.nodes.len(), 1);
        assert_eq!(g.power_ports(), 1);
        assert_eq!(p.organs.len(), 3);
        assert!(p.nodes[0].mount);
        assert!(p.organs.iter().all(|o| o.host == 0));
        assert_eq!(Genome::slinger().power_ports(), 4);
        assert_eq!(Genome::weaver().power_ports(), 6);
        assert_eq!(
            crate::simulation::dev::specimen_genome("longslinger").power_ports(),
            8
        );
    }

    #[test]
    fn appearance_inherits_and_mutates_on_a_bounded_nonconsuming_stream() {
        let a = Appearance::carrier(Power::Sling);
        let b = Appearance {
            organ_reach: 2.5,
            ..Appearance::carrier(Power::Phase)
        };
        let rng = Rng::new(17);
        let next = rng.clone().next_u64();
        let mut changed = false;
        let mut both = [false; 2];
        for seed in 0..100 {
            let own = Rng::new(seed);
            let crossed = Appearance::crossover(a, b, &own);
            both[usize::from(crossed.identity == b.identity)] = true;
            assert!((1.0..=2.5).contains(&crossed.organ_reach));
            let m = a.mutate(&own);
            changed |= m != a;
            assert_eq!(m.identity, a.identity);
            assert_eq!(m, a.mutate(&own));
            assert!((0.25..=3.0).contains(&m.organ_reach));
        }
        assert!(changed && both.into_iter().all(|v| v));
        assert_eq!(rng.clone().next_u64(), next);
        assert_eq!(
            Appearance {
                organ_reach: f32::NAN,
                ..a
            }
            .limited()
            .organ_reach,
            1.0
        );
    }

    #[test]
    fn authored_surfaces_and_looks_are_genetic_presets() {
        for (g, p, surface) in [
            (Genome::slinger(), Power::Sling, Surface::Solid),
            (Genome::oozer(), Power::Engulf, Surface::Soft),
            (Genome::murmur(), Power::Cloud, Surface::Motes),
            (Genome::stormcap(), Power::Emp, Surface::Solid),
        ] {
            assert_eq!(g.appearance, Appearance::carrier(p));
            assert_eq!(g.appearance.surface, surface);
        }
        let soft = crate::simulation::dev::specimen_genome("softslinger");
        assert_eq!(soft.power_ports(), 1);
        assert_eq!(soft.appearance.surface, Surface::Soft);
        assert!(Power::Sling.active(&soft));
        assert!(!Power::Engulf.active(&soft));
    }
}
