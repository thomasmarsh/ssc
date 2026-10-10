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

// Independent forks keep both the primary lottery and all caller draw positions fixed.
const CARRIER_SALT: u64 = 0xCA22_1E25_0000_0003;
const MODULE_SALT: u64 = 0xADDE_D902_0000_0003;
pub const CARRIER_VARIANT_CHANCE: f32 = 0.30;
// Share of all powered founders, included within the 30-percent variation attempts.
pub const UNUSUAL_CARRIER_CHANCE: f32 = 0.05;
pub const MULTI_POWER_CHANCE: f32 = 0.02;
pub const EXTRA_POWER_CONTINUE_CHANCE: f32 = 0.10;
pub const MAX_SAMPLED_POWERS: usize = 3;
pub const MAX_SAMPLED_CARRIER_BODIES: u32 = 16;

/// Founder species retain their familiar identity, with an independently sampled carrier
/// and a small compatible capability tail. Authored genomes and awakening never call this.
pub(crate) fn diversify(g: &mut Genome, source: &Rng, params: &crate::world::SectorParams) {
    let Some(primary) = g.live_power().map(|c| c.power) else {
        return;
    };
    let key = source.clone().next_u64();
    let mut body = Rng::new(key ^ CARRIER_SALT);
    let roll = body.f32();
    if roll < CARRIER_VARIANT_CHANCE {
        let mut candidate = *g;
        candidate.aspect *= body.range(0.75, 1.35);
        candidate.radius *= body.range(0.8, 1.2);
        candidate.appearance.organ_reach = body.range(0.6, 1.8);
        candidate.hue = (candidate.hue + body.range(-0.08, 0.08)).rem_euclid(1.0);
        if candidate.limbs > 0 {
            candidate.limbs = body.int(2, 6) as u8;
            candidate.limb_len = body.int(1, 2) as u8;
        }
        if roll < UNUSUAL_CARRIER_CHANCE {
            candidate.appearance.surface = match body.int(0, 2) {
                0 => Surface::Solid,
                1 => Surface::Soft,
                _ => Surface::Motes,
            };
            // Existing weighted animal grammar supplies a full range of independent
            // body plans. One bounded attempt: no rejection loop or hidden draw coupling.
            let mut animal = anatomy::AnimalGenome::sample(&mut body);
            animal.depth = 0;
            candidate.anatomy = Some(anatomy::AnimalSpecimen {
                genome: animal.limited(),
                seed: body.next_u64(),
            });
        }
        candidate = candidate.limited();
        if primary.fits(&candidate) && candidate.parts() <= MAX_SAMPLED_CARRIER_BODIES {
            *g = candidate;
        }
    }
    let mut modules = Rng::new(key ^ MODULE_SALT);
    if !modules.chance(MULTI_POWER_CHANCE) {
        return;
    }
    for _ in 1..MAX_SAMPLED_POWERS {
        let weights = crate::power::weights(params);
        let eligible: Vec<_> = Power::ALL
            .into_iter()
            .zip(weights)
            .filter(|(p, w)| *w > 0.0 && !p.active(g) && p.fits(g))
            .collect();
        let total: f32 = eligible.iter().map(|(_, w)| w).sum();
        if total <= 0.0 {
            break;
        }
        let mut pick = modules.f32() * total;
        for (power, weight) in eligible {
            if pick < weight {
                // Express only the module: styling here would erase the original carrier.
                crate::power::express(g, power, modules.f32(), crate::power::species_intensity());
                break;
            }
            pick -= weight;
        }
        if !modules.chance(EXTRA_POWER_CONTINUE_CHANCE) {
            break;
        }
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
    fn founder_diversity_is_deterministic_bounded_and_keeps_identity_and_draws() {
        let params = crate::world::SectorParams {
            depth: 30.0,
            danger: 0.5,
            aggression: 0.5,
            density: 0.5,
            distortion: 0.5,
            tech: 0.5,
            swarm: 0.5,
        };
        let mut variants = 0;
        let mut unusual = 0;
        let mut multi = 0;
        let mut triples = 0;
        let mut ports = std::collections::HashSet::new();
        for seed in 0..4000 {
            let source = Rng::new(seed);
            let base = Genome::slinger();
            let mut g = base;
            diversify(&mut g, &source, &params);
            let mut repeat = base;
            diversify(&mut repeat, &source, &params);
            assert_eq!(g, repeat);
            assert_eq!(source.clone().next_u64(), Rng::new(seed).next_u64());
            assert_eq!(g.appearance.identity, base.appearance.identity);
            assert_eq!(
                g.power_module(Power::Sling),
                base.power_module(Power::Sling)
            );
            assert_eq!(g.weapon, base.weapon);
            assert_eq!(g, g.limited());
            assert!(g.parts() <= MAX_SAMPLED_CARRIER_BODIES);
            assert!(g.live_powers().all(|c| c.power.fits(&g)));
            let count = g.live_powers().count();
            assert!((1..=MAX_SAMPLED_POWERS).contains(&count));
            if g != base && count == 1 {
                variants += 1;
            }
            unusual += usize::from(g.anatomy.is_some());
            multi += usize::from(count > 1);
            triples += usize::from(count == 3);
            ports.insert(g.power_ports());
            if seed % 100 == 0 {
                let child = Genome::crossover(g, base, &mut Rng::new(seed + 8));
                assert_eq!(child, child.limited());
                assert!(child.live_powers().any(|c| c.power == Power::Sling));
            }
        }
        assert!((700..1400).contains(&variants), "variants {variants}");
        assert!((40..220).contains(&unusual), "anatomy {unusual}");
        assert!((40..130).contains(&multi), "multi {multi}");
        assert!((1..20).contains(&triples), "triples {triples}");
        assert!(ports.len() >= 5);
        eprintln!(
            "4000 Sling founders: variants={variants}, anatomy={unusual}, multi={multi}, triples={triples}, ports={ports:?}"
        );
    }

    #[test]
    fn sampled_founder_census_respects_depth_and_primary_carrier() {
        let params = crate::world::SectorParams {
            depth: 30.0,
            danger: 0.5,
            aggression: 0.5,
            density: 0.5,
            distortion: 0.5,
            tech: 0.5,
            swarm: 0.5,
        };
        let mut carriers = 0;
        let mut multi = 0;
        let mut triples = 0;
        let mut anatomy = 0;
        let mut examples = [false; 3];
        for seed in 0..20000 {
            let g = Genome::sample(&mut Rng::new(seed), &params);
            let count = g.live_powers().count();
            if count == 0 {
                continue;
            }
            carriers += 1;
            for (slot, wanted) in [
                g.anatomy.is_some(),
                count > 1,
                g.sling > GATE && g.power_ports() != 4,
            ]
            .into_iter()
            .enumerate()
            {
                if wanted && !examples[slot] {
                    examples[slot] = true;
                    eprintln!(
                        "example {slot}: seed={seed}, identity={:?}, powers={:?}, parts={}, ports={}",
                        g.appearance.identity,
                        g.live_powers().map(|c| c.power).collect::<Vec<_>>(),
                        g.parts(),
                        g.power_ports()
                    );
                }
            }
            multi += usize::from(count > 1);
            triples += usize::from(count == 3);
            anatomy += usize::from(g.anatomy.is_some());
            assert!(count <= MAX_SAMPLED_POWERS);
            assert!(g.live_powers().all(|c| c.power.fits(&g)));
            assert!(g.appearance.identity.is_some());
        }
        assert!(carriers > 1000 && carriers < 2200, "carriers {carriers}");
        assert!(multi > 10 && multi < carriers / 20, "multi {multi}");
        assert!(triples > 0 && anatomy > 10);
        for depth in [0.0, 1.0, 2.0] {
            for seed in 0..100 {
                let p = crate::world::SectorParams { depth, ..params };
                assert_eq!(
                    Genome::sample(&mut Rng::new(seed), &p)
                        .live_powers()
                        .count(),
                    0
                );
            }
        }
        eprintln!(
            "20000 far founders: carriers={carriers}, multi={multi}, triples={triples}, anatomy={anatomy}"
        );
    }

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
