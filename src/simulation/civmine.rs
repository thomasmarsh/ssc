//! Civilization mining: rank-and-file members of a territory work rocks inside it by the same
//! ore rules as the ship's beam (shrink to the floor, crumble, remembered as fallen, spent ore
//! in the shared `mined` map, so a civilization can drain a planetoid before the player
//! arrives) and deposit into the territory's stash, capped at `STOCK_CAP`.
//!
//! - **Who and what.** At most `MAX_MINERS` per territory, one rock each, never the same rock
//!   twice. A miner is the lowest-spawn-index free member (generated, not bred) and its rock
//!   one of the three lowest-index minable rocks in reach, chosen with `civ_rng`, so a run is
//!   deterministic. Crystal (it bursts) and husks (they hatch) are left alone, and so is any
//!   rock the ship is working.
//! - **Flee or escort.** A miner that has the ship within `FLEE_RANGE` and no warrior or
//!   elder near drops what it is doing and runs; with an escort it keeps working.
//! - **Budget.** A territory works at most `ORE_BUDGET` (plus a bit for strength) of ore in a
//!   session, so a civilization cannot wipe a sector's rocks; the budget is spent from planetoids
//!   and rocks alike.
//! - **The stash.** The capital base draws on it (`FEED_RATE` a second) only while it can
//!   build a guardian, and while mining is active the passive trickle is off. Surplus piles up:
//!   a visible cache marker at the capital, and if the capital falls, half of it drops as
//!   pickups to loot.

use super::mining::{Material, rate};
use super::upgrades::Item;
use super::*;
use crate::territory::{CivRole, Standing};
use std::collections::BTreeSet;

/// The most the stash holds, all materials together.
pub const STOCK_CAP: f32 = 150.0;
pub const MAX_MINERS: usize = 2;
/// Ore a territory may work in one session: this, plus `ORE_PER_STRENGTH` times its strength.
pub const ORE_BUDGET: f32 = 450.0;
const ORE_PER_STRENGTH: f32 = 200.0;
/// A miner works within this of a rock's surface, and looks this far for a rock.
const WORK_RANGE: f32 = 190.0;
const SEARCH: f32 = 2600.0;
/// The ship this close sends an unescorted miner running; an escort within `ESCORT_RANGE` of
/// the miner keeps it working.
pub const FLEE_RANGE: f32 = 900.0;
const ESCORT_RANGE: f32 = 520.0;
/// The ore a miner works per second relative to the beam (the stash is a civilization's, with
/// many hands).
const YIELD: f32 = 1.5;
/// Units a capital base takes from the stash per second while it can build.
pub const FEED_RATE: f32 = 1.4;
/// Share of the stash a fallen capital spills as pickups.
pub const SPILL: f32 = 0.5;
/// Seconds before another miner is looked for after a failed search, and a fleeing miner's rest.
const RETRY: f32 = 5.0;
const REST: f32 = 6.0;
/// How long a miner may spend reaching a rock (walls and crowds can trap it).
const PATIENCE: f32 = 25.0;
/// The stash shows a marker above this much.
pub const MARK_AT: f32 = 8.0;

#[derive(Clone, Debug)]
pub struct Miner {
    pub body: u64,
    pub rock: u64,
    /// Seconds left running from the ship.
    pub fled: f32,
    pub working: bool,
    /// Seconds of travelling left before a miner that is not getting anywhere gives up.
    pub patience: f32,
    /// Where the beam ends on the rock while working.
    pub end: Vec2,
}

/// One territory's mining state.
#[derive(Clone, Debug, Default)]
pub struct Mining {
    pub miners: Vec<Miner>,
    /// Metal, volatiles, crystal.
    pub stock: [f32; 3],
    /// Ore worked this session, against the budget.
    pub spent: f32,
    retry: f32,
    /// Whether mining is on: a miner is assigned or the stash holds something.
    pub active: bool,
}

impl Mining {
    pub fn total(&self) -> f32 {
        self.stock.iter().sum()
    }

    fn room(&self) -> f32 {
        (STOCK_CAP - self.total()).max(0.0)
    }

    /// Takes up to `amount` from the richest material, returning what came out.
    pub fn withdraw(&mut self, amount: f32) -> f32 {
        let mut taken = 0.0;
        while taken < amount - 1e-4 {
            let Some((slot, have)) = self
                .stock
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, v)| *v > 1e-4)
                .max_by(|a, b| a.1.total_cmp(&b.1))
            else {
                break;
            };
            let part = (amount - taken).min(have);
            self.stock[slot] -= part;
            taken += part;
        }
        taken
    }
}

/// A stash marker for the renderer: where, how full, the territory's tint and the mix of
/// materials in it.
#[derive(Clone, Copy, Debug)]
pub struct Cache {
    pub at: Vec2,
    pub fill: f32,
    pub tint: [f32; 3],
    pub mix: [f32; 3],
}

impl Game {
    pub fn ore_budget(&self, territory: u64) -> f32 {
        ORE_BUDGET
            + ORE_PER_STRENGTH
                * self
                    .civ_territories
                    .get(&territory)
                    .map_or(1.0, |t| t.strength)
    }

    /// The stash a territory has built up.
    pub fn civ_stock(&self, territory: u64) -> f32 {
        self.civ_mining.get(&territory).map_or(0.0, Mining::total)
    }

    /// Whether mining is on for a territory (its capital base takes no passive trickle).
    pub fn civ_mining_active(&self, territory: u64) -> bool {
        self.civ_mining.get(&territory).is_some_and(|m| m.active)
    }

    /// Beams of working miners: where they start, where they end, the territory's tint.
    pub fn miner_beams(&self) -> Vec<(Vec2, Vec2, [f32; 3])> {
        let mut out = Vec::new();
        for (tid, mining) in &self.civ_mining {
            let tint = self.civ_colors.get(tid).copied().unwrap_or([1.0; 3]);
            for miner in mining.miners.iter().filter(|m| m.working) {
                if let Some(body) = self.body(miner.body) {
                    out.push((body.position, miner.end, tint));
                }
            }
        }
        out
    }

    /// Stash markers of loaded capitals with something in them.
    pub fn caches(&self) -> Vec<Cache> {
        let mut out = Vec::new();
        for body in self.bodies.iter().filter(|b| b.base.is_some()) {
            let Some((tid, CivRole::Capital)) =
                body.origin.and_then(|o| self.civ_bases.get(&o)).copied()
            else {
                continue;
            };
            let Some(mining) = self.civ_mining.get(&tid) else {
                continue;
            };
            let total = mining.total();
            if total < MARK_AT {
                continue;
            }
            let mix = mining.stock.map(|v| v / total);
            out.push(Cache {
                at: body.position + Vec2::new(body.radius * 1.5, -body.radius * 1.5),
                fill: (total / STOCK_CAP).clamp(0.0, 1.0),
                tint: self.civ_colors.get(&tid).copied().unwrap_or([1.0; 3]),
                mix,
            });
        }
        out
    }

    /// The capital fell: half the stash spills as pickups and the rest is lost.
    pub(super) fn spill_cache(&mut self, territory: u64, at: Vec2) {
        let Some(mining) = self.civ_mining.get_mut(&territory) else {
            return;
        };
        let stock = std::mem::take(&mut mining.stock);
        mining.active = false;
        let mut spilled = 0.0;
        for (k, kind) in Material::ALL.into_iter().enumerate() {
            let amount = (stock[k] * SPILL).floor();
            if amount >= 1.0 {
                spilled += amount;
                let drift = Vec2::from_angle(2.1 * k as f32 + 0.4) * 55.0;
                self.drop_item(at, drift, Item::Material(kind, amount));
            }
        }
        if spilled >= 1.0 {
            self.notify(
                format!("CACHE SPILLED  {spilled:.0} materials"),
                upgrades::Rarity::Rare,
            );
        }
    }

    /// Per tick, after steering: assigns miners, moves them, works rocks, feeds the stash.
    pub(super) fn update_civ_mining(&mut self, dt: f32) {
        let ship = self.player().map(|p| p.position);
        // Territories with a loaded rank-and-file member, or work already under way.
        let mut tids: BTreeSet<u64> = self.civ_mining.keys().copied().collect();
        for body in self.bodies.iter().filter(|b| b.active && !b.follower) {
            if let Some((tid, CivRole::Member)) = self.civ_of(body) {
                tids.insert(tid);
            }
        }
        for tid in tids {
            self.mine_for(tid, ship, dt);
        }
    }

    fn mine_for(&mut self, tid: u64, ship: Option<Vec2>, dt: f32) {
        let fallen = self.civ_standing(tid) == Standing::Fallen;
        let budget = self.ore_budget(tid);
        let mut mining = self.civ_mining.remove(&tid).unwrap_or_default();
        mining.retry = (mining.retry - dt).max(0.0);
        if fallen {
            mining.miners.clear();
            mining.active = false;
            self.civ_mining.insert(tid, mining);
            return;
        }
        let seed = self.seed;
        // Keep only miners whose body and rock are still good.
        mining.miners.retain(|m| {
            let body_ok = self.body(m.body).is_some_and(|b| {
                b.active && b.health > b.max_health * 0.6 && b.since_hit > 2.5 && !b.consumed
            });
            let rock_ok = self.body(m.rock).is_some_and(|r| r.minable());
            body_ok && rock_ok
        });
        let escorts: Vec<Vec2> = self
            .bodies
            .iter()
            .filter(|b| b.active && !b.follower)
            .filter(|b| {
                matches!(self.civ_of(b), Some((t, CivRole::Warrior | CivRole::Elder)) if t == tid)
            })
            .map(|b| b.position)
            .collect();
        let escorted = |at: Vec2| escorts.iter().any(|e| e.distance(at) < ESCORT_RANGE);
        let hunted = |at: Vec2| ship.is_some_and(|s| s.distance(at) < FLEE_RANGE);

        // A new miner, if there is a slot, ore left to work, room in the stash and quiet.
        if mining.miners.len() < MAX_MINERS
            && mining.retry <= 0.0
            && mining.spent < budget
            && mining.room() > 1.0
        {
            self.assign_miner(tid, &mut mining, &escorted, &hunted, seed);
        }

        let mut ended = Vec::new();
        for slot in 0..mining.miners.len() {
            let (body_id, rock_id) = (mining.miners[slot].body, mining.miners[slot].rock);
            let (Some(bi), Some(ri)) = (
                self.bodies.iter().position(|b| b.id == body_id),
                self.bodies.iter().position(|b| b.id == rock_id),
            ) else {
                continue;
            };
            let (at, speed) = (self.bodies[bi].position, self.bodies[bi].genome.speed);
            mining.miners[slot].fled = (mining.miners[slot].fled - dt).max(0.0);
            mining.miners[slot].working = false;
            if hunted(at) && !escorted(at) {
                // Run from the ship; resume once it is gone.
                mining.miners[slot].fled = REST;
                let away = ship.map_or(Vec2::X, |s| (at - s).normalize_or_zero());
                self.bodies[bi].velocity = away * speed * 0.9;
                continue;
            }
            if mining.miners[slot].fled > 0.0 {
                continue;
            }
            let rock = &self.bodies[ri];
            let offset = rock.position - at;
            let gap = offset.length() - rock.radius;
            if gap > WORK_RANGE {
                mining.miners[slot].patience -= dt;
                if mining.miners[slot].patience <= 0.0 {
                    ended.push(slot);
                    mining.retry = RETRY;
                }
                self.bodies[bi].velocity = offset.normalize_or_zero() * speed * 0.6;
                continue;
            }
            // In range: hold still and work the rock.
            let (kind, material, ore, radius) =
                (rock.rock, rock.material(seed), rock.ore(), rock.radius);
            let end = rock.position - offset.normalize_or_zero() * radius;
            self.bodies[bi].velocity *= 0.8;
            let want = rate(kind) * YIELD * dt;
            let left = (budget - mining.spent).max(0.0);
            let mined = want.min(ore).min(mining.room()).min(left);
            if mined <= 1e-6 {
                if left <= 1e-6 || mining.room() <= 1e-6 {
                    ended.push(slot);
                }
                continue;
            }
            mining.stock[material as usize] += mined;
            mining.spent += mined;
            mining.miners[slot].working = true;
            mining.miners[slot].patience = PATIENCE;
            mining.miners[slot].end = end;
            if let Some(leftover) = self.drain_rock(ri, mined) {
                let room = mining.room();
                mining.stock[material as usize] += leftover.min(room);
                mining.miners[slot].working = false;
                ended.push(slot);
            }
        }
        for slot in ended.into_iter().rev() {
            mining.miners.remove(slot);
        }
        mining.active = !mining.miners.is_empty() || mining.total() > 1e-3;
        self.civ_mining.insert(tid, mining);
    }

    /// Picks the lowest-index free member and one of the three lowest-index rocks in reach.
    fn assign_miner(
        &mut self,
        tid: u64,
        mining: &mut Mining,
        escorted: &dyn Fn(Vec2) -> bool,
        hunted: &dyn Fn(Vec2) -> bool,
        seed: u64,
    ) {
        let busy: Vec<u64> = mining.miners.iter().map(|m| m.body).collect();
        // Rocks any civilization is working, and the one the ship's beam holds.
        let taken: Vec<u64> = self
            .civ_mining
            .values()
            .flat_map(|m| m.miners.iter().map(|x| x.rock))
            .chain(mining.miners.iter().map(|m| m.rock))
            .chain(self.mine_target)
            .collect();
        let mut candidates: Vec<(u32, i32, i32, u64)> = self
            .bodies
            .iter()
            .filter(|b| {
                b.active
                    && !b.follower
                    && !b.alert
                    && !b.enraged
                    && b.origin.is_some()
                    && !busy.contains(&b.id)
                    && matches!(self.civ_of(b), Some((t, CivRole::Member)) if t == tid)
                    && b.health > b.max_health * 0.8
                    && (!hunted(b.position) || escorted(b.position))
            })
            .filter_map(|b| b.origin.map(|(q, i)| (i, q.x, q.y, b.id)))
            .collect();
        candidates.sort_unstable();
        let mut territory_of: HashMap<SectorId, Option<u64>> = HashMap::new();
        for &(_, _, _, member) in candidates.iter().take(12) {
            let Some(at) = self.body(member).map(|b| b.position) else {
                continue;
            };
            let mut rocks: Vec<(u32, i32, i32, u64)> = self
                .bodies
                .iter()
                .filter(|r| {
                    r.minable()
                        && r.active
                        && matches!(
                            r.rock,
                            RockKind::Plain | RockKind::Ore | RockKind::Ice | RockKind::Planetoid
                        )
                        && !taken.contains(&r.id)
                        && r.origin.is_some()
                        && r.position.distance(at) - r.radius < SEARCH
                        && self.clear_shot(at, r.position)
                })
                .filter(|r| {
                    let q = SectorId::containing(r.position);
                    *territory_of
                        .entry(q)
                        .or_insert_with(|| world::territory(seed, q).map(|t| t.id))
                        == Some(tid)
                })
                .filter_map(|r| r.origin.map(|(q, i)| (i, q.x, q.y, r.id)))
                .collect();
            if rocks.is_empty() {
                continue;
            }
            rocks.sort_unstable();
            let pick = self.civ_rng.int(0, rocks.len().min(3) as u32 - 1) as usize;
            mining.miners.push(Miner {
                body: member,
                rock: rocks[pick].3,
                fled: 0.0,
                working: false,
                patience: PATIENCE,
                end: at,
            });
            return;
        }
        mining.retry = RETRY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::simulation::tests::{DT, add, empty_game, set_player, spawn};
    use crate::territory::{CivShape, Fall};

    const SEED: u64 = crate::config::MASTER_SEED;

    fn horde() -> crate::territory::Territory {
        for x in -40..=40 {
            for y in -40..=40 {
                if let Some(t) = world::territory(SEED, SectorId { x, y })
                    && t.capital == (SectorId { x, y })
                    && t.shape == CivShape::Horde
                {
                    return t;
                }
            }
        }
        panic!("no horde");
    }

    fn hold(game: &mut Game, at: Vec2, seconds: f32) {
        for _ in 0..(seconds / 0.05) as usize {
            set_player(game, at, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
    }

    fn quiet_visit(t: &crate::territory::Territory) -> (Game, Vec2) {
        let spot = t.capital.center() + Vec2::new(0.0, 2800.0);
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.teleport(spot);
        game.step(DT, Input::default());
        (game, spot)
    }

    #[test]
    fn miners_work_rocks_in_their_territory_under_every_cap_and_share_the_mined_map() {
        let t = horde();
        let (mut game, spot) = quiet_visit(&t);
        let rocks_before = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Asteroid)
            .count();
        let mut seen_miners = 0;
        for _ in 0..24 {
            hold(&mut game, spot, 10.0);
            let m = game.civ_mining.get(&t.id).expect("a horde mines");
            seen_miners = seen_miners.max(m.miners.len());
            assert!(m.miners.len() <= MAX_MINERS);
            assert!(m.total() <= STOCK_CAP + 1e-3, "stash {}", m.total());
            assert!(m.spent <= game.ore_budget(t.id) + 1e-3);
            let rocks: Vec<u64> = m.miners.iter().map(|x| x.rock).collect();
            let mut unique = rocks.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), rocks.len(), "one rock per miner");
            for miner in &m.miners {
                let rock = game.body(miner.rock).unwrap();
                let (q, _) = rock.origin.unwrap();
                // Judged where the rock was placed: a drifting rock may cross the border
                // after a miner has taken it.
                assert_eq!(world::territory(SEED, q).map(|x| x.id), Some(t.id));
                assert!(q.chebyshev_distance(t.capital) <= 4);
            }
        }
        assert!(seen_miners > 0, "miners were assigned");
        let m = &game.civ_mining[&t.id];
        assert!(m.spent > 1.0, "ore was worked: {}", m.spent);
        assert!(
            !game.mined.is_empty() || game.fallen.values().any(|f| !f.is_empty()),
            "spent ore is remembered in the shared map"
        );
        let rocks_after = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Asteroid)
            .count();
        assert!(
            rocks_before.saturating_sub(rocks_after) <= 12,
            "no wiping out the sector's rocks"
        );
    }

    #[test]
    fn the_budget_the_cap_and_a_hunting_ship_all_stop_mining() {
        let t = horde();
        let (mut game, spot) = quiet_visit(&t);
        // Budget spent: nobody is sent.
        let budget = game.ore_budget(t.id);
        game.civ_mining.insert(
            t.id,
            Mining {
                spent: budget,
                ..Mining::default()
            },
        );
        hold(&mut game, spot, 20.0);
        assert!(game.civ_mining[&t.id].miners.is_empty());
        // Stash full: nobody is sent either.
        game.civ_mining.insert(
            t.id,
            Mining {
                stock: [150.0, 0.0, 0.0],
                ..Mining::default()
            },
        );
        // (The capital would draw the stash down whenever it can build a guardian, so the
        // fixture keeps it full.)
        for _ in 0..400 {
            game.civ_mining.get_mut(&t.id).unwrap().stock = [150.0, 0.0, 0.0];
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
        assert!(game.civ_mining[&t.id].miners.is_empty());
        assert!(game.civ_mining[&t.id].total() <= STOCK_CAP + 1e-3);
        // A ship on top of the capital sends unescorted miners running, and nobody starts.
        game.civ_mining.insert(t.id, Mining::default());
        let heart = t.capital.center();
        game.teleport(heart);
        for _ in 0..200 {
            set_player(&mut game, heart, Vec2::ZERO);
            game.step(0.05, Input::default());
            let m = &game.civ_mining[&t.id];
            for miner in m.miners.iter().filter(|x| x.working) {
                let at = game.body(miner.body).unwrap().position;
                assert!(at.distance(heart) >= FLEE_RANGE || game.civ_strength(t.id) > 0);
            }
        }
    }

    #[test]
    fn stock_replaces_the_trickle_and_feeds_the_capital() {
        let t = horde();
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.teleport(t.capital.center() + Vec2::new(0.0, 2800.0));
        game.step(DT, Input::default());
        let spot = game.player().unwrap().position;
        let base = game
            .bodies
            .iter()
            .find(|b| {
                b.origin.is_some_and(|o| {
                    game.civ_bases
                        .get(&o)
                        .is_some_and(|c| c.1 == CivRole::Capital)
                })
            })
            .map(|b| b.id)
            .unwrap();
        let stock = |g: &Game| g.body(base).unwrap().base.as_ref().unwrap().stock;
        // Mining on with an empty stash and no guardian slot trouble: no trickle at all.
        game.civ_mining.insert(
            t.id,
            Mining {
                active: true,
                stock: [60.0, 0.0, 0.0],
                ..Mining::default()
            },
        );
        // Keep the miners out of it so the stash is the only income.
        game.civ_mining.get_mut(&t.id).unwrap().spent = game.ore_budget(t.id);
        let before = stock(&game);
        for _ in 0..40 {
            set_player(&mut game, spot, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
        let after_stock = game.civ_mining[&t.id].stock[0];
        assert!(after_stock < 60.0, "the capital drew on the stash");
        assert!(stock(&game) > before || after_stock < 55.0);
    }

    #[test]
    fn a_fallen_capital_spills_half_its_stash_as_loot() {
        let t = horde();
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.teleport(t.capital.center());
        game.step(DT, Input::default());
        game.civ_mining.insert(
            t.id,
            Mining {
                stock: [60.0, 20.0, 0.0],
                active: true,
                ..Mining::default()
            },
        );
        assert!(
            !game.caches().is_empty(),
            "a stocked capital shows its cache"
        );
        let base = game
            .bodies
            .iter()
            .find(|b| {
                b.origin.is_some_and(|o| {
                    game.civ_bases
                        .get(&o)
                        .is_some_and(|c| c.1 == CivRole::Capital)
                })
            })
            .map(|b| b.id)
            .unwrap();
        game.pickups.clear();
        game.bodies
            .iter_mut()
            .find(|b| b.id == base)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        let got = |kind| {
            game.pickups
                .iter()
                .filter_map(|p| match p.item {
                    Item::Material(k, a) if k == kind => Some(a),
                    _ => None,
                })
                .sum::<f32>()
        };
        // The ship may have collected some of it on the spot.
        let metal = got(Material::Metal) + game.cargo.metal;
        let volatiles = got(Material::Volatiles) + game.cargo.volatiles;
        assert!(metal >= 29.0, "{metal}");
        assert!(volatiles >= 9.0, "{volatiles}");
        assert!(game.civ_stock(t.id) < 1.0);
        assert!(game.caches().is_empty());
        assert!(
            game.civ_fall(t.id)
                == Fall {
                    capital: true,
                    elder: false
                }
        );
    }

    #[test]
    fn draining_a_rock_obeys_the_lode_rules_and_a_planetoid_is_shared_with_the_player() {
        let mut game = empty_game();
        let q = SectorId { x: 9, y: 9 };
        let id = add(&mut game, BodyKind::Asteroid, Vec2::new(0.0, 900.0));
        {
            let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            b.rock = RockKind::Planetoid;
            b.pinned = true;
            b.radius = 200.0;
            b.origin = Some((q, 4));
        }
        let index = game.bodies.iter().position(|b| b.id == id).unwrap();
        assert!(
            game.drain_rock(index, 150.0).is_none(),
            "a planetoid never crumbles"
        );
        assert_eq!(game.bodies[index].radius, 200.0);
        // A reload meets the depleted planetoid.
        let mut fresh = game.make_body(BodyKind::Asteroid, Vec2::ZERO);
        fresh.rock = RockKind::Planetoid;
        fresh.radius = 200.0;
        fresh.origin = Some((q, 4));
        game.apply_mined(&mut fresh);
        assert!((fresh.ore() - 250.0).abs() < 0.5, "{}", fresh.ore());
        // A small rock shrinks, then crumbles at the floor and is recorded as fallen.
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(0.0, -900.0));
        {
            let b = game.bodies.iter_mut().find(|b| b.id == rock).unwrap();
            b.radius = 40.0;
            b.origin = Some((q, 7));
        }
        let index = game.bodies.iter().position(|b| b.id == rock).unwrap();
        assert!(game.drain_rock(index, 10.0).is_none());
        assert!(game.bodies[index].radius < 40.0 && game.bodies[index].radius > 14.0);
        let left = game.bodies[index].ore();
        assert!(game.drain_rock(index, left - 0.01).is_some());
        assert!(game.bodies[index].consumed);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        game.step(DT, Input::default());
        assert!(game.fallen.get(&q).is_some_and(|s| s.contains(&7)));
    }

    #[test]
    fn creatures_do_not_shoot_at_a_wall_without_a_line_to_the_ship() {
        let t = horde();
        let species = t.member(SEED);
        let run = |wall: bool| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            game.civ_territories.insert(t.id, t);
            game.set_regard(t.id, -80.0);
            game.civ_lineages.insert(t.id, (t.id, CivRole::Member));
            let shooter = spawn(&mut game, &species, Vec2::new(0.0, 500.0));
            if wall {
                let id = add(&mut game, BodyKind::Asteroid, Vec2::new(0.0, 250.0));
                let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                b.rock = RockKind::Wall;
                b.pinned = true;
                b.radius = 60.0;
            }
            let mut shots = 0;
            for _ in 0..360 {
                set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
                if let Some(b) = game.bodies.iter_mut().find(|b| b.id == shooter) {
                    b.alert = true;
                    b.position = Vec2::new(0.0, 500.0);
                }
                game.step(DT, Input::default());
                shots += game.bullets.iter().filter(|b| !b.friendly).count();
            }
            shots
        };
        assert!(run(false) > 0, "sanity: it shoots in the open");
        assert_eq!(run(true), 0, "no shots into a wall");
    }

    #[test]
    fn withdrawing_takes_from_the_richest_material_and_never_overdraws() {
        let mut m = Mining {
            stock: [10.0, 30.0, 5.0],
            ..Mining::default()
        };
        assert!((m.withdraw(20.0) - 20.0).abs() < 1e-4);
        assert!((m.stock[1] - 10.0).abs() < 1e-4);
        let rest = m.withdraw(100.0);
        assert!((rest - 25.0).abs() < 1e-3);
        assert!(m.total() < 1e-3);
    }
}
