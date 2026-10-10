//! Sonar: the ship sends a ring out and, as it passes, the nearest planetoids, civilization
//! seats, fortresses and the player's own landing pads answer with echoes that linger and
//! fade. The answers come from the world generator, a pure function of the seed and sector
//! coordinates, so they reach sectors far beyond the simulated region without loading them.
//! Curiosity (`discovery`) adds existing live rifts and dynamic wells to base ping, and sealed
//! organ relics to LODE ECHO. Those handles resolve after tick cleanup; live targets are never
//! inferred from generation or predicted poses.
//! Echoes are information only: nothing in the rules reads them, and a ping costs nothing but
//! its cooldown.
//!
//! The base ping always also answers with the nearest living civilization, wherever it is
//! (`EchoKind::Nearest`, found by `territory::nearest_civilization`, searched out to
//! `territory::SEARCH_CELLS`): a long-range bearing blip that carries its distance in
//! sectors, however far the ring itself reaches.
//!
//! The base ping is what the ship starts with. Bench upgrades (`skills`) stretch its reach,
//! speed it up, shorten its cooldown and widen its answer, and four reveal tiers, each bought
//! once, add echo kinds: pads the enemy has found, rich lodes and renewable planetoids, nests
//! and egg clusters, and the predator density of each sector.

use super::mining::{Material, material_of, ore_for, renewable};
use super::skills::Skill;
use super::{BodyKind, Cue, Game};
use crate::genome::Niche;
use crate::simulation::Tunables;
use crate::territory::{CivRole, SeatCache, Standing};
use crate::world::{self, RockKind, SECTOR_SIZE, SectorId, Spawn};
use bevy::prelude::Vec2;
use std::collections::HashMap;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum EchoKind {
    Planetoid,
    /// An outpost of a civilization.
    Civilization,
    /// A civilization's capital, walled.
    Fortress,
    /// The nearest civilization wherever it lies: a bearing blip at the nearest sector of its
    /// territory (its capital's, from inside); `weight` is the distance in sectors.
    Nearest,
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
    /// Temporary, live paired doorway.
    Rift,
    /// Live moving or changing gravity hazard.
    Well,
    /// Sealed organ specimen, revealed by LODES.
    Relic,
}

impl EchoKind {
    fn cap(self, tune: &Tunables) -> usize {
        match self {
            Self::Rift => 4,
            Self::Well | Self::Relic => 2,
            Self::Planetoid => tune.ping_cap_planetoid,
            Self::Civilization => tune.ping_cap_civilization,
            Self::Fortress => tune.ping_cap_fortress,
            Self::Nearest => 1,
            Self::Pad => tune.ping_cap_pad,
            Self::PadAlert => tune.cap_pad_alert,
            Self::Lode => tune.cap_lode,
            Self::Nest => tune.cap_nest,
            Self::Eggs => tune.cap_eggs,
            Self::Predators => tune.cap_predators,
        }
    }

    /// The sonar upgrade that reveals this kind, if it is not part of the base ping.
    pub fn unlocked_by(self) -> Option<Skill> {
        match self {
            Self::PadAlert => Some(Skill::EchoPads),
            Self::Lode | Self::Relic => Some(Skill::EchoLodes),
            Self::Nest | Self::Eggs => Some(Skill::EchoNests),
            Self::Predators => Some(Skill::EchoPredators),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Echo {
    pub(super) target: Option<super::discovery::Target>,
    pub(super) scan: Option<Ring>,
    /// Concise live discovery information.
    pub discovery: Option<super::discovery::Info>,
    pub kind: EchoKind,
    pub position: Vec2,
    /// Size of the thing (a planetoid's radius), for drawing its marker.
    pub radius: f32,
    /// A civilization's tint, or a lode's material color.
    pub tint: Option<[f32; 3]>,
    /// How much there is: predators in a sector, creatures in a nest, eggs in a husk, ore in
    /// a lode. Zero when it does not apply.
    pub weight: f32,
    /// A lode that regrows (a renewable planetoid).
    pub renewable: bool,
    /// Game time at which the ring reaches it and it sounds.
    pub born: f32,
    pub(super) sounded: bool,
}

/// The nearest civilization as the ship's last ping reported it.
#[derive(Clone, Debug, PartialEq)]
pub struct NearestReport {
    pub name: String,
    pub tint: [f32; 3],
    /// Distance from the ship now, in sectors.
    pub sectors: f32,
    /// Unit direction from the ship (zero when it is on top of it).
    pub direction: Vec2,
    /// Remaining brightness in (0, 1].
    pub fade: f32,
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
pub(super) struct Site {
    pub(super) kind: EchoKind,
    pub(super) position: Vec2,
    pub(super) radius: f32,
    pub(super) tint: Option<[f32; 3]>,
    pub(super) weight: f32,
    /// Spawn indices that make up the site; it is gone when all of them are destroyed.
    pub(super) members: Vec<u32>,
    pub(super) territory: Option<u64>,
    /// A planetoid that regrows: it answers as a lode once that tier is owned.
    pub(super) renewable: bool,
}

#[derive(Default)]
pub struct PingState {
    pub(super) cooldown: f32,
    pub(super) ring: Option<Ring>,
    pub(super) echoes: Vec<Echo>,
    cache: HashMap<SectorId, Vec<Site>>,
    seats: SeatCache,
}

impl PingState {
    /// The sites of a sector, generated once.
    pub(super) fn sites(&mut self, seed: u64, id: SectorId, tune: &Tunables) -> &[Site] {
        self.cache
            .entry(id)
            .or_insert_with(|| sites_of(seed, id, tune))
    }
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

pub(super) fn sites_of(seed: u64, id: SectorId, tune: &Tunables) -> Vec<Site> {
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
                site.renewable = renewable(seed, key, tune);
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
            (
                BodyKind::Asteroid,
                rock @ (RockKind::Plain | RockKind::Ore | RockKind::Crystal | RockKind::Ice),
                _,
            ) if !spawn.pinned => {
                let contents = super::mining::asteroid_contents(seed, (id, spawn.index));
                if contents.amounts().next().is_none() {
                    continue;
                }
                let ore = ore_for(rock, spawn.radius.unwrap_or(0.0));
                if ore >= tune.lode_min_ore {
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
            .filter(|s| {
                s.kind == BodyKind::Creature && s.position.distance(heart) < tune.ping_nest_reach
            })
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
        self.loadout
            .skills
            .ping_cooldown(self.tune.ping_cooldown, &self.tune)
    }

    /// Sends a ping from the ship. Refused (false) while recharging, dead or over.
    pub fn ping(&mut self) -> bool {
        if self.game_over || self.ping.cooldown > 0.0 {
            return false;
        }
        if self.sonar_blinded() {
            // A glare's flash: the ring will not go out until it fades (Faraday shortens it).
            self.notify_once(
                "SONAR BLIND  A GLARE HAS IT  WAIT, OR FARADAY SHORTENS IT".into(),
                super::upgrades::Rarity::Common,
            );
            return false;
        }
        self.send_ping(false)
    }

    /// Sends the ping. A `free` one (the one on entering a sector) leaves the cooldown alone.
    pub(super) fn send_ping(&mut self, free: bool) -> bool {
        let Some(origin) = self.player().map(|p| p.position) else {
            return false;
        };
        let skills = self.loadout.skills;
        let range =
            skills.ping_range(self.tune.ping_range, &self.tune) * self.realm_effects().sensor;
        let speed = skills.ping_speed(self.tune.ping_ring_speed, &self.tune);
        let owns = |kind: EchoKind| kind.unlocked_by().is_none_or(|s| skills.level(s) > 0);
        if !free {
            self.ping.cooldown = skills.ping_cooldown(self.tune.ping_cooldown, &self.tune);
        }
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
                    .or_insert_with(|| sites_of(seed, id, &self.tune))
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
                        target: None,
                        scan: None,
                        discovery: None,
                        kind,
                        position: site.position,
                        radius: site.radius,
                        tint: site.tint,
                        weight,
                        renewable: site.renewable,
                        born: self.time + distance / speed,
                        sounded: false,
                    });
                }
            }
        }
        // The nearest civilization, however far: it sounds when the ring reaches it, or as
        // the ring dies for one beyond its range.
        let seed = self.seed;
        let mut seats = std::mem::take(&mut self.ping.seats);
        let fallen =
            |t: &crate::territory::Territory| t.standing(self.civ_fall(t.id)) == Standing::Fallen;
        let nearest = crate::territory::nearest_civilization(seed, origin, &mut seats, &fallen);
        self.ping.seats = seats;
        if let Some(near) = nearest {
            let position = near.sector.center();
            let distance = position.distance(origin);
            found.push(Echo {
                target: None,
                scan: None,
                discovery: None,
                kind: EchoKind::Nearest,
                position,
                radius: 0.0,
                tint: Some(near.territory.color(seed)),
                weight: distance / SECTOR_SIZE,
                renewable: false,
                born: self.time + distance.min(range) / speed,
                sounded: false,
            });
        }
        for pad in self.pads() {
            let position = self.pad_position(pad);
            let distance = position.distance(origin);
            if distance <= range {
                let alert = owns(EchoKind::PadAlert) && self.pad_exposed(pad.key);
                found.push(Echo {
                    target: None,
                    scan: None,
                    discovery: None,
                    kind: if alert {
                        EchoKind::PadAlert
                    } else {
                        EchoKind::Pad
                    },
                    position,
                    radius: 0.0,
                    tint: None,
                    weight: 0.0,
                    renewable: false,
                    born: self.time + distance / speed,
                    sounded: false,
                });
            }
        }
        found.extend(self.discovery_candidates(self.ping.ring.unwrap()));
        found.sort_by(|a, b| a.born.total_cmp(&b.born));
        let extra = skills.ping_extra_targets(&self.tune);
        let mut taken: HashMap<EchoKind, usize> = HashMap::new();
        found.retain(|echo| {
            let n = taken.entry(echo.kind).or_default();
            *n += 1;
            echo.discovery.is_some() || *n <= echo.kind.cap(&self.tune) + extra
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
        let old = std::mem::take(&mut self.ping.echoes);
        self.ping.echoes = old
            .into_iter()
            .filter_map(|e| self.refresh_discovery(e))
            .collect();
        let mut sounded = Vec::new();
        for echo in &mut self.ping.echoes {
            if !echo.sounded && time >= echo.born {
                echo.sounded = true;
                sounded.push(*echo);
            }
        }
        self.ping
            .echoes
            .retain(|e| time < e.born + self.tune.ping_echo_life);
        let any = !sounded.is_empty();
        for echo in sounded {
            self.chart_learn_echo(&echo);
            self.cue(Cue::Echo { at: echo.position });
            if echo.discovery.is_none()
                && let Some(kind) = super::lure::LureKind::of_echo(echo.kind, echo.renewable)
            {
                self.consider_lure(super::lure::Lure {
                    kind,
                    position: echo.position,
                });
            }
        }
        // An apex elder in reach is the marker of last resort.
        if any && let Some(apex) = self.apex_report() {
            self.consider_lure(super::lure::Lure {
                kind: super::lure::LureKind::Apex,
                position: apex.position,
            });
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
        self.ping.ring.map_or(self.tune.ping_range, |r| r.range)
    }

    /// What the sounded nearest-civilization echo says: who, how far in sectors and which
    /// way from the ship, while it lingers.
    pub fn nearest_civilization(&self) -> Option<NearestReport> {
        let ship = self.player()?.position;
        let (echo, fade) = self.echoes().find(|(e, _)| e.kind == EchoKind::Nearest)?;
        let territory = world::territory(self.seed, SectorId::containing(echo.position))?;
        Some(NearestReport {
            name: territory.name(self.seed),
            tint: echo.tint.unwrap_or([0.8, 0.8, 0.8]),
            sectors: echo.position.distance(ship) / SECTOR_SIZE,
            direction: (echo.position - ship).normalize_or_zero(),
            fade,
        })
    }

    /// Echoes that have sounded, with their remaining brightness in (0, 1].
    pub fn echoes(&self) -> impl Iterator<Item = (&Echo, f32)> {
        let time = self.time;
        self.ping
            .echoes
            .iter()
            .filter(move |e| {
                time >= e.born
                    && time < e.born + self.tune.ping_echo_life
                    && self.discovery_valid(e)
            })
            .map(move |e| {
                (
                    e,
                    (1.0 - (time - e.born) / self.tune.ping_echo_life).clamp(0.0, 1.0),
                )
            })
    }
}

/// Planetoids a ping would find, for tests of the far-reach claim.
#[cfg(test)]
pub(super) fn site_count(seed: u64, id: SectorId) -> usize {
    sites_of(seed, id, &crate::simulation::DEFAULT_TUNING).len()
}

#[cfg(test)]
mod tests {
    use super::super::tests::{DT, empty_game};
    use super::super::{Bearing, GuideKind, Input};
    use super::*;
    use crate::simulation::DEFAULT_TUNING;

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
        run(&mut game, DEFAULT_TUNING.ping_cooldown + 0.2);
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
            assert!((delay - distance / DEFAULT_TUNING.ping_ring_speed).abs() < 1e-3);
        }
        run(&mut game, 1.0);
        let heard = game.echoes().count();
        let due = all.iter().filter(|e| e.born <= game.time).count();
        assert_eq!(heard, due);
        run(&mut game, 3.0);
        assert_eq!(game.echoes().count(), all.len());
        run(&mut game, DEFAULT_TUNING.ping_echo_life + 1.0);
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
            assert!(n <= kind.cap(&DEFAULT_TUNING));
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
                    if sites_of(42, id, &DEFAULT_TUNING)
                        .iter()
                        .any(|s| s.kind == kind)
                    {
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
        // The nearest-civilization bearing is exempt from the ring's range (tested apart).
        let mut echoes = game.ping.echoes.clone();
        echoes.retain(|e| e.kind != EchoKind::Nearest);
        echoes
    }

    #[test]
    fn the_base_ping_is_unchanged_and_new_kinds_are_hidden() {
        let mut game = empty_game();
        assert_eq!(game.ping_recharge(), DEFAULT_TUNING.ping_cooldown);
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
                assert!(n <= kind.cap(&DEFAULT_TUNING));
            }
            assert!(
                echoes
                    .iter()
                    .all(|e| e.position.distance(id.center()) <= DEFAULT_TUNING.ping_range)
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
        assert!(
            !near
                .iter()
                .any(|e| e.position.distance(start) > DEFAULT_TUNING.ping_range)
        );
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
        assert_eq!(
            far.ping_ring_range(),
            DEFAULT_TUNING.ping_range + 4.0 * DEFAULT_TUNING.ping_reach_step
        );
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
        let mut last = DEFAULT_TUNING.ping_cooldown;
        for level in 1..=4u8 {
            let game = skilled(&[(Skill::PingCooldown, level)]);
            assert!(game.ping_recharge() < last);
            assert!(game.ping_recharge() >= DEFAULT_TUNING.ping_cooldown_floor);
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
            assert!(count(&a, kind) <= kind.cap(&DEFAULT_TUNING));
            assert!(count(&b, kind) <= kind.cap(&DEFAULT_TUNING) + 4);
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
            assert!(echo.position.distance(at) <= DEFAULT_TUNING.ping_range);
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
                <= DEFAULT_TUNING.ping_cap_planetoid
        );
        assert!(!before.is_empty());
    }

    #[test]
    fn a_destroyed_nest_or_a_cleared_sector_goes_quiet() {
        let id = sector_with(EchoKind::Nest);
        let sites = sites_of(42, id, &DEFAULT_TUNING);
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

    #[test]
    fn the_base_ping_always_names_the_nearest_civilization_and_how_far() {
        for seed in [42_u64, 7, 99] {
            let mut game = empty_game();
            game.seed = seed;
            let at = Vec2::new(-90.0, 140.0) * SECTOR_SIZE;
            game.bodies[0].position = at;
            assert!(game.ping());
            let echo = game
                .ping
                .echoes
                .iter()
                .find(|e| e.kind == EchoKind::Nearest)
                .copied()
                .expect("the nearest civilization answers wherever the ship is");
            let truth =
                crate::territory::nearest_civilization(seed, at, &mut Default::default(), &|_| {
                    false
                })
                .unwrap();
            assert_eq!(echo.position, truth.sector.center());
            assert!((echo.weight - echo.position.distance(at) / SECTOR_SIZE).abs() < 1e-3);
            assert!(echo.tint.is_some());
            // It sounds no later than the ring runs out, however far the thing is.
            assert!(
                echo.born - game.time
                    <= DEFAULT_TUNING.ping_range / DEFAULT_TUNING.ping_ring_speed + 1e-3
            );
            run(
                &mut game,
                DEFAULT_TUNING.ping_range / DEFAULT_TUNING.ping_ring_speed + 0.5,
            );
            let report = game
                .nearest_civilization()
                .expect("reported while it lingers");
            assert!((report.sectors - echo.weight).abs() < 0.05);
            assert!((report.direction.length() - 1.0).abs() < 1e-3);
            let bearings = game.echo_bearings(at, Vec2::new(900.0, 500.0));
            assert!(
                matches!(
                    bearings.first().map(|b| b.kind),
                    Some(GuideKind::Echo(EchoKind::Nearest, _))
                ),
                "the arrow to the nearest civilization leads"
            );
        }
    }

    #[test]
    fn from_home_the_base_ping_finds_the_early_outpost() {
        let mut game = empty_game();
        assert!(game.ping());
        let echo = game
            .ping
            .echoes
            .iter()
            .find(|e| e.kind == EchoKind::Nearest)
            .expect("an outpost lies near HOME");
        let outpost = crate::territory::outpost(game.seed);
        assert!(
            world::territory(game.seed, SectorId::containing(echo.position))
                .is_some_and(|t| t.id == outpost.id)
        );
        assert!(echo.weight < 5.0, "{} sectors", echo.weight);
    }

    #[test]
    fn a_fallen_civilization_is_not_the_nearest_any_more() {
        let mut game = empty_game();
        let outpost = crate::territory::outpost(game.seed);
        game.civs.fall.insert(
            outpost.id,
            crate::territory::Fall {
                capital: true,
                elder: true,
            },
        );
        assert!(game.ping());
        let echo = game
            .ping
            .echoes
            .iter()
            .find(|e| e.kind == EchoKind::Nearest)
            .expect("another one is found");
        assert!(
            world::territory(game.seed, SectorId::containing(echo.position))
                .is_some_and(|t| t.id != outpost.id)
        );
    }
}
