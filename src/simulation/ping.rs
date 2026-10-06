//! Sonar: the ship sends a ring out and, as it passes, the nearest planetoids, civilization
//! seats, fortresses and the player's own landing pads answer with echoes that linger and
//! fade. The answers come from the world generator, a pure function of the seed and sector
//! coordinates, so they reach sectors far beyond the simulated region without loading them.
//! Echoes are information only: nothing in the rules reads them, and a ping costs nothing but
//! its cooldown.

use super::{BodyKind, Cue, Game};
use crate::territory::{CivRole, Standing};
use crate::world::{self, RockKind, SectorId};
use bevy::prelude::Vec2;
use std::collections::HashMap;

/// Seconds before the ship may ping again.
pub const PING_COOLDOWN: f32 = 5.0;
/// How far the ring travels, in units, and how fast.
pub const PING_RANGE: f32 = 20_000.0;
pub const RING_SPEED: f32 = 7_000.0;
/// Seconds an echo lasts once it has sounded.
pub const ECHO_LIFE: f32 = 9.0;
/// Sectors searched around the ship (Chebyshev); covers `PING_RANGE` in every direction.
const SEARCH: i32 = 3;
/// Most echoes of each kind.
const CAP_PLANETOID: usize = 3;
const CAP_CIVILIZATION: usize = 2;
const CAP_FORTRESS: usize = 2;
const CAP_PAD: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EchoKind {
    Planetoid,
    /// An outpost of a civilization.
    Civilization,
    /// A civilization's capital, walled.
    Fortress,
    /// One of the player's own landing pads.
    Pad,
}

impl EchoKind {
    fn cap(self) -> usize {
        match self {
            Self::Planetoid => CAP_PLANETOID,
            Self::Civilization => CAP_CIVILIZATION,
            Self::Fortress => CAP_FORTRESS,
            Self::Pad => CAP_PAD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Echo {
    pub kind: EchoKind,
    pub position: Vec2,
    /// Size of the thing (a planetoid's radius), for drawing its marker.
    pub radius: f32,
    /// A civilization's tint.
    pub tint: Option<[f32; 3]>,
    /// Game time at which the ring reaches it and it sounds.
    pub born: f32,
    sounded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub origin: Vec2,
    pub started: f32,
}

/// A generated site that could answer a ping, cached per sector (generation is pure).
#[derive(Clone, Copy, Debug)]
struct Site {
    kind: EchoKind,
    position: Vec2,
    radius: f32,
    tint: Option<[f32; 3]>,
    index: u32,
    territory: Option<u64>,
}

#[derive(Default)]
pub struct PingState {
    cooldown: f32,
    pub(super) ring: Option<Ring>,
    pub(super) echoes: Vec<Echo>,
    cache: HashMap<SectorId, Vec<Site>>,
}

fn sites_of(seed: u64, id: SectorId) -> Vec<Site> {
    let territory = world::territory(seed, id);
    let tint = territory.map(|t| t.color(seed));
    world::generate(seed, id)
        .into_iter()
        .filter_map(|spawn| {
            let (kind, tint, territory) = match (spawn.kind, spawn.rock, spawn.civ) {
                (BodyKind::Asteroid, RockKind::Planetoid, _) => (EchoKind::Planetoid, None, None),
                (BodyKind::Base, _, Some(tag)) if spawn.fort.is_none() => match tag.role {
                    CivRole::Capital => (EchoKind::Fortress, tint, Some(tag.territory)),
                    CivRole::Outpost => (EchoKind::Civilization, tint, Some(tag.territory)),
                    _ => return None,
                },
                _ => return None,
            };
            Some(Site {
                kind,
                position: spawn.position,
                radius: spawn.radius.unwrap_or(0.0),
                tint,
                index: spawn.index,
                territory,
            })
        })
        .collect()
}

impl Game {
    /// Sends a ping from the ship. Refused (false) while recharging, dead or over.
    pub fn ping(&mut self) -> bool {
        if self.game_over || self.ping.cooldown > 0.0 {
            return false;
        }
        let Some(origin) = self.player().map(|p| p.position) else {
            return false;
        };
        self.ping.cooldown = PING_COOLDOWN;
        self.ping.ring = Some(Ring {
            origin,
            started: self.time,
        });
        let mut found: Vec<Echo> = Vec::new();
        let home = SectorId::containing(origin);
        for dx in -SEARCH..=SEARCH {
            for dy in -SEARCH..=SEARCH {
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
                    let gone = fallen.is_some_and(|f| f.contains(&site.index))
                        || site
                            .territory
                            .is_some_and(|t| self.civ_standing(t) == Standing::Fallen);
                    let distance = site.position.distance(origin);
                    if gone || distance > PING_RANGE {
                        continue;
                    }
                    found.push(Echo {
                        kind: site.kind,
                        position: site.position,
                        radius: site.radius,
                        tint: site.tint,
                        born: self.time + distance / RING_SPEED,
                        sounded: false,
                    });
                }
            }
        }
        for pad in self.pads() {
            let position = self.pad_position(pad);
            let distance = position.distance(origin);
            if distance <= PING_RANGE {
                found.push(Echo {
                    kind: EchoKind::Pad,
                    position,
                    radius: 0.0,
                    tint: None,
                    born: self.time + distance / RING_SPEED,
                    sounded: false,
                });
            }
        }
        found.sort_by(|a, b| a.born.total_cmp(&b.born));
        let mut taken: HashMap<EchoKind, usize> = HashMap::new();
        found.retain(|echo| {
            let n = taken.entry(echo.kind).or_default();
            *n += 1;
            *n <= echo.kind.cap()
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
            .is_some_and(|r| (time - r.started) * RING_SPEED > PING_RANGE)
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
            .map(|r| (r.origin, ((self.time - r.started) * RING_SPEED).max(0.0)))
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
}
