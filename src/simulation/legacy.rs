//! Insurance and legacy. When the last ship is lost, the run leaves two things to the next one.
//!
//! - **The legacy stash** is carried at once: a share of what the run mined, per material and
//!   capped (`tuning`: 25 percent up to 120 with insurance on, 10 percent up to 40 with it off),
//!   and, with insurance on, the best weapon profile owned at up to level 2. So later runs start
//!   a little faster, never with the late game handed over.
//! - **The wreck** is left at the place of death, holding what the hold still carried (up to a
//!   cap per material) and the best part. Flying within `WRECK_RADIUS` of it in a later run
//!   recovers it, as far as the hold has room. A wreck inside a living civilization's territory
//!   is looted after a delay fixed by a hash of the seed, sector and wreck, counted in play time
//!   across runs. At most three wrecks wait; the oldest is lost.
//!
//! Persistence is in memory like the rest of the game: `Game::reset` after a lost run carries a
//! `Legacy` into the new `Game`; restarting mid-run keeps the wrecks but earns no new legacy.

use super::arsenal::{Gain, Profile};
use super::tuning as t;
use super::upgrades::{Item, Part, Rarity};
use super::*;
use crate::territory::Standing;
use crate::world::hash2;

const LOOT_SALT: u64 = 0x100D_0000_0000_0B1D;

/// A lost ship's remains.
#[derive(Clone, Debug, PartialEq)]
pub struct Wreck {
    pub id: u32,
    pub position: Vec2,
    pub cargo: Cargo,
    pub part: Option<Part>,
    /// Seconds of play it has lain there, across runs.
    pub age: f32,
}

/// What a run leaves behind and what the current run was given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Legacy {
    /// Materials carried into this run, and the weapon.
    pub carried: Cargo,
    pub weapon: Option<(Profile, u8)>,
    pub wrecks: Vec<Wreck>,
    /// How many runs came before this one.
    pub generation: u32,
    next_wreck: u32,
}

impl Legacy {
    pub fn is_empty(&self) -> bool {
        self.weapon.is_none()
            && self.carried.metal + self.carried.volatiles + self.carried.crystal < 0.5
    }
}

/// What the finished run will pass on, shown on the summary.
#[derive(Clone, Debug, PartialEq)]
pub struct Bequest {
    pub carried: Cargo,
    pub weapon: Option<(Profile, u8)>,
    pub wreck: Option<Wreck>,
    pub insured: bool,
}

/// Share, cap and weapon rule for the insurance setting, for the toggle's notice.
pub fn terms(insured: bool) -> (f32, f32, u8) {
    if insured {
        (
            t::LEGACY_FRACTION,
            t::LEGACY_CAP,
            t::LEGACY_WEAPON_LEVEL_CAP,
        )
    } else {
        (t::LEGACY_FRACTION_BARE, t::LEGACY_CAP_BARE, 0)
    }
}

/// Seconds a wreck lies before the civilization holding its sector loots it.
pub fn loot_time(seed: u64, sector: SectorId, id: u32) -> f32 {
    let h = hash2(seed ^ LOOT_SALT ^ u64::from(id), sector.x, sector.y);
    t::LOOT_AFTER + t::LOOT_JITTER * ((h % 10_000) as f32 / 10_000.0)
}

impl Game {
    /// What this run would pass on if it ended now (and the wreck it would leave).
    pub fn bequest(&self, position: Vec2, part: Option<Part>) -> Bequest {
        let insured = self.is_insured();
        let (share, cap, weapon_cap) = terms(insured);
        let mut carried = Cargo::default();
        for (k, kind) in Material::ALL.into_iter().enumerate() {
            carried.add(kind, (self.run.mined[k] * share).min(cap).floor());
        }
        let weapon = (weapon_cap > 0)
            .then(|| {
                self.loadout
                    .arsenal
                    .owned()
                    .into_iter()
                    .filter(|&p| p != Profile::Stock)
                    .max_by_key(|&p| (self.loadout.arsenal.level(p), std::cmp::Reverse(p.index())))
                    .map(|p| (p, self.loadout.arsenal.level(p).min(weapon_cap)))
            })
            .flatten();
        let mut held = Cargo::default();
        for kind in Material::ALL {
            held.add(kind, self.cargo.amount(kind).min(t::WRECK_CAP).floor());
        }
        let has = held.metal + held.volatiles + held.crystal >= 0.5 || part.is_some();
        let wreck = has.then_some(Wreck {
            id: 0,
            position,
            cargo: held,
            part,
            age: 0.0,
        });
        Bequest {
            carried,
            weapon,
            wreck,
            insured,
        }
    }

    /// Called when the last ship is lost: seals what the run leaves.
    pub fn seal_bequest(&mut self, position: Vec2, part: Option<Part>) {
        self.bequest = Some(self.bequest(position, part));
    }

    /// The finished run's bequest, once it is over.
    pub fn pending_bequest(&self) -> Option<&Bequest> {
        self.bequest.as_ref()
    }

    /// The new game after this one: this seed, with the legacy of a lost run (if it was lost)
    /// and every wreck still waiting.
    pub fn next_run(&self) -> Game {
        let mut next = Game::new(self.seed);
        let mut legacy = Legacy {
            generation: self.legacy.generation,
            wrecks: self.legacy.wrecks.clone(),
            next_wreck: self.legacy.next_wreck,
            ..Legacy::default()
        };
        if let Some(bequest) = &self.bequest {
            legacy.generation += 1;
            legacy.carried = bequest.carried;
            legacy.weapon = bequest.weapon;
            if let Some(mut wreck) = bequest.wreck.clone() {
                legacy.next_wreck += 1;
                wreck.id = legacy.next_wreck;
                legacy.wrecks.push(wreck);
                while legacy.wrecks.len() > t::MAX_WRECKS {
                    legacy.wrecks.remove(0);
                }
            }
        }
        next.begin_with(legacy);
        next
    }

    /// Starts this (fresh) game with a legacy: materials in the hold, the weapon armed.
    fn begin_with(&mut self, legacy: Legacy) {
        for kind in Material::ALL {
            self.cargo.add(kind, legacy.carried.amount(kind));
        }
        if let Some((profile, level)) = legacy.weapon {
            let gain = self.loadout.arsenal.acquire(profile, level);
            if matches!(gain, Gain::New(_)) {
                self.run.weapons += 1;
            }
            self.refresh_stats();
        }
        self.legacy = legacy;
        if !self.legacy.is_empty() {
            self.notify(self.legacy_line(), Rarity::Rare);
        }
    }

    /// A short statement of what the legacy brought, for the HUD at run start.
    pub fn legacy_line(&self) -> String {
        let c = &self.legacy.carried;
        let mut parts = vec![format!(
            "{:.0}M {:.0}V {:.0}C",
            c.metal, c.volatiles, c.crystal
        )];
        if let Some((profile, level)) = self.legacy.weapon {
            parts.push(format!("{} {level}", profile.label()));
        }
        format!("LEGACY  {}", parts.join("  "))
    }

    /// The legacy line while a run is young, else None.
    pub fn legacy_hud(&self) -> Option<String> {
        (self.time < t::LEGACY_HUD_SECONDS && !self.legacy.is_empty()).then(|| self.legacy_line())
    }

    pub fn legacy(&self) -> &Legacy {
        &self.legacy
    }

    pub fn wrecks(&self) -> &[Wreck] {
        &self.legacy.wrecks
    }

    pub(super) fn legacy_wreck_sectors(&self) -> Vec<SectorId> {
        self.legacy
            .wrecks
            .iter()
            .map(|w| SectorId::containing(w.position))
            .collect()
    }

    /// Wrecks age, are looted by a civilization that holds their sector, and are recovered
    /// by flying over them.
    pub(super) fn update_legacy(&mut self, dt: f32) {
        if self.legacy.wrecks.is_empty() {
            return;
        }
        let seed = self.seed;
        let mut gone: Vec<u32> = Vec::new();
        for i in 0..self.legacy.wrecks.len() {
            self.legacy.wrecks[i].age += dt;
            let (id, position, age) = {
                let w = &self.legacy.wrecks[i];
                (w.id, w.position, w.age)
            };
            let sector = SectorId::containing(position);
            let rival = world::territory(seed, sector)
                .filter(|t| self.civ_standing(t.id) != Standing::Fallen);
            if rival.is_some() && age >= loot_time(seed, sector, id) {
                gone.push(id);
                self.notify("A RIVAL LOOTED YOUR WRECK".into(), Rarity::Epic);
            }
        }
        self.legacy.wrecks.retain(|w| !gone.contains(&w.id));
        let Some(ship) = self.player().map(|p| p.position) else {
            return;
        };
        let Some(at) = self
            .legacy
            .wrecks
            .iter()
            .position(|w| w.position.distance(ship) <= t::WRECK_RADIUS)
        else {
            return;
        };
        let mut wreck = self.legacy.wrecks[at].clone();
        let mut got = Vec::new();
        for kind in Material::ALL {
            let moved = self.cargo.add(kind, wreck.cargo.amount(kind));
            wreck.cargo.take(kind, moved);
            if moved >= 0.5 {
                got.push(format!("{moved:.0}{}", kind.letter()));
            }
        }
        let part = wreck.part.take();
        let empty = wreck.cargo.metal + wreck.cargo.volatiles + wreck.cargo.crystal < 0.5;
        if let Some(part) = &part {
            got.push(part.name.to_uppercase());
        }
        if got.is_empty() {
            return;
        }
        self.cue(Cue::Pickup {
            rarity: Rarity::Epic,
        });
        if empty {
            self.legacy.wrecks.remove(at);
            self.notify(format!("WRECK RECOVERED  {}", got.join(" ")), Rarity::Epic);
        } else {
            self.legacy.wrecks[at] = wreck;
            self.notify(
                format!("WRECK PART RECOVERED  {}  hold full", got.join(" ")),
                Rarity::Epic,
            );
        }
        if let Some(part) = part {
            self.collect(Item::Part(part));
        }
    }

    /// Lines for the summary panel: what this run passes on.
    pub fn legacy_report(&self) -> Vec<String> {
        let Some(b) = &self.bequest else {
            return Vec::new();
        };
        let (share, cap, _) = terms(b.insured);
        let mut out = vec![format!(
            "({})  carries {:.0}M {:.0}V {:.0}C   ({:.0}% of ore mined, up to {:.0} each){}",
            if b.insured { "INSURED" } else { "UNINSURED" },
            b.carried.metal,
            b.carried.volatiles,
            b.carried.crystal,
            share * 100.0,
            cap,
            match b.weapon {
                Some((p, l)) => format!("   weapon {} {l}", p.label()),
                None => String::new(),
            }
        )];
        if let Some(w) = &b.wreck {
            let sector = SectorId::containing(w.position);
            let rival = if world::territory(self.seed, sector).is_some() {
                "   a rival may loot it"
            } else {
                ""
            };
            out.push(format!(
                "WRECK at sector ({}, {})  holds {:.0}M {:.0}V {:.0}C{}{}",
                sector.x,
                sector.y,
                w.cargo.metal,
                w.cargo.volatiles,
                w.cargo.crystal,
                w.part.as_ref().map_or(String::new(), |p| format!(
                    "  and {}",
                    p.name.to_uppercase()
                )),
                rival
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{DT, empty_game};
    use super::*;

    fn lose_the_ship(game: &mut Game) {
        game.lives = 1;
        game.player_invulnerability = 0.0;
        if let Some(ship) = game.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.health = 0.0;
        }
        game.step(DT, Input::default());
        assert!(game.game_over, "the last ship is lost");
    }

    fn test_part() -> Part {
        use super::super::upgrades::{Effect, Slot, Stat};
        Part {
            name: "Test Plate".into(),
            slot: Slot::Plating,
            rarity: Rarity::Rare,
            grade: 1.0,
            stem: String::new(),
            core: usize::MAX,
            effects: vec![Effect::Stat(Stat::Hull, 0.6)],
        }
    }

    fn mined(game: &mut Game, metal: f32, volatiles: f32, crystal: f32) {
        game.run.mined = [metal, volatiles, crystal];
    }

    fn some_weapon() -> Profile {
        Profile::ALL
            .into_iter()
            .find(|&p| p != Profile::Stock)
            .unwrap()
    }

    fn territory_sector(seed: u64) -> SectorId {
        for ring in 6..40i32 {
            for x in -ring..=ring {
                for y in [-ring, ring] {
                    for id in [SectorId { x, y }, SectorId { x: y, y: x }] {
                        if world::territory(seed, id).is_some() {
                            return id;
                        }
                    }
                }
            }
        }
        panic!("no territory found");
    }

    #[test]
    fn a_share_of_the_ore_is_carried_up_to_a_cap_and_insurance_widens_it() {
        let mut game = empty_game();
        mined(&mut game, 1000.0, 100.0, 0.0);
        let b = game.bequest(Vec2::ZERO, None);
        assert!(b.insured);
        assert_eq!(b.carried.metal, t::LEGACY_CAP, "capped");
        assert_eq!(b.carried.volatiles, 25.0, "a quarter");
        assert_eq!(b.carried.crystal, 0.0);
        game.toggle_insurance();
        let b = game.bequest(Vec2::ZERO, None);
        assert!(!b.insured);
        assert_eq!(b.carried.metal, t::LEGACY_CAP_BARE);
        assert_eq!(b.carried.volatiles, 10.0);
        assert!(b.carried.metal < 120.0 && b.carried.volatiles < 25.0);
    }

    #[test]
    fn one_weapon_level_is_carried_only_with_insurance_and_never_the_stock_gun() {
        let mut game = empty_game();
        assert_eq!(game.bequest(Vec2::ZERO, None).weapon, None, "nothing owned");
        let weapon = some_weapon();
        game.loadout.arsenal.acquire(weapon, weapon.max_level());
        let b = game.bequest(Vec2::ZERO, None);
        assert_eq!(
            b.weapon,
            Some((weapon, t::LEGACY_WEAPON_LEVEL_CAP.min(weapon.max_level())))
        );
        game.toggle_insurance();
        assert_eq!(game.bequest(Vec2::ZERO, None).weapon, None);
    }

    #[test]
    fn the_next_run_starts_with_the_legacy_and_nothing_else_of_the_old_one() {
        let mut game = empty_game();
        mined(&mut game, 200.0, 80.0, 40.0);
        let weapon = some_weapon();
        game.loadout.arsenal.acquire(weapon, 3);
        game.score = 999;
        lose_the_ship(&mut game);
        assert!(game.pending_bequest().is_some());
        assert!(!game.legacy_report().is_empty());
        game.reset();
        assert!(!game.game_over);
        assert_eq!(game.score, 0);
        assert_eq!(game.legacy().generation, 1);
        assert_eq!(game.cargo.metal, 50.0);
        assert_eq!(game.cargo.volatiles, 20.0);
        assert_eq!(game.cargo.crystal, 10.0);
        assert_eq!(game.loadout.arsenal.level(weapon), 2, "capped at level two");
        assert!(game.legacy_hud().is_some());
        assert_eq!(
            game.run.total_mined(),
            0.0,
            "the new run starts counting afresh"
        );
        assert!(game.loadout.parts.is_empty(), "parts do not carry");
    }

    #[test]
    fn restarting_midway_keeps_wrecks_but_earns_no_legacy() {
        let mut game = empty_game();
        mined(&mut game, 500.0, 500.0, 500.0);
        game.reset();
        assert!(game.legacy().is_empty());
        assert_eq!(game.cargo.metal, 0.0);
        let mut game = empty_game();
        lose_the_ship(&mut game);
        game.cargo = Cargo::default();
        game.reset();
        let wrecks = game.wrecks().len();
        game.reset();
        assert_eq!(
            game.wrecks().len(),
            wrecks,
            "wrecks survive a plain restart"
        );
    }

    fn dead_with(hold: f32, at: Vec2) -> Game {
        let mut game = empty_game();
        game.teleport(at);
        game.cargo = Cargo {
            metal: hold,
            volatiles: hold / 2.0,
            crystal: 0.0,
            ..Default::default()
        };
        game.collect(Item::Part(test_part()));
        lose_the_ship(&mut game);
        game
    }

    #[test]
    fn a_wreck_is_left_where_the_ship_died_with_the_hold_and_the_best_part() {
        let at = Vec2::new(1500.0, 800.0);
        let game = dead_with(100.0, at);
        let b = game.pending_bequest().unwrap();
        let wreck = b.wreck.as_ref().expect("a wreck");
        assert!(wreck.position.distance(at) < 1.0);
        assert!(wreck.part.is_some(), "the best part rides in the wreck");
        assert!(wreck.cargo.metal > 0.0 && wreck.cargo.metal <= t::WRECK_CAP);
        let mut next = game.next_run();
        assert_eq!(next.wrecks().len(), 1);
        assert_eq!(next.wrecks()[0].position, wreck.position);
        assert!(next.chart_entries().iter().any(|e| e.wreck));
        // Not yet recovered from afar.
        next.step(DT, Input::default());
        assert_eq!(next.wrecks().len(), 1);
    }

    #[test]
    fn flying_over_the_wreck_restores_it_and_a_full_hold_leaves_the_rest() {
        let at = Vec2::new(1500.0, 800.0);
        let game = dead_with(100.0, at);
        let mut next = game.next_run();
        let held = next.wrecks()[0].cargo;
        next.teleport(at + Vec2::new(t::WRECK_RADIUS - 10.0, 0.0));
        next.step(DT, Input::default());
        assert!(next.wrecks().is_empty(), "recovered");
        assert_eq!(next.cargo.metal, held.metal);
        assert_eq!(next.loadout.parts.len(), 1, "the part is back");
        // With a nearly full hold only part of it fits and the wreck stays.
        let mut next = game.next_run();
        let room = next.cargo.cap(Material::Metal);
        next.cargo.metal = room - 10.0;
        next.teleport(at);
        next.step(DT, Input::default());
        assert_eq!(next.wrecks().len(), 1);
        assert!((next.wrecks()[0].cargo.metal - (held.metal - 10.0)).abs() < 0.5);
        assert!(next.wrecks()[0].part.is_none(), "the part always comes");
    }

    #[test]
    fn only_three_wrecks_wait_and_the_oldest_is_lost() {
        let mut game = dead_with(50.0, Vec2::new(100.0, 0.0));
        for k in 1..=3 {
            game = game.next_run();
            game.teleport(Vec2::new(100.0 + 600.0 * k as f32, 0.0));
            game.collect(Item::Part(test_part()));
            game.cargo.metal = 10.0;
            lose_the_ship(&mut game);
        }
        let next = game.next_run();
        assert_eq!(next.wrecks().len(), t::MAX_WRECKS);
        assert!(
            next.wrecks().iter().all(|w| w.position.x > 150.0),
            "the first is gone"
        );
        let ids: Vec<u32> = next.wrecks().iter().map(|w| w.id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_rival_loots_a_wreck_in_its_territory_after_a_fixed_delay_and_only_there() {
        let seed = 42;
        let id = territory_sector(seed);
        let time = loot_time(seed, id, 1);
        assert!((t::LOOT_AFTER..t::LOOT_AFTER + t::LOOT_JITTER).contains(&time));
        assert_eq!(time, loot_time(seed, id, 1), "deterministic");
        let wreck = |at: Vec2| Wreck {
            id: 1,
            position: at,
            cargo: Cargo {
                metal: 50.0,
                ..Default::default()
            },
            part: None,
            age: 0.0,
        };
        // In a territory.
        let mut game = empty_game();
        game.legacy.wrecks.push(wreck(id.center()));
        game.legacy.wrecks[0].age = time - 1.0;
        for _ in 0..(2.0 / DT) as usize {
            game.step(DT, Input::default());
        }
        assert!(game.wrecks().is_empty(), "looted");
        // Out in the quiet, however long it waits.
        let mut game = empty_game();
        game.legacy.wrecks.push(wreck(Vec2::new(3000.0, 0.0)));
        game.legacy.wrecks[0].age = 1.0e6;
        game.step(DT, Input::default());
        assert_eq!(game.wrecks().len(), 1);
    }

    #[test]
    fn a_fallen_civilization_no_longer_loots() {
        let seed = 42;
        let id = territory_sector(seed);
        let tid = world::territory(seed, id).unwrap().id;
        let mut game = empty_game();
        game.civ_territories
            .insert(tid, world::territory(seed, id).unwrap());
        game.civ_fall.insert(
            tid,
            crate::territory::Fall {
                capital: true,
                elder: true,
            },
        );
        assert_eq!(game.civ_standing(tid), Standing::Fallen);
        game.legacy.wrecks.push(Wreck {
            id: 1,
            position: id.center(),
            cargo: Cargo::default(),
            part: None,
            age: 1.0e6,
        });
        game.step(DT, Input::default());
        assert_eq!(game.wrecks().len(), 1);
    }

    #[test]
    fn legacy_is_deterministic() {
        let go = || {
            let mut game = empty_game();
            mined(&mut game, 300.0, 90.0, 10.0);
            lose_the_ship(&mut game);
            let next = game.next_run();
            (next.cargo, next.legacy().clone())
        };
        assert_eq!(go(), go());
    }
}
