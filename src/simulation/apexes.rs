//! Apex elders in play (generation is `crate::apex`): the banner when one stirs, its HUD
//! readout, and what its death pays and records. A slain apex is remembered by spawn index
//! like any kill, so it does not return in this world instance.

use super::tuning as t;
use super::upgrades::{self, Item, Rarity, Slot, Source};
use super::*;
use crate::apex::Rank;

/// What the simulation knows of a generated apex.
#[derive(Clone, Debug, PartialEq)]
pub struct ApexInfo {
    /// Display name, uppercase.
    pub name: String,
    pub rank: Rank,
}

/// The HUD's account of the nearest apex.
#[derive(Clone, Debug, PartialEq)]
pub struct ApexReport {
    pub name: String,
    pub rank: Rank,
    pub distance: f32,
    pub position: Vec2,
    /// Hull remaining, 0 to 1.
    pub health: f32,
    pub alert: bool,
}

impl Game {
    /// Remembers an apex when its sector loads.
    pub(super) fn register_apex(&mut self, id: SectorId, index: u32, rank: Rank) {
        let name = crate::apex::name(self.seed, id).to_uppercase();
        self.apexes.insert((id, index), ApexInfo { name, rank });
    }

    /// The apex a body is, if it is one.
    pub fn apex_of(&self, body: &Body) -> Option<&ApexInfo> {
        if body.kind != BodyKind::Creature {
            return None;
        }
        self.apexes.get(&body.origin?)
    }

    /// Posts the banner the first time an apex comes into range.
    pub(super) fn update_apex(&mut self) {
        if self.apexes.is_empty() {
            return;
        }
        let Some(ship) = self.player().map(|p| p.position) else {
            return;
        };
        let mut stirred: Vec<((SectorId, u32), String)> = Vec::new();
        for body in self.bodies.iter().filter(|b| b.active && !b.consumed) {
            if let Some(key) = body.origin
                && !self.apex_seen.contains(&key)
                && body.position.distance(ship) < t::APEX_NOTICE_RANGE
                && let Some(info) = self.apex_of(body)
            {
                stirred.push((key, info.name.clone()));
            }
        }
        for (key, name) in stirred {
            self.apex_seen.insert(key);
            self.notify(format!("APEX: {name} stirs"), Rarity::Epic);
            self.cue(Cue::Extirpated);
        }
    }

    /// The nearest living apex within `APEX_HUD_RANGE`, for the HUD and the arrows.
    pub fn apex_report(&self) -> Option<ApexReport> {
        let ship = self.player()?.position;
        self.bodies
            .iter()
            .filter(|b| b.active && !b.consumed && !b.follower)
            .filter_map(|b| self.apex_of(b).map(|info| (b, info)))
            .map(|(b, info)| ApexReport {
                name: info.name.clone(),
                rank: info.rank,
                distance: b.position.distance(ship),
                position: b.position,
                health: (b.health / b.max_health).clamp(0.0, 1.0),
                alert: b.alert,
            })
            .filter(|r| r.distance <= t::APEX_HUD_RANGE)
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
    }

    /// An apex died: record it by name, pay the bounty and post the banner.
    pub(super) fn apex_slain(&mut self, body: &Body) {
        if body.consumed {
            return;
        }
        let Some(info) = self.apex_of(body).cloned() else {
            return;
        };
        let share = if info.rank == Rank::Major { 1.0 } else { 0.5 };
        self.score = self
            .score
            .saturating_add((t::APEX_SCORE * share * body.genes.threat) as u64);
        self.run.apex_slain.push(info.name.clone());
        self.notify(format!("APEX SLAIN: {}", info.name), Rarity::Epic);
        self.cue(Cue::Extirpated);
    }

    /// What a slain apex leaves: materials of every kind, an epic part in the slot of an
    /// ability the ship cannot yet do (plating for parry, engine for dash, else cannon), two
    /// rare parts and two lucky rolls.
    pub(super) fn apex_loot(&self, body: &Body, rng: &mut Rng, params: SectorParams) -> Vec<Item> {
        let Some(info) = self.apex_of(body) else {
            return Vec::new();
        };
        let share = if info.rank == Rank::Major { 1.0 } else { 0.5 };
        let mut drops: Vec<Item> = Material::ALL
            .into_iter()
            .map(|m| Item::Material(m, (t::APEX_MATERIAL * share).round()))
            .collect();
        let wanted = if self.loadout.skills.level(skills::Skill::Parry) == 0 {
            Slot::Plating
        } else if self.loadout.skills.level(skills::Skill::Dash) == 0 {
            Slot::Engine
        } else {
            Slot::Cannon
        };
        let mut source = Source::of_creature(&body.genome, body.genes.threat * 1.25, params);
        source.bias = 1.0;
        source.affinity = [0.05; Slot::ALL.len()];
        source.affinity[wanted.index()] = 40.0;
        source.min_rarity = Rarity::Epic;
        let mut token = upgrades::roll_part(rng, &source);
        for _ in 0..16 {
            if token.slot == wanted {
                break;
            }
            token = upgrades::roll_part(rng, &source);
        }
        drops.push(Item::Part(token));
        let mut source = Source::of_creature(&body.genome, body.genes.threat * 1.25, params);
        source.bias = 1.0;
        source.min_rarity = Rarity::Rare;
        for _ in 0..2 {
            drops.push(Item::Part(upgrades::roll_part(rng, &source)));
        }
        for _ in 0..2 {
            drops.push(upgrades::roll_item(rng, &source));
        }
        drops
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::DT;
    use crate::world::Spawn;

    const SEED: u64 = 0x535343;

    /// The first sector holding an apex of `rank`, nearest HOME first.
    fn find(seed: u64, rank: Rank) -> SectorId {
        let mut found: Vec<SectorId> = Vec::new();
        for x in -30..=30 {
            for y in -30..=30 {
                let id = SectorId { x, y };
                if crate::apex::rank(seed, id) == Some(rank)
                    && !world::generate(seed, id).is_empty()
                    && world::generate(seed, id)
                        .last()
                        .is_some_and(|s| s.apex.is_some())
                {
                    found.push(id);
                }
            }
        }
        found.sort_by_key(|id| (crate::range::ring(*id), id.x, id.y));
        *found.first().expect("an apex sector")
    }

    fn apex_spawn(seed: u64, id: SectorId) -> Spawn {
        world::generate(seed, id)
            .pop()
            .filter(|s| s.apex.is_some())
            .unwrap()
    }

    fn visit(id: SectorId) -> Game {
        let at = apex_spawn(SEED, id).position;
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.teleport(at + Vec2::new(0.0, 900.0));
        game.step(DT, Input::default());
        game
    }

    fn the_apex(game: &Game) -> &Body {
        game.bodies
            .iter()
            .find(|b| game.apex_of(b).is_some())
            .expect("an apex is loaded")
    }

    #[test]
    fn apexes_are_deterministic_named_and_the_last_spawn_of_their_sector() {
        let id = find(SEED, Rank::Major);
        let a = world::generate(SEED, id);
        assert_eq!(a, world::generate(SEED, id));
        let last = a.last().unwrap();
        assert_eq!(last.apex, Some(Rank::Major));
        assert_eq!(last.index as usize, a.len() - 1);
        assert_eq!(a.iter().filter(|s| s.apex.is_some()).count(), 1);
        let name = crate::apex::name(SEED, id);
        assert_eq!(name, crate::apex::name(SEED, id));
        assert!(name.contains(' ') && name.len() > 6, "{name}");
        // Another seed gives other elders.
        assert_ne!(crate::apex::name(SEED ^ 1, id), name);
    }

    #[test]
    fn apexes_are_rare_and_never_in_the_opening_rings() {
        let (mut major, mut lesser, mut sectors) = (0, 0, 0);
        for seed in [SEED, 42] {
            for x in -40..=40 {
                for y in -40..=40 {
                    let id = SectorId { x, y };
                    let ring = crate::range::ring(id);
                    match crate::apex::rank(seed, id) {
                        Some(Rank::Major) => {
                            assert!(ring >= crate::apex::APEX_RING, "{id:?}");
                            major += 1;
                        }
                        Some(Rank::Lesser) => {
                            assert!(
                                (crate::apex::LESSER_RING..crate::apex::APEX_RING).contains(&ring)
                            );
                            lesser += 1;
                        }
                        None => {}
                    }
                    if ring >= crate::apex::APEX_RING {
                        sectors += 1;
                    }
                }
            }
        }
        let rate = major as f32 / sectors as f32;
        assert!((0.008..0.04).contains(&rate), "major rate {rate}");
        assert!(lesser < major / 3 + 1, "{lesser} lesser against {major}");
        for ring in 0..crate::apex::LESSER_RING as i32 {
            for x in -ring..=ring {
                for y in -ring..=ring {
                    assert!(crate::apex::rank(SEED, SectorId { x, y }).is_none());
                }
            }
        }
    }

    #[test]
    fn an_apex_is_a_grown_up_single_body_within_the_sector_budget() {
        for seed in [SEED, 42, 7] {
            let mut checked = 0;
            for x in -25..=25 {
                for y in -25..=25 {
                    let id = SectorId { x, y };
                    let spawns = world::generate(seed, id);
                    let Some(spawn) = spawns.last().filter(|s| s.apex.is_some()) else {
                        continue;
                    };
                    let g = spawn.species.unwrap().genome;
                    assert_eq!(g.parts(), 1);
                    assert!(
                        g.radius >= 34.0 && g.hull >= 100.0 && g.shield >= 40.0,
                        "{g:?}"
                    );
                    assert!(g.bounty >= 400.0);
                    let used: u32 = spawns
                        .iter()
                        .filter_map(|s| s.species)
                        .map(|s| s.genome.parts())
                        .sum();
                    assert!(used <= world::SECTOR_BODY_BUDGET, "{used}");
                    checked += 1;
                }
            }
            assert!(checked >= 3, "{seed}: {checked}");
        }
    }

    #[test]
    fn an_apex_stirs_once_and_shows_on_the_hud_and_the_arrows() {
        let id = find(SEED, Rank::Major);
        let name = crate::apex::name(SEED, id).to_uppercase();
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        let at = apex_spawn(SEED, id).position;
        game.teleport(at + Vec2::new(0.0, 1400.0));
        let mut banners = 0;
        for _ in 0..120 {
            game.step(DT, Input::default());
            banners += game
                .notices
                .iter()
                .filter(|n| n.text == format!("APEX: {name} stirs"))
                .count();
            game.notices.clear();
        }
        assert_eq!(banners, 1, "announced exactly once");
        let report = game.apex_report().expect("in range of the HUD");
        assert_eq!(report.name, name);
        assert!(report.distance < 3600.0 && report.health > 0.99);
        // Off screen it has its own arrow, even with ordinary arrows ignoring it.
        let bearings = game.apex_bearings(game.focus, Vec2::new(300.0, 200.0));
        assert_eq!(bearings.len(), 1);
        assert!(matches!(bearings[0].kind, GuideKind::Apex { .. }));
        assert!(
            game.guide_bearings(game.focus, Vec2::new(300.0, 200.0))
                .iter()
                .all(|b| !matches!(b.kind, GuideKind::Apex { .. }))
        );
    }

    #[test]
    fn a_slain_apex_drops_a_hoard_is_recorded_by_name_and_never_returns() {
        let id = find(SEED, Rank::Major);
        let name = crate::apex::name(SEED, id).to_uppercase();
        let mut game = visit(id);
        let (body_id, key) = {
            let b = the_apex(&game);
            (b.id, b.origin.unwrap())
        };
        game.pickups.clear();
        game.notices.clear();
        game.bodies
            .iter_mut()
            .find(|b| b.id == body_id)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.run.apex_slain, vec![name.clone()]);
        assert!(
            game.notices
                .iter()
                .any(|n| n.text == format!("APEX SLAIN: {name}"))
        );
        assert!(
            game.notices.iter().all(|n| !n.text.contains("extirpated")),
            "an apex is no species"
        );
        let parts: Vec<_> = game
            .pickups
            .iter()
            .filter_map(|p| match &p.item {
                Item::Part(part) => Some(part),
                _ => None,
            })
            .collect();
        assert!(parts.len() >= 3);
        let epic = parts
            .iter()
            .find(|p| p.rarity == Rarity::Epic)
            .expect("an epic token");
        assert_eq!(epic.slot, Slot::Plating, "parry is still locked");
        let metal: f32 = game
            .pickups
            .iter()
            .filter_map(|p| match p.item {
                Item::Material(Material::Metal, n) => Some(n),
                _ => None,
            })
            .sum();
        assert!(metal >= t::APEX_MATERIAL);
        assert!(
            game.run_report()
                .lines
                .iter()
                .any(|l| l == &format!("APEX SLAIN: {name}"))
        );
        // Gone for good: leave, come back, nothing of it generated into the world.
        assert!(game.fallen[&key.0].contains(&key.1));
        game.teleport(Vec2::ZERO);
        for _ in 0..5 {
            game.step(DT, Input::default());
        }
        game.teleport(apex_spawn(SEED, id).position);
        for _ in 0..5 {
            game.step(DT, Input::default());
        }
        assert!(game.bodies.iter().all(|b| game.apex_of(b).is_none()));
        assert!(game.apex_report().is_none());
    }

    #[test]
    fn the_token_follows_the_unlock_that_is_still_missing() {
        let id = find(SEED, Rank::Major);
        let mut game = visit(id);
        game.loadout.skills.raise(skills::Skill::Parry);
        game.pickups.clear();
        let body_id = the_apex(&game).id;
        game.bodies
            .iter_mut()
            .find(|b| b.id == body_id)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        let epic = game
            .pickups
            .iter()
            .find_map(|p| match &p.item {
                Item::Part(part) if part.rarity == Rarity::Epic => Some(part.slot),
                _ => None,
            })
            .expect("an epic");
        assert_eq!(epic, Slot::Engine, "parry is owned, dash is next");
    }

    #[test]
    fn a_long_stay_beside_an_apex_keeps_the_population_bounded() {
        let id = find(SEED, Rank::Major);
        let mut game = visit(id);
        for _ in 0..(120.0 / 0.05) as usize {
            let at = apex_spawn(SEED, id).position + Vec2::new(0.0, 900.0);
            if let Some(p) = game.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                p.position = at;
                p.velocity = Vec2::ZERO;
            }
            game.step(0.05, Input::default());
        }
        assert!(game.bodies.len() + game.food.len() + game.eggs.len() < MAX_BODIES);
        let apexes = game
            .bodies
            .iter()
            .filter(|b| game.apex_of(b).is_some())
            .count();
        assert!(apexes <= 1, "one apex, found {apexes}");
        for q in &game.active {
            let creatures = game
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Creature && SectorId::containing(b.position) == *q)
                .count();
            assert!(creatures <= 2 * world::SECTOR_BODY_BUDGET as usize);
        }
    }
}
