//! Sonar: the ship sends a ring out and, as it passes, the nearest planetoids, civilization
//! seats, fortresses and the player's own landing pads answer with echoes that linger and
//! fade. The answers come from the world generator, a pure function of the seed and sector
//! coordinates, so they reach sectors far beyond the simulated region without loading them.
//! Echoes are information only: nothing in the rules reads them, and a ping costs nothing but
//! its cooldown.
//!
//! The base ping is what the ship starts with. Bench upgrades (`skills`) stretch its reach,
//! speed it up, shorten its cooldown and widen its answer, and four reveal tiers, each bought
//! once, add echo kinds: pads the enemy has found, rich lodes and renewable planetoids, nests
//! and egg clusters, and the predator density of each sector.

use super::mining::{Material, material_of, ore_for, renewable};
use super::skills::Skill;
use super::tuning as t;
use super::{BodyKind, Cue, Game};
use crate::genome::Niche;
use crate::territory::{CivRole, Standing};
use crate::world::{self, RockKind, SECTOR_SIZE, SectorId, Spawn};
use bevy::prelude::Vec2;
use std::collections::HashMap;

/// Seconds before the ship may ping again (base; upgrades shorten it).
pub const PING_COOLDOWN: f32 = 5.0;
/// How far the ring travels, in units, and how fast (base).
pub const PING_RANGE: f32 = 20_000.0;
pub const RING_SPEED: f32 = 7_000.0;
/// Seconds an echo lasts once it has sounded.
pub const ECHO_LIFE: f32 = 9.0;
/// Most echoes of each base kind.
const CAP_PLANETOID: usize = 3;
const CAP_CIVILIZATION: usize = 2;
const CAP_FORTRESS: usize = 2;
const CAP_PAD: usize = 2;
/// A nest's stones lie within this of its heart; dwellers are counted inside it.
const NEST_REACH: f32 = 180.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EchoKind {
    Planetoid,
    /// An outpost of a civilization.
    Civilization,
    /// A civilization's capital, walled.
    Fortress,
    /// One of the player's own landing pads.
    Pad,
    /// One of the player's pads that the enemy has found (pad watch).
    PadAlert,
    /// A rich mining spot: a renewable planetoid or a rock heavy with ore (lode echo).
    Lode,
    /// A ring of stones sheltering creatures (nest echo).
    Nest,
    /// An inhabited husk, a cluster of eggs waiting to hatch (nest echo).
    Eggs,
    /// How many hostile creatures roam a sector (predator echo); `weight` is the count.
    Predators,
}

impl EchoKind {
    fn cap(self) -> usize {
        match self {
            Self::Planetoid => CAP_PLANETOID,
            Self::Civilization => CAP_CIVILIZATION,
            Self::Fortress => CAP_FORTRESS,
            Self::Pad => CAP_PAD,
            Self::PadAlert => t::CAP_PAD_ALERT,
            Self::Lode => t::CAP_LODE,
            Self::Nest => t::CAP_NEST,
            Self::Eggs => t::CAP_EGGS,
            Self::Predators => t::CAP_PREDATORS,
        }
    }

    /// The sonar upgrade that reveals this kind, if it is not part of the base ping.
    pub fn unlocked_by(self) -> Option<Skill> {
        match self {
            Self::PadAlert => Some(Skill::EchoPads),
            Self::Lode => Some(Skill::EchoLodes),
            Self::Nest | Self::Eggs => Some(Skill::EchoNests),
            Self::Predators => Some(Skill::EchoPredators),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Echo {
    pub kind: EchoKind,
    pub position: Vec2,
    /// Size of the thing (a planetoid's radius), for drawing its marker.
    pub radius: f32,
    /// A civilization's tint, or a lode's material color.
    pub tint: Option<[f32; 3]>,
    /// How much there is: predators in a sector, creatures in a nest, eggs in a husk, ore in
    /// a lode. Zero when it does not apply.
    pub weight: f32,
    /// Game time at which the ring reaches it and it sounds.
    pub born: f32,
    sounded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub origin: Vec2,
    pub started: f32,
    /// How far this ring travels and how fast, as upgraded when it was sent.
    pub range: f32,
    pub speed: f32,
}

/// A generated site that could answer a ping, cached per sector (generation is pure).
#[derive(Clone, Debug)]
struct Site {
    kind: EchoKind,
    position: Vec2,
    radius: f32,
    tint: Option<[f32; 3]>,
    weight: f32,
    /// Spawn indices that make up the site; it is gone when all of them are destroyed.
    members: Vec<u32>,
    territory: Option<u64>,
    /// A planetoid that regrows: it answers as a lode once that tier is owned.
    renewable: bool,
}

#[derive(Default)]
pub struct PingState {
    pub(super) cooldown: f32,
    pub(super) ring: Option<Ring>,
    pub(super) echoes: Vec<Echo>,
    cache: HashMap<SectorId, Vec<Site>>,
}

fn is_stone(spawn: &Spawn) -> bool {
    spawn.kind == BodyKind::Asteroid
        && spawn.pinned
        && spawn.rock == RockKind::Plain
        && spawn.fort.is_none()
}

fn material_tint(material: Material) -> Option<[f32; 3]> {
    Some(material.color())
}

fn sites_of(seed: u64, id: SectorId) -> Vec<Site> {
    let territory = world::territory(seed, id);
    let tint = territory.map(|t| t.color(seed));
    let spawns = world::generate(seed, id);
    let mut sites: Vec<Site> = Vec::new();
    let single = |kind, spawn: &Spawn, tint, territory, weight: f32, renewable| Site {
        kind,
        position: spawn.position,
        radius: spawn.radius.unwrap_or(0.0),
        tint,
        weight,
        members: vec![spawn.index],
        territory,
        renewable,
    };
    for spawn in &spawns {
        match (spawn.kind, spawn.rock, spawn.civ) {
            (BodyKind::Asteroid, RockKind::Planetoid, _) => {
                let key = (id, spawn.index);
                let mut site = single(EchoKind::Planetoid, spawn, None, None, 0.0, false);
                site.renewable = renewable(seed, key);
                if site.renewable {
                    site.tint = material_tint(material_of(seed, RockKind::Planetoid, Some(key)));
                }
                sites.push(site);
            }
            (BodyKind::Base, _, Some(tag)) if spawn.fort.is_none() => {
                let kind = match tag.role {
                    CivRole::Capital => EchoKind::Fortress,
                    CivRole::Outpost => EchoKind::Civilization,
                    _ => continue,
                };
                sites.push(single(kind, spawn, tint, Some(tag.territory), 0.0, false));
            }
            (BodyKind::Asteroid, rock @ (RockKind::Ore | RockKind::Crystal | RockKind::Ice), _)
                if !spawn.pinned =>
            {
                let ore = ore_for(rock, spawn.radius.unwrap_or(0.0));
                if ore >= t::LODE_MIN_ORE {
                    let material = material_of(seed, rock, Some((id, spawn.index)));
                    sites.push(single(
                        EchoKind::Lode,
                        spawn,
                        material_tint(material),
                        None,
                        ore,
                        false,
                    ));
                }
            }
            (BodyKind::Asteroid, RockKind::Husk, _) if spawn.den.is_some() => {
                let eggs = spawn.den.map_or(0.0, |(_, n)| f32::from(n));
                sites.push(single(EchoKind::Eggs, spawn, None, None, eggs, false));
            }
            _ => {}
        }
    }
    // Nests: runs of consecutive pinned stones (a nest pushes its ring in one go).
    let mut at = 0;
    while at < spawns.len() {
        if !is_stone(&spawns[at]) {
            at += 1;
            continue;
        }
        let end = (at..spawns.len())
            .find(|&i| !is_stone(&spawns[i]))
            .unwrap_or(spawns.len());
        let ring = &spawns[at..end];
        let heart = ring.iter().map(|s| s.position).sum::<Vec2>() / ring.len() as f32;
        let dwellers = spawns
            .iter()
            .filter(|s| s.kind == BodyKind::Creature && s.position.distance(heart) < NEST_REACH)
            .count();
        sites.push(Site {
            kind: EchoKind::Nest,
            position: heart,
            radius: 0.0,
            tint: None,
            weight: dwellers as f32,
            members: ring.iter().map(|s| s.index).collect(),
            territory: None,
            renewable: false,
        });
        at = end;
    }
    // Predators: the wild hostile creatures of the sector, read as one density.
    let hostile: Vec<&Spawn> = spawns
        .iter()
        .filter(|s| {
            s.kind == BodyKind::Creature
                && s.civ.is_none()
                && s.rooted.is_none()
                && s.species
                    .is_some_and(|sp| sp.genome.niche() != Niche::School)
        })
        .collect();
    if !hostile.is_empty() {
        let heart = hostile.iter().map(|s| s.position).sum::<Vec2>() / hostile.len() as f32;
        sites.push(Site {
            kind: EchoKind::Predators,
            position: heart,
            radius: 0.0,
            tint: None,
            weight: hostile.len() as f32,
            members: hostile.iter().map(|s| s.index).collect(),
            territory: None,
            renewable: false,
        });
    }
    sites
}

impl Game {
    /// Seconds a ping takes to recharge with the rig as it is.
    pub fn ping_recharge(&self) -> f32 {
        self.loadout.skills.ping_cooldown(PING_COOLDOWN)
    }

    /// Sends a ping from the ship. Refused (false) while recharging, dead or over.
    pub fn ping(&mut self) -> bool {
        if self.game_over || self.ping.cooldown > 0.0 {
            return false;
        }
        let Some(origin) = self.player().map(|p| p.position) else {
            return false;
        };
        let skills = self.loadout.skills;
        let range = skills.ping_range(PING_RANGE);
        let speed = skills.ping_speed(RING_SPEED);
        let owns = |kind: EchoKind| kind.unlocked_by().is_none_or(|s| skills.level(s) > 0);
        self.ping.cooldown = skills.ping_cooldown(PING_COOLDOWN);
        self.ping.ring = Some(Ring {
            origin,
            started: self.time,
            range,
            speed,
        });
        let mut found: Vec<Echo> = Vec::new();
        let home = SectorId::containing(origin);
        let search = (range / SECTOR_SIZE) as i32;
        for dx in -search..=search {
            for dy in -search..=search {
                let id = SectorId {
                    x: home.x + dx,
                    y: home.y + dy,
                };
                let seed = self.seed;
                let sites = self
                    .ping
                    .cache
                    .entry(id)
                    .or_insert_with(|| sites_of(seed, id))
                    .clone();
                let fallen = self.fallen.get(&id);
                for site in sites.iter() {
                    let destroyed = |i: &u32| fallen.is_some_and(|f| f.contains(i));
                    let gone = site.members.iter().all(destroyed)
                        || site
                            .territory
                            .is_some_and(|t| self.civ_standing(t) == Standing::Fallen);
                    let distance = site.position.distance(origin);
                    if gone || distance > range {
                        continue;
                    }
                    let kind = if site.renewable && owns(EchoKind::Lode) {
                        EchoKind::Lode
                    } else {
                        site.kind
                    };
                    if !owns(kind) {
                        continue;
                    }
                    let weight = if kind == EchoKind::Predators {
                        site.members.iter().filter(|i| !destroyed(i)).count() as f32
                    } else {
                        site.weight
                    };
                    found.push(Echo {
                        kind,
                        position: site.position,
                        radius: site.radius,
                        tint: site.tint,
                        weight,
                        born: self.time + distance / speed,
                        sounded: false,
                    });
                }
            }
        }
        for pad in self.pads() {
            let position = self.pad_position(pad);
            let distance = position.distance(origin);
            if distance <= range {
                let alert = owns(EchoKind::PadAlert) && self.pad_exposed(pad.key);
                found.push(Echo {
                    kind: if alert {
                        EchoKind::PadAlert
                    } else {
                        EchoKind::Pad
                    },
                    position,
                    radius: 0.0,
                    tint: None,
                    weight: 0.0,
                    born: self.time + distance / speed,
                    sounded: false,
                });
            }
        }
        found.sort_by(|a, b| a.born.total_cmp(&b.born));
        let extra = skills.ping_extra_targets();
        let mut taken: HashMap<EchoKind, usize> = HashMap::new();
        found.retain(|echo| {
            let n = taken.entry(echo.kind).or_default();
            *n += 1;
            *n <= echo.kind.cap() + extra
        });
        self.ping.echoes = found;
        self.cue(Cue::Ping);
        true
    }

    pub(super) fn update_ping(&mut self, dt: f32) {
        self.ping.cooldown = (self.ping.cooldown - dt).max(0.0);
        let time = self.time;
        if self
            .ping
            .ring
            .is_some_and(|r| (time - r.started) * r.speed > r.range)
        {
            self.ping.ring = None;
        }
        let mut sounded = Vec::new();
        for echo in &mut self.ping.echoes {
            if !echo.sounded && time >= echo.born {
                echo.sounded = true;
                sounded.push(echo.position);
            }
        }
        self.ping.echoes.retain(|e| time < e.born + ECHO_LIFE);
        for at in sounded {
            self.cue(Cue::Echo { at });
        }
    }

    /// Seconds until the next ping is allowed.
    pub fn ping_cooldown(&self) -> f32 {
        self.ping.cooldown
    }

    /// The expanding ring, as (origin, current radius), while it travels.
    pub fn ping_ring(&self) -> Option<(Vec2, f32)> {
        self.ping
            .ring
            .map(|r| (r.origin, ((self.time - r.started) * r.speed).max(0.0)))
    }

    /// How far the ring in flight travels, for fading it (the base range without one).
    pub fn ping_ring_range(&self) -> f32 {
        self.ping.ring.map_or(PING_RANGE, |r| r.range)
    }

    /// Echoes that have sounded, with their remaining brightness in (0, 1].
    pub fn echoes(&self) -> impl Iterator<Item = (&Echo, f32)> {
        let time = self.time;
        self.ping
            .echoes
            .iter()
            .filter(move |e| time >= e.born)
            .map(move |e| (e, (1.0 - (time - e.born) / ECHO_LIFE).clamp(0.0, 1.0)))
    }
}

/// Planetoids a ping would find, for tests of the far-reach claim.
#[cfg(test)]
pub(super) fn site_count(seed: u64, id: SectorId) -> usize {
    sites_of(seed, id).len()
}

#[cfg(test)]
mod tests {
    use super::super::tests::{DT, empty_game};
    use super::super::{Bearing, GuideKind, Input};
    use super::t;
    use super::*;

    fn run(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            game.step(DT, Input::default());
        }
    }

    #[test]
    fn a_ping_has_a_cooldown() {
        let mut game = empty_game();
        assert!(game.ping());
        assert!(!game.ping());
        run(&mut game, PING_COOLDOWN + 0.2);
        assert!(game.ping());
    }

    #[test]
    fn echoes_arrive_when_the_ring_does_and_reach_far_beyond_the_simulated_region() {
        let mut game = empty_game();
        assert!(game.ping());
        let all: Vec<Echo> = game.ping.echoes.clone();
        assert!(!all.is_empty(), "something lies within three sectors");
        assert!(
            all.iter().any(|e| e.position.length() > 4000.0),
            "an echo from outside the simulated region"
        );
        assert_eq!(game.echoes().count(), 0, "nothing sounds at once");
        for echo in &all {
            let delay = echo.born - game.time;
            let distance = echo.position.length();
            assert!((delay - distance / RING_SPEED).abs() < 1e-3);
        }
        run(&mut game, 1.0);
        let heard = game.echoes().count();
        let due = all.iter().filter(|e| e.born <= game.time).count();
        assert_eq!(heard, due);
        run(&mut game, 3.0);
        assert_eq!(game.echoes().count(), all.len());
        run(&mut game, ECHO_LIFE + 1.0);
        assert_eq!(game.echoes().count(), 0, "echoes fade away");
        assert!(game.ping_ring().is_none());
    }

    #[test]
    fn echoes_cue_sound_once_each_and_the_ping_itself_sounds() {
        let mut game = empty_game();
        game.drain_cues();
        game.ping();
        let n = game.ping.echoes.len();
        run(&mut game, 4.0);
        let cues = game.drain_cues();
        assert_eq!(cues.iter().filter(|c| **c == Cue::Ping).count(), 1);
        assert_eq!(
            cues.iter()
                .filter(|c| matches!(c, Cue::Echo { .. }))
                .count(),
            n
        );
    }

    #[test]
    fn the_answer_is_capped_per_kind_and_deterministic() {
        let (mut a, mut b) = (empty_game(), empty_game());
        a.teleport(Vec2::new(30_000.0, -12_000.0));
        b.teleport(Vec2::new(30_000.0, -12_000.0));
        a.ping();
        b.ping();
        assert_eq!(a.ping.echoes, b.ping.echoes);
        for kind in [
            EchoKind::Planetoid,
            EchoKind::Civilization,
            EchoKind::Fortress,
            EchoKind::Pad,
        ] {
            let n = a.ping.echoes.iter().filter(|e| e.kind == kind).count();
            assert!(n <= kind.cap());
        }
        assert!(site_count(42, SectorId { x: 5, y: -2 }) < 50);
    }

    #[test]
    fn offscreen_echoes_get_arrows() {
        let mut game = empty_game();
        game.ping();
        run(&mut game, 4.0);
        let half = Vec2::new(900.0, 500.0);
        let arrows: Vec<Bearing> = game.echo_bearings(Vec2::ZERO, half);
        assert!(!arrows.is_empty());
        assert!(arrows.iter().all(|a| matches!(a.kind, GuideKind::Echo(..))));
        assert!(arrows.len() <= 4);
    }

    fn range_of(game: &Game) -> f32 {
        game.ping_ring_range()
    }

    fn skilled(skills: &[(Skill, u8)]) -> Game {
        let mut game = empty_game();
        for &(skill, level) in skills {
            for _ in 0..level {
                game.loadout.skills.raise(skill);
            }
        }
        game
    }

    /// The first sector near HOME (outward in rings) whose generation holds a site of `kind`.
    fn sector_with(kind: EchoKind) -> SectorId {
        for ring in 1..14i32 {
            for x in -ring..=ring {
                for y in -ring..=ring {
                    if x.abs().max(y.abs()) != ring {
                        continue;
                    }
                    let id = SectorId { x, y };
                    if sites_of(42, id).iter().any(|s| s.kind == kind) {
                        return id;
                    }
                }
            }
        }
        panic!("no {kind:?} site found");
    }

    fn kinds_after_ping(game: &mut Game, at: Vec2) -> Vec<Echo> {
        game.teleport(at);
        game.ping.cooldown = 0.0;
        assert!(game.ping());
        game.ping.echoes.clone()
    }

    #[test]
    fn the_base_ping_is_unchanged_and_new_kinds_are_hidden() {
        let mut game = empty_game();
        assert_eq!(game.ping_recharge(), PING_COOLDOWN);
        for id in [
            sector_with(EchoKind::Nest),
            sector_with(EchoKind::Predators),
            sector_with(EchoKind::Eggs),
            sector_with(EchoKind::Lode),
        ] {
            let echoes = kinds_after_ping(&mut game, id.center());
            assert!(echoes.iter().all(|e| e.kind.unlocked_by().is_none()));
            for kind in [EchoKind::Planetoid, EchoKind::Civilization] {
                let n = echoes.iter().filter(|e| e.kind == kind).count();
                assert!(n <= kind.cap());
            }
            assert!(
                echoes
                    .iter()
                    .all(|e| e.position.distance(id.center()) <= PING_RANGE)
            );
        }
    }

    #[test]
    fn reach_extends_the_ring_and_the_sectors_it_searches() {
        let start = Vec2::new(30_000.0, 7_000.0);
        let mut base = skilled(&[]);
        let mut far = skilled(&[(Skill::PingReach, 4), (Skill::PingTargets, 4)]);
        let near = kinds_after_ping(&mut base, start);
        let wide = kinds_after_ping(&mut far, start);
        assert!(!near.iter().any(|e| e.position.distance(start) > PING_RANGE));
        assert!(
            wide.iter()
                .all(|e| e.position.distance(start) <= range_of(&far))
        );
        assert_eq!(base.ping.cache.len(), 7 * 7, "three sectors each way");
        assert_eq!(far.ping.cache.len(), 13 * 13, "six sectors each way");
        let farthest = |v: &[Echo]| {
            v.iter()
                .map(|e| e.position.distance(start))
                .fold(0.0, f32::max)
        };
        assert!(farthest(&wide) > farthest(&near), "level 4 hears farther");
        assert_eq!(far.ping_ring_range(), PING_RANGE + 4.0 * t::PING_REACH_STEP);
        let range = far.ping_ring_range();
        run(&mut far, 1.0);
        let (_, radius) = far.ping_ring().unwrap();
        assert!(radius < range);
        run(&mut far, 6.0);
        assert!(far.ping_ring().is_none(), "the ring ends at its range");
    }

    #[test]
    fn speed_brings_echoes_back_sooner() {
        let start = Vec2::new(12_000.0, 3_000.0);
        let mut slow = skilled(&[]);
        let mut fast = skilled(&[(Skill::PingSpeed, 4)]);
        let a = kinds_after_ping(&mut slow, start);
        let b = kinds_after_ping(&mut fast, start);
        assert_eq!(a.len(), b.len(), "speed changes timing, not the answer");
        for (x, y) in a.iter().zip(&b) {
            assert!(y.born < x.born || x.born == slow.time);
        }
        fast.ping.cooldown = 0.0;
        let before = fast.time;
        run(&mut fast, 0.5);
        assert!(fast.time > before);
        let (_, r_fast) = fast.ping_ring().unwrap_or((Vec2::ZERO, 0.0));
        let (_, r_slow) = slow.ping_ring().unwrap();
        assert!(r_fast >= 0.0 && r_slow == 0.0);
    }

    #[test]
    fn recharge_shortens_per_level_down_to_a_floor() {
        let mut last = PING_COOLDOWN;
        for level in 1..=4u8 {
            let game = skilled(&[(Skill::PingCooldown, level)]);
            assert!(game.ping_recharge() < last);
            assert!(game.ping_recharge() >= t::PING_COOLDOWN_FLOOR);
            last = game.ping_recharge();
        }
        let mut game = skilled(&[(Skill::PingCooldown, 4)]);
        assert!(game.ping());
        assert!((game.ping_cooldown() - last).abs() < 1e-4);
        run(&mut game, last + 0.1);
        assert!(game.ping(), "ready again after the shorter wait");
    }

    #[test]
    fn targets_widen_every_kind_by_one_a_level() {
        let id = sector_with(EchoKind::Predators);
        let mut base = skilled(&[(Skill::EchoPredators, 1), (Skill::EchoLodes, 1)]);
        let mut wide = skilled(&[
            (Skill::EchoPredators, 1),
            (Skill::EchoLodes, 1),
            (Skill::PingTargets, 4),
        ]);
        let a = kinds_after_ping(&mut base, id.center());
        let b = kinds_after_ping(&mut wide, id.center());
        let count = |v: &[Echo], k| v.iter().filter(|e| e.kind == k).count();
        for kind in [EchoKind::Planetoid, EchoKind::Predators, EchoKind::Lode] {
            assert!(count(&a, kind) <= kind.cap());
            assert!(count(&b, kind) <= kind.cap() + 4);
            assert!(count(&b, kind) >= count(&a, kind));
        }
        assert!(b.len() > a.len(), "more echoes overall");
    }

    #[test]
    fn each_reveal_tier_shows_its_kind_only_once_bought() {
        let tiers = [
            (Skill::EchoLodes, EchoKind::Lode),
            (Skill::EchoNests, EchoKind::Nest),
            (Skill::EchoNests, EchoKind::Eggs),
            (Skill::EchoPredators, EchoKind::Predators),
        ];
        for (skill, kind) in tiers {
            let at = sector_with(kind).center();
            let mut locked = skilled(&[]);
            assert!(
                !kinds_after_ping(&mut locked, at)
                    .iter()
                    .any(|e| e.kind == kind),
                "{kind:?} stays silent while locked"
            );
            let mut owned = skilled(&[(skill, 1)]);
            let found = kinds_after_ping(&mut owned, at);
            let echo = found
                .iter()
                .find(|e| e.kind == kind)
                .unwrap_or_else(|| panic!("{kind:?} answers once {skill:?} is owned"));
            assert!(echo.position.distance(at) <= PING_RANGE);
            match kind {
                EchoKind::Predators => assert!(echo.weight >= 1.0, "a density reading"),
                EchoKind::Nest => assert!(echo.weight >= 0.0),
                EchoKind::Eggs => assert!(echo.weight >= 1.0),
                EchoKind::Lode => assert!(echo.tint.is_some()),
                _ => {}
            }
        }
    }

    #[test]
    fn lodes_replace_the_plain_planetoid_echo_for_renewable_ones() {
        let mut locked = skilled(&[]);
        let mut owned = skilled(&[(Skill::EchoLodes, 1)]);
        let at = Vec2::new(3_000.0, -2_000.0);
        let before = kinds_after_ping(&mut locked, at);
        let after = kinds_after_ping(&mut owned, at);
        let lode_at: Vec<Vec2> = after
            .iter()
            .filter(|e| e.kind == EchoKind::Lode)
            .map(|e| e.position)
            .collect();
        for echo in after.iter().filter(|e| e.kind == EchoKind::Planetoid) {
            assert!(!lode_at.contains(&echo.position), "no double marker");
        }
        assert!(
            after
                .iter()
                .filter(|e| e.kind == EchoKind::Planetoid)
                .count()
                <= CAP_PLANETOID
        );
        assert!(!before.is_empty());
    }

    #[test]
    fn a_destroyed_nest_or_a_cleared_sector_goes_quiet() {
        let id = sector_with(EchoKind::Nest);
        let sites = sites_of(42, id);
        let nest = sites.iter().find(|s| s.kind == EchoKind::Nest).unwrap();
        let mut game = skilled(&[(Skill::EchoNests, 1)]);
        let at = nest.position;
        let seen = |game: &mut Game| {
            kinds_after_ping(game, at)
                .iter()
                .any(|e| e.kind == EchoKind::Nest && e.position.distance(at) < 1.0)
        };
        assert!(seen(&mut game));
        game.fallen
            .entry(id)
            .or_default()
            .extend(nest.members.iter().copied());
        assert!(!seen(&mut game), "every stone is gone");
    }

    #[test]
    fn upgraded_pings_are_deterministic() {
        let skills = [
            (Skill::PingReach, 2),
            (Skill::PingTargets, 2),
            (Skill::EchoLodes, 1),
            (Skill::EchoNests, 1),
            (Skill::EchoPredators, 1),
        ];
        let (mut a, mut b) = (skilled(&skills), skilled(&skills));
        let at = Vec2::new(-20_000.0, 9_000.0);
        assert_eq!(kinds_after_ping(&mut a, at), kinds_after_ping(&mut b, at));
    }
}
