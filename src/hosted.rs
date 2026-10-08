//! Creatures inside creatures (workstream 7): an optional genome section that gives a host
//! slots for residents. A host genome carries a count and a relationship; its residents are
//! plain creatures derived from it by `resident`, spawned attached (the same mechanic as
//! rooted life, see `simulation/attach.rs` and `simulation/root.rs`) and released when the
//! host dies or unloads (a brood then swarms: see `simulation/root.rs`).
//!
//! Slice one: one megafauna host (the apex elder: rare, big and already deterministic) and
//! residents that ride its head. The relationship shapes the resident's genome (a brood is
//! the host's own young, a symbiote an unarmed rider, a parasite a stinging one); the
//! behaviors that differ by relationship (cleaning, draining, swarming on release) are
//! still to come (`TODO:` in `docs/WORKSTREAMS.md`, workstream 7).
//!
//! Nothing here learns, and nothing is an enum branch of an enemy kind: the relationship is
//! a gene value read by one function.

use crate::genome::{Diet, Fecundity, Genome, Social, Trigger, Weapon};
use crate::world::hash2;
use std::f32::consts::TAU;

/// Separates the resident streams from every other one.
pub const HOSTED_SALT: u64 = 0x4057_ED00_0000_0007;
/// The most residents any one host may carry, whatever its genome says. A hard cap beside the
/// sector's body budget (`world::SECTOR_BODY_BUDGET`), which residents also count against.
pub const MAX_RESIDENTS: u8 = 6;
/// Radius of a resident, in world units (brood excepted: it is a small copy of its host).
pub const RESIDENT_RADIUS: f32 = 10.0;

/// How a resident relates to its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    /// Rides along unarmed, and is a friend of the host's.
    Symbiote,
    /// Stings, a hanger-on at the host's cost.
    Parasite,
    /// The host's own young.
    Brood,
}

impl Relation {
    pub const ALL: [Relation; 3] = [Relation::Symbiote, Relation::Parasite, Relation::Brood];
}

/// Host slots: how many residents a creature carries and what they are to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hosted {
    pub count: u8,
    pub relation: Relation,
}

impl Hosted {
    /// The same section forced into range.
    pub fn limited(self) -> Self {
        Self {
            count: self.count.min(MAX_RESIDENTS),
            ..self
        }
    }

    /// A host's slots from a hash: two to four residents, relationship picked evenly.
    pub fn from_hash(h: u64) -> Self {
        Self {
            count: 2 + ((h >> 8) % 3) as u8,
            relation: Relation::ALL[((h >> 24) % 3) as usize],
        }
        .limited()
    }

    /// How many residents of `RESIDENT_RADIUS` really fit around a host of `host_radius`
    /// (half its rim, so they never crowd), never above the genome's count or the cap.
    pub fn fitting(&self, host_radius: f32) -> u8 {
        let fit = crate::simulation::attach::capacity(host_radius, RESIDENT_RADIUS, 0.5);
        self.limited().count.min(fit.min(255) as u8)
    }

    /// The anchor (host frame) of resident `k` of `n`: spread evenly around the rim, rotated
    /// by a hash of the host so hosts differ.
    pub fn anchor(host_key: u64, k: u8, n: u8) -> f32 {
        let turn = (hash2(host_key ^ HOSTED_SALT, 1, 2) % 1024) as f32 / 1024.0 * TAU;
        turn + f32::from(k) * TAU / f32::from(n.max(1))
    }
}

/// The resident of a host with genome `host`, a pure function of the host's genes. Small,
/// single-bodied, without a brain or an anatomy, in the host's colours (a symbiote's hue is
/// turned toward green so it reads as a friend).
pub fn resident(host: &Genome, relation: Relation) -> Genome {
    let base = Genome {
        segments: 1,
        limbs: 0,
        sides: 3,
        radius: RESIDENT_RADIUS,
        mass: 4.0,
        hull: 40.0,
        shield: 0.0,
        speed: 260.0,
        cruise: 80.0,
        sight: 1200.0,
        lose: 1800.0,
        social: Social::Pack,
        trigger: Trigger::Sight,
        diet: Diet::None,
        fecundity: Fecundity::Rare,
        hue: host.hue,
        pale: host.pale,
        bright: host.bright,
        ..Genome::default()
    };
    match relation {
        Relation::Brood => Genome {
            radius: RESIDENT_RADIUS + 2.0,
            weapon: Weapon::Projectile,
            volley: 1,
            fire_period: 2.6,
            shot_speed: 340.0,
            weapon_range: 600.0,
            contact_damage: 6.0,
            bounty: 30.0,
            ..base
        },
        Relation::Parasite => Genome {
            contact_damage: 9.0,
            bounty: 40.0,
            ..base
        },
        Relation::Symbiote => Genome {
            contact_damage: 0.0,
            bounty: 0.0,
            hue: (host.hue + 0.33).rem_euclid(1.0),
            ..base
        },
    }
    .limited()
}

/// Whether `genome` is a brood resident (see `resident`): the host's armed young, the one
/// relation that swarms when its host is lost. Read from the genes so a released resident,
/// which no longer knows its host, still behaves as what it is.
pub fn is_brood(genome: &Genome) -> bool {
    genome.hosted.is_none()
        && genome.anatomy.is_none()
        && genome.weapon == Weapon::Projectile
        && genome.radius == RESIDENT_RADIUS + 2.0
        && genome.social == Social::Pack
        && genome.diet == Diet::None
}

/// The unarmed resident shape shared by symbiotes and parasites (see `resident`).
fn is_rider(genome: &Genome) -> bool {
    genome.hosted.is_none()
        && genome.anatomy.is_none()
        && genome.weapon != Weapon::Projectile
        && genome.parts() == 1
        && genome.radius == RESIDENT_RADIUS
        && genome.mass == 4.0
        && genome.hull == 40.0
        && genome.social == Social::Pack
        && genome.diet == Diet::None
}

/// A symbiote resident: harmless, and it tends its host (`root::tend_hosts`).
pub fn is_symbiote(genome: &Genome) -> bool {
    is_rider(genome) && genome.contact_damage == 0.0
}

/// A parasite resident: it stings, and it drains its host (`root::tend_hosts`).
pub fn is_parasite(genome: &Genome) -> bool {
    is_rider(genome) && genome.contact_damage > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residents_are_valid_small_single_bodies_in_every_relation() {
        let host = Genome::default();
        for relation in Relation::ALL {
            let g = resident(&host, relation);
            assert_eq!(g, g.limited());
            assert_eq!(g.parts(), 1);
            assert!(g.anatomy.is_none() && g.hosted.is_none());
            assert_eq!(g.learner, 0.0);
            assert!(g.radius < 20.0);
        }
        assert_eq!(resident(&host, Relation::Symbiote).contact_damage, 0.0);
        for relation in Relation::ALL {
            let g = resident(&host, relation);
            assert_eq!(is_brood(&g), relation == Relation::Brood);
            assert_eq!(is_symbiote(&g), relation == Relation::Symbiote);
            assert_eq!(is_parasite(&g), relation == Relation::Parasite);
        }
    }

    #[test]
    fn slots_are_capped_and_fit_the_rim() {
        let wild = Hosted {
            count: 200,
            relation: Relation::Brood,
        };
        assert_eq!(wild.limited().count, MAX_RESIDENTS);
        assert!(wild.fitting(500.0) <= MAX_RESIDENTS);
        assert_eq!(wild.fitting(5.0), 1);
        for h in 0..200u64 {
            let hosted = Hosted::from_hash(hash2(h, 3, 4));
            assert!((2..=4).contains(&hosted.count));
        }
    }
}
