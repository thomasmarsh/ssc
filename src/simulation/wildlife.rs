//! Wildlife versus civilizations in play. `crate::affinity` says how a species regards a
//! civilization (hostile, neutral, friendly); this module acts on it.
//!
//! Every `FAUNA_PERIOD` seconds each wild creature near a civil body is classed by the affinity
//! of its lineage for that civilization at its position:
//!
//! - **Hostile** wildlife sets upon the nearest civil body (people, seats, turrets), biting it.
//!   Pressure is capped per body and per civilization, a settlement is spared most of it, and
//!   structures and elders are worn down but never destroyed (their fall is the ship's doing).
//! - A civilization's idle people hunt hostile wildlife near their post, and its turrets fire
//!   on it. Kills on either side are eaten-style: no score, no loot.
//! - **Friendly** wildlife is not shooed off and drifts toward the settlement, forming mixed
//!   herds near its people; it takes no part in a civilization's alarms.
//! - **Neutral** wildlife is left as before (it edges away from civil creatures).
//!
//! Stances are rebuilt from scratch each scan and kept in ordered maps, so play is
//! deterministic. Nothing here draws.

use super::tuning as t;
use super::*;
use crate::affinity::{self, Disposition};
use crate::territory::{CivRole, Standing};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stance {
    pub tid: u64,
    /// The civil body the wildlife is set on (hostile) or drawn toward (friendly).
    pub target: u64,
    pub disposition: Disposition,
    pub affinity: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Fauna {
    /// Wild body -> its stance toward a civilization (neutral wildlife has none).
    pub stances: BTreeMap<u64, Stance>,
    /// Striking civil body (idle person or turret) -> the hostile wild body it is after.
    pub defenders: BTreeMap<u64, u64>,
    clock: f32,
    /// Per civilization, what the ship has earned by killing its enemies in this window.
    credit: BTreeMap<u64, Credit>,
    /// Game time before which no further kill banner is posted.
    banner_until: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Credit {
    start: f32,
    kills: u32,
    earned: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Attack,
    Herd,
    Defend,
}

/// What steering needs to know about a body involved: which way, how far, and why.
#[derive(Clone, Copy)]
pub(super) struct Pull {
    pub toward: Vec2,
    pub distance: f32,
    pub mode: Mode,
}

struct Thing {
    id: u64,
    tid: u64,
    position: Vec2,
    unit: bool,
    turret: bool,
    idle: bool,
    home: Option<Vec2>,
}

impl Game {
    /// The disposition of a lineage toward a registered civilization at a world position.
    pub fn fauna_affinity(&self, body: &Body, tid: u64) -> Option<f32> {
        let civ = self.civ_territories.get(&tid)?;
        Some(affinity::affinity(
            self.seed,
            body.species,
            &body.genome,
            civ,
            body.position / world::SECTOR_SIZE,
        ))
    }

    /// The civilization a body belongs to: a creature by lineage, a station or turret by spawn.
    fn civil_owner(&self, b: &Body) -> Option<u64> {
        match b.kind {
            BodyKind::Creature if !b.follower => self.civ_of(b).map(|c| c.0),
            BodyKind::Base => {
                let key = b.origin?;
                self.civ_bases
                    .get(&key)
                    .or_else(|| self.civ_works.get(&key))
                    .map(|c| c.0)
            }
            _ => None,
        }
    }

    fn scan_fauna(&mut self) {
        let mut things: Vec<Thing> = Vec::new();
        for b in self
            .bodies
            .iter()
            .filter(|b| b.active && b.health > 0.0 && !b.consumed)
        {
            let Some(tid) = self.civil_owner(b) else {
                continue;
            };
            if !self.civ_territories.contains_key(&tid)
                || self.civ_standing(tid) == Standing::Fallen
            {
                continue;
            }
            let turret = b.kind == BodyKind::Base
                && b.origin
                    .and_then(|o| self.civ_works.get(&o))
                    .is_some_and(|c| c.1 == CivRole::Turret);
            things.push(Thing {
                id: b.id,
                tid,
                position: b.position,
                unit: b.kind == BodyKind::Creature,
                turret,
                idle: !b.alert && b.panic <= 0.0,
                home: b.home,
            });
        }
        let mut fauna = Fauna {
            clock: t::FAUNA_PERIOD,
            ..Fauna::default()
        };
        if things.is_empty() {
            self.keep_stances(fauna);
            return;
        }
        // Candidates: (distance squared, wild id, thing index, affinity, disposition).
        let mut hostile: Vec<(f32, u64, usize, f32)> = Vec::new();
        for w in self.bodies.iter().filter(|b| {
            b.active
                && b.kind == BodyKind::Creature
                && !b.follower
                && b.health > 0.0
                && !b.consumed
                && b.root.is_none()
                && b.parent.is_none()
                && !self.civ_lineages.contains_key(&b.species)
                && self.apex_of(b).is_none()
        }) {
            let mut best: Option<(f32, usize)> = None;
            for (i, th) in things.iter().enumerate() {
                let d = th.position.distance_squared(w.position);
                if d < t::FRIEND_RANGE * t::FRIEND_RANGE && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
            let Some((d, i)) = best else { continue };
            let Some(a) = self.fauna_affinity(w, things[i].tid) else {
                continue;
            };
            match Disposition::of(a) {
                Disposition::Hostile if d < t::HOSTILE_REACH * t::HOSTILE_REACH => {
                    hostile.push((d, w.id, i, a));
                }
                Disposition::Friendly => {
                    fauna.stances.insert(
                        w.id,
                        Stance {
                            tid: things[i].tid,
                            target: things[i].id,
                            disposition: Disposition::Friendly,
                            affinity: a,
                        },
                    );
                }
                _ => {}
            }
        }
        hostile.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut on_target: BTreeMap<u64, usize> = BTreeMap::new();
        let mut on_civ: BTreeMap<u64, usize> = BTreeMap::new();
        for (_, wid, i, a) in hostile {
            let th = &things[i];
            let cap = if self.civ_peaceful(th.tid) {
                t::MAX_ASSAULT_PEACEFUL
            } else {
                t::MAX_ASSAULT
            };
            let (n_target, n_civ) = (
                on_target.get(&th.id).copied().unwrap_or(0),
                on_civ.get(&th.tid).copied().unwrap_or(0),
            );
            if n_target >= t::MAX_ATTACKERS || n_civ >= cap {
                continue;
            }
            *on_target.entry(th.id).or_default() += 1;
            *on_civ.entry(th.tid).or_default() += 1;
            fauna.stances.insert(
                wid,
                Stance {
                    tid: th.tid,
                    target: th.id,
                    disposition: Disposition::Hostile,
                    affinity: a,
                },
            );
        }
        // Defenders: each idle person or turret picks the nearest hostile of its own civilization.
        let positions: BTreeMap<u64, Vec2> = self
            .bodies
            .iter()
            .filter(|b| fauna.stances.contains_key(&b.id))
            .map(|b| (b.id, b.position))
            .collect();
        for th in things.iter().filter(|th| (th.unit && th.idle) || th.turret) {
            let range = if th.turret {
                t::TURRET_DEFEND_RANGE
            } else {
                t::DEFEND_RANGE
            };
            let pick = fauna
                .stances
                .iter()
                .filter(|(_, s)| s.disposition == Disposition::Hostile && s.tid == th.tid)
                .filter_map(|(wid, _)| positions.get(wid).map(|p| (*wid, *p)))
                .filter(|(_, p)| p.distance(th.position) < range)
                .filter(|(_, p)| {
                    th.home
                        .is_none_or(|h| p.distance(h) < t::DEFEND_LEASH + range)
                })
                .min_by(|a, b| {
                    a.1.distance_squared(th.position)
                        .total_cmp(&b.1.distance_squared(th.position))
                        .then(a.0.cmp(&b.0))
                });
            if let Some((wid, _)) = pick {
                fauna.defenders.insert(th.id, wid);
            }
        }
        self.keep_stances(fauna);
    }

    /// Swaps in a fresh scan, keeping what the ship has earned.
    fn keep_stances(&mut self, fresh: Fauna) {
        self.fauna.stances = fresh.stances;
        self.fauna.defenders = fresh.defenders;
        self.fauna.clock = fresh.clock;
    }

    /// A wild creature fell to the ship near a civilization's people. Killing what a civilization
    /// has taken in costs regard (by how fond of it they are); killing what they are at war with
    /// earns a little, with diminishing returns and a cap per window so it cannot be farmed.
    pub(super) fn wildlife_killed(&mut self, body: &Body) {
        if body.kind != BodyKind::Creature || body.follower || body.consumed {
            return;
        }
        let Some(st) = self.fauna.stances.get(&body.id).copied() else {
            return;
        };
        if self.civ_standing(st.tid) == Standing::Fallen {
            return;
        }
        let Some(civ) = self.civ_territories.get(&st.tid).copied() else {
            return;
        };
        let name = civ.name(self.seed);
        let now = self.time;
        let (delta, text, rarity) = match st.disposition {
            Disposition::Friendly => (
                -t::FRIEND_KILL_COST * st.affinity,
                format!("{name}  - they valued this herd"),
                upgrades::Rarity::Rare,
            ),
            Disposition::Hostile => {
                let credit = self.fauna.credit.entry(st.tid).or_default();
                if now - credit.start > t::GAIN_WINDOW {
                    *credit = Credit {
                        start: now,
                        ..Credit::default()
                    };
                }
                let want = t::HOSTILE_KILL_GAIN
                    * -st.affinity
                    * t::GAIN_DIMINISH.powi(credit.kills as i32);
                let gain = want.min((t::GAIN_CAP - credit.earned).max(0.0));
                credit.kills += 1;
                credit.earned += gain;
                (
                    gain,
                    format!("{name}  - they thank you"),
                    upgrades::Rarity::Common,
                )
            }
            Disposition::Neutral => return,
        };
        if delta == 0.0 {
            return;
        }
        self.shift_regard(st.tid, delta);
        if now >= self.fauna.banner_until {
            self.fauna.banner_until = now + t::KILL_BANNER_EVERY;
            self.notify(text, rarity);
        }
    }

    /// The wildlife near the ship that a civilization of the territory it is in has a view on,
    /// nearest first: (species name, disposition), hostile and friendly only, at most
    /// `TAG_COUNT`.
    pub fn fauna_tags(&self) -> Vec<(String, Disposition)> {
        let (Some(civ), Some(ship)) = (self.territory, self.player().map(|p| p.position)) else {
            return Vec::new();
        };
        let mut seen: Vec<(f32, u64, String, Disposition)> = Vec::new();
        for b in self.bodies.iter().filter(|b| {
            b.active
                && b.kind == BodyKind::Creature
                && !b.follower
                && !self.civ_lineages.contains_key(&b.species)
                && self.apex_of(b).is_none()
        }) {
            let d = b.position.distance_squared(ship);
            if d > t::TAG_RANGE * t::TAG_RANGE || seen.iter().any(|s| s.1 == b.species) {
                continue;
            }
            let a = affinity::affinity(
                self.seed,
                b.species,
                &b.genome,
                &civ,
                b.position / world::SECTOR_SIZE,
            );
            let disposition = Disposition::of(a);
            if disposition != Disposition::Neutral {
                seen.push((d, b.species, b.genome.name(), disposition));
            }
        }
        seen.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        seen.into_iter()
            .take(t::TAG_COUNT)
            .map(|(_, _, n, d)| (n, d))
            .collect()
    }

    /// How the wildlife of a sector in or beside a territory stands toward it, for the star
    /// map: the civilization's name and the sector's mood.
    pub fn sector_mood(&self, sector: SectorId) -> Option<(String, affinity::Mood)> {
        let civ = crate::territory::nearby_territory(self.seed, sector)?;
        let eco = crate::range::ecology(self.seed, sector);
        Some((
            civ.name(self.seed),
            affinity::mood(self.seed, &eco, &civ, sector),
        ))
    }

    /// Per tick: rescan now and then, then let hostile wildlife bite and defenders strike.
    pub(super) fn update_wildlife(&mut self, dt: f32) {
        self.fauna.clock -= dt;
        if self.fauna.clock <= 0.0 {
            self.scan_fauna();
        }
        if self.fauna.stances.is_empty() {
            return;
        }
        let index: HashMap<u64, usize> = self
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.active)
            .map(|(i, b)| (b.id, i))
            .collect();
        let bites: Vec<(u64, Stance)> = self
            .fauna
            .stances
            .iter()
            .filter(|(_, s)| s.disposition == Disposition::Hostile)
            .map(|(w, s)| (*w, *s))
            .collect();
        for (wid, st) in bites {
            let (Some(&wi), Some(&ti)) = (index.get(&wid), index.get(&st.target)) else {
                continue;
            };
            let (w, tgt) = (&self.bodies[wi], &self.bodies[ti]);
            let reach = w.radius + tgt.radius + t::FAUNA_BITE_REACH;
            if w.bite_clock > 0.0
                || w.panic > 0.0
                || tgt.health <= 0.0
                || w.position.distance_squared(tgt.position) > reach * reach
            {
                continue;
            }
            let mut amount = (t::FAUNA_BITE + t::FAUNA_BITE_PER_CONTACT * w.genome.contact_damage)
                * w.genes.threat.max(1.0);
            let guarded = tgt.kind == BodyKind::Base || self.is_elder(tgt);
            if tgt.kind == BodyKind::Base {
                amount *= t::FAUNA_STRUCTURE_SCALE;
            }
            if self.civ_peaceful(st.tid) {
                amount *= t::FAUNA_PEACEFUL_SCALE;
            }
            self.bodies[wi].bite_clock = t::FAUNA_BITE_PERIOD;
            let at = self.bodies[ti].position;
            let tgt = &mut self.bodies[ti];
            damage(tgt, amount, 0.0);
            if guarded {
                tgt.health = tgt.health.max(tgt.max_health * t::FAUNA_STRUCTURE_FLOOR);
            } else if tgt.health <= 0.0 {
                tgt.consumed = true;
            }
            self.effect(at, 10.0, 0.18, EffectKind::Impact);
        }
        let strikes: Vec<(u64, u64)> = self.fauna.defenders.iter().map(|(a, b)| (*a, *b)).collect();
        for (sid, wid) in strikes {
            let (Some(&si), Some(&wi)) = (index.get(&sid), index.get(&wid)) else {
                continue;
            };
            let s = &self.bodies[si];
            let w = &self.bodies[wi];
            if s.fire_cooldown > 0.0 || w.health <= 0.0 {
                continue;
            }
            let turret = s.kind == BodyKind::Base;
            let reach = if turret {
                t::TURRET_DEFEND_RANGE
            } else if s.genome.weapon != crate::genome::Weapon::None {
                (s.genome.weapon_range * t::DEFEND_REACH_SHARE).min(t::DEFEND_RANGE)
            } else {
                s.radius + w.radius + 12.0
            };
            if s.position.distance(w.position) > reach {
                continue;
            }
            let amount = if turret {
                t::TURRET_STRIKE * s.genes.threat.max(1.0)
            } else {
                t::STRIKE_DAMAGE * s.genes.threat.max(1.0)
            };
            let period = if turret {
                1.2
            } else {
                s.genome.fire_period.max(t::STRIKE_PERIOD_MIN)
            };
            self.bodies[si].fire_cooldown = period;
            let at = self.bodies[wi].position;
            let target = &mut self.bodies[wi];
            damage(target, amount, 0.0);
            if target.health <= 0.0 {
                target.consumed = true;
            }
            self.effect(at, 12.0, 0.2, EffectKind::Impact);
        }
    }

    /// Steering's view: for every body with a stance or a hunt, which way and why.
    pub(super) fn fauna_pulls(&self) -> HashMap<u64, Pull> {
        let mut out = HashMap::new();
        if self.fauna.stances.is_empty() {
            return out;
        }
        let wanted: HashSet<u64> = self
            .fauna
            .stances
            .values()
            .map(|s| s.target)
            .chain(self.fauna.defenders.values().copied())
            .collect();
        let at: HashMap<u64, Vec2> = self
            .bodies
            .iter()
            .filter(|b| wanted.contains(&b.id))
            .map(|b| (b.id, b.position))
            .collect();
        let me: HashMap<u64, Vec2> = self
            .bodies
            .iter()
            .filter(|b| {
                self.fauna.stances.contains_key(&b.id) || self.fauna.defenders.contains_key(&b.id)
            })
            .map(|b| (b.id, b.position))
            .collect();
        for (wid, st) in &self.fauna.stances {
            let (Some(from), Some(to)) = (me.get(wid), at.get(&st.target)) else {
                continue;
            };
            let toward = *to - *from;
            let mode = if st.disposition == Disposition::Hostile {
                Mode::Attack
            } else {
                Mode::Herd
            };
            out.insert(
                *wid,
                Pull {
                    toward,
                    distance: toward.length(),
                    mode,
                },
            );
        }
        for (sid, wid) in &self.fauna.defenders {
            let (Some(from), Some(to)) = (me.get(sid), at.get(wid)) else {
                continue;
            };
            if self
                .bodies
                .iter()
                .any(|b| b.id == *sid && b.kind == BodyKind::Creature)
            {
                let toward = *to - *from;
                out.insert(
                    *sid,
                    Pull {
                        toward,
                        distance: toward.length(),
                        mode: Mode::Defend,
                    },
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species, Weapon};
    use crate::simulation::tests::{DT, add, empty_game, set_player, spawn};
    use crate::territory::{CivShape, Territory};

    const SEED: u64 = 42;
    /// Where the staged fights happen, far from the ship at the origin.
    const SPOT: Vec2 = Vec2::new(-1500.0, 1200.0);

    fn horde() -> Territory {
        horde_in(SEED)
    }

    fn horde_in(seed: u64) -> Territory {
        for x in -40..=40 {
            for y in -40..=40 {
                let id = SectorId { x, y };
                if let Some(t) = world::territory(seed, id)
                    && t.capital == id
                    && t.shape == CivShape::Horde
                {
                    return t;
                }
            }
        }
        panic!("no horde");
    }

    fn brute() -> Genome {
        Genome {
            hull: 400.0,
            radius: 16.0,
            mass: 8.0,
            contact_damage: 12.0,
            speed: 140.0,
            cruise: 60.0,
            sight: 200.0,
            lose: 200.0,
            ..Genome::default()
        }
    }

    /// A species of `genome` whose affinity for `t` at `at` (world position) has the wanted
    /// disposition.
    fn species_that_is(want: Disposition, genome: Genome, t: &Territory, at: Vec2) -> Species {
        for k in 1..2000u64 {
            let s = Species {
                lineage: k * 7_919 + 1,
                generation: 0,
                genome,
            };
            let a = affinity::affinity(SEED, s.lineage, &genome, t, at / world::SECTOR_SIZE);
            if Disposition::of(a) == want {
                return s;
            }
        }
        panic!("no {want:?} species");
    }

    fn stage(t: Territory) -> Game {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        game.civ_territories.insert(t.id, t);
        game.set_regard(t.id, 0.0);
        game
    }

    fn member_at(game: &mut Game, t: &Territory, at: Vec2) -> u64 {
        let species = t.member(SEED);
        game.civ_lineages
            .insert(species.lineage, (t.id, CivRole::Member));
        spawn(game, &species, at)
    }

    fn run(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            set_player(game, Vec2::ZERO, Vec2::ZERO);
            game.step(DT, Input::default());
        }
    }

    fn health(game: &Game, id: u64) -> f32 {
        game.body(id).map_or(0.0, |b| b.health)
    }

    #[test]
    fn hostile_species_hurt_a_civilization() {
        let t = horde();
        let mut game = stage(t);
        // A defenceless member: nobody strikes back, so the bites are the whole story.
        let member = member_at(&mut game, &t, SPOT);
        let hull = health(&game, member);
        game.bodies
            .iter_mut()
            .find(|b| b.id == member)
            .unwrap()
            .genome
            .weapon = Weapon::None;
        let species = species_that_is(Disposition::Hostile, brute(), &t, SPOT);
        spawn(&mut game, &species, SPOT + Vec2::new(300.0, 0.0));
        run(&mut game, 8.0);
        assert!(health(&game, member) < hull, "the member was not bitten");
    }

    #[test]
    fn civilization_defends_and_kills_without_score() {
        let t = horde();
        let mut game = stage(t);
        let member = member_at(&mut game, &t, SPOT);
        let frail = Genome {
            hull: 25.0,
            ..brute()
        };
        let species = species_that_is(Disposition::Hostile, frail, &t, SPOT);
        let wild = spawn(&mut game, &species, SPOT + Vec2::new(500.0, 0.0));
        let score = game.score;
        run(&mut game, 12.0);
        assert!(game.body(wild).is_none(), "the hostile was not seen off");
        assert_eq!(game.score, score, "a civil kill pays the ship nothing");
        assert!(game.body(member).is_some() || game.civ_strength(t.id) == 0);
    }

    #[test]
    fn friendly_wildlife_is_tolerated_and_drawn_in() {
        let t = horde();
        let mut game = stage(t);
        let member = member_at(&mut game, &t, SPOT);
        let hull = health(&game, member);
        let species = species_that_is(Disposition::Friendly, brute(), &t, SPOT);
        let wild = spawn(&mut game, &species, SPOT + Vec2::new(1200.0, 0.0));
        run(&mut game, 10.0);
        let w = game.body(wild).expect("the friend was attacked");
        assert_eq!(w.health, w.max_health);
        assert!(!w.alert);
        assert_eq!(health(&game, member), hull);
        assert_eq!(
            game.fauna.stances.get(&wild).map(|s| s.disposition),
            Some(Disposition::Friendly)
        );
        let gap = w.position.distance(game.body(member).unwrap().position);
        assert!(gap < 1400.0, "the friend stayed away: {gap}");
    }

    #[test]
    fn neutral_wildlife_and_civilization_ignore_each_other() {
        let t = horde();
        let mut game = stage(t);
        let member = member_at(&mut game, &t, SPOT);
        let hull = health(&game, member);
        let species = species_that_is(Disposition::Neutral, brute(), &t, SPOT);
        let wild = spawn(&mut game, &species, SPOT + Vec2::new(400.0, 0.0));
        run(&mut game, 8.0);
        assert!(game.fauna.stances.is_empty());
        assert_eq!(health(&game, wild), game.body(wild).unwrap().max_health);
        assert_eq!(health(&game, member), hull);
    }

    #[test]
    fn stance_follows_the_region_and_ignores_the_ships_regard() {
        let t = horde();
        // The same genome and lineage, hostile in one place and friendly in another.
        let g = brute();
        let mut found = None;
        'outer: for k in 1..800u64 {
            let lineage = k * 104_729 + 3;
            let (mut h, mut f) = (None, None);
            for i in 0..80 {
                let at = Vec2::new(0.0, 3200.0 + i as f32 * 900.0);
                let a = affinity::affinity(SEED, lineage, &g, &t, at / world::SECTOR_SIZE);
                match Disposition::of(a) {
                    Disposition::Hostile => h = h.or(Some(at)),
                    Disposition::Friendly => f = f.or(Some(at)),
                    _ => {}
                }
                if let (Some(h), Some(f)) = (h, f) {
                    found = Some((lineage, h, f));
                    break 'outer;
                }
            }
        }
        let (lineage, hostile_at, friendly_at) = found.expect("no swinging species");
        let species = Species {
            lineage,
            generation: 0,
            genome: g,
        };
        let stance_at = |at: Vec2, regard: f32| {
            let mut game = stage(t);
            member_at(&mut game, &t, at);
            game.set_regard(t.id, regard);
            let wild = spawn(&mut game, &species, at + Vec2::new(300.0, 0.0));
            game.scan_fauna();
            game.fauna.stances.get(&wild).map(|s| s.disposition)
        };
        assert_eq!(stance_at(hostile_at, 0.0), Some(Disposition::Hostile));
        assert_eq!(stance_at(friendly_at, 0.0), Some(Disposition::Friendly));
        // What the civilization thinks of the ship changes nothing about its wildlife.
        for regard in [-80.0, 80.0] {
            assert_eq!(stance_at(hostile_at, regard), Some(Disposition::Hostile));
            assert_eq!(stance_at(friendly_at, regard), Some(Disposition::Friendly));
        }
    }

    #[test]
    fn a_species_can_hate_a_civilization_yet_pass_the_ship_by() {
        let t = horde();
        // A harm-triggered creature leaves the ship alone until hurt, whatever it thinks of a
        // civilization.
        let g = Genome {
            trigger: crate::genome::Trigger::Harm,
            ..brute()
        };
        let species = species_that_is(Disposition::Hostile, g, &t, SPOT);
        let mut game = stage(t);
        member_at(&mut game, &t, SPOT);
        let wild = spawn(&mut game, &species, SPOT + Vec2::new(900.0, 0.0));
        // Before the first blow: a creature hurt in the fight is of course provoked.
        run(&mut game, 0.1);
        let w = game.body(wild).unwrap();
        assert!(!w.alert, "passive to the ship");
        assert_eq!(
            game.fauna.stances.get(&wild).map(|s| s.disposition),
            Some(Disposition::Hostile)
        );
    }

    #[test]
    fn pressure_is_capped_per_body_and_per_civilization() {
        let t = horde();
        let mut game = stage(t);
        member_at(&mut game, &t, SPOT);
        let species = species_that_is(Disposition::Hostile, brute(), &t, SPOT);
        for k in 0..30 {
            spawn(
                &mut game,
                &species,
                SPOT + Vec2::new(250.0 + 40.0 * (k % 6) as f32, 40.0 * (k / 6) as f32),
            );
        }
        game.scan_fauna();
        let hostile: Vec<_> = game
            .fauna
            .stances
            .values()
            .filter(|s| s.disposition == Disposition::Hostile)
            .collect();
        assert!(!hostile.is_empty());
        assert!(hostile.len() <= t::MAX_ASSAULT);
        let mut per: BTreeMap<u64, usize> = BTreeMap::new();
        for s in hostile {
            *per.entry(s.target).or_default() += 1;
        }
        assert!(per.values().all(|&n| n <= t::MAX_ATTACKERS));
    }

    #[test]
    fn wildlife_wears_down_a_station_but_never_destroys_it() {
        let t = horde();
        let mut game = stage(t);
        let base = add(&mut game, BodyKind::Base, SPOT);
        let key = (SectorId::containing(SPOT), 9_999);
        {
            let b = game.bodies.iter_mut().find(|b| b.id == base).unwrap();
            b.origin = Some(key);
            b.health = 60.0;
            b.max_health = 60.0;
        }
        game.civ_bases.insert(key, (t.id, CivRole::Outpost));
        let species = species_that_is(Disposition::Hostile, brute(), &t, SPOT);
        for k in 0..3 {
            spawn(
                &mut game,
                &species,
                SPOT + Vec2::new(200.0, 90.0 * k as f32),
            );
        }
        run(&mut game, 40.0);
        let h = health(&game, base);
        assert!(h < 60.0, "untouched");
        assert!(
            h >= 60.0 * t::FAUNA_STRUCTURE_FLOOR - 0.01,
            "destroyed: {h}"
        );
    }

    #[test]
    fn the_fight_is_deterministic() {
        let t = horde();
        let once = || {
            let mut game = stage(t);
            member_at(&mut game, &t, SPOT);
            member_at(&mut game, &t, SPOT + Vec2::new(100.0, 60.0));
            let species = species_that_is(Disposition::Hostile, brute(), &t, SPOT);
            for k in 0..6 {
                spawn(
                    &mut game,
                    &species,
                    SPOT + Vec2::new(400.0, 70.0 * k as f32),
                );
            }
            run(&mut game, 10.0);
            game.bodies
                .iter()
                .map(|b| (b.id, b.health.to_bits(), b.position.x.to_bits()))
                .collect::<Vec<_>>()
        };
        assert_eq!(once(), once());
    }

    /// A fresh world with the ship parked `offset` from `at`.
    fn visit(seed: u64, at: Vec2) -> Game {
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(at);
        game.step(DT, Input::default());
        game
    }

    #[test]
    fn real_territories_and_their_wildlife_stay_bounded_and_the_early_outpost_survives() {
        let seed = 0x535343;
        let outpost = crate::territory::outpost(seed);
        let ship = outpost.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(seed, ship);
        let before = game.civ_strength(outpost.id);
        assert!(before >= 1);
        for _ in 0..(150.0 / 0.05) as usize {
            set_player(&mut game, ship, Vec2::ZERO);
            game.step(0.05, Input::default());
            assert!(game.bodies.len() < MAX_BODIES);
        }
        assert!(game.civ_standing(outpost.id) == Standing::Thriving);
        assert!(
            game.civ_strength(outpost.id) >= 1,
            "the early outpost was wiped out by wildlife"
        );
        let capital = horde_in(seed);
        let ship = capital.capital.center() + Vec2::new(0.0, 2500.0);
        let mut game = visit(seed, ship);
        for _ in 0..(150.0 / 0.05) as usize {
            set_player(&mut game, ship, Vec2::ZERO);
            game.step(0.05, Input::default());
        }
        assert!(game.bodies.len() < MAX_BODIES);
        assert!(game.civ_strength(capital.id) <= 3 * crate::simulation::civ::CIV_CAP);
    }

    /// Stages a creature of `want` disposition beside a member, scans, and has the ship kill it.
    fn kill_one(game: &mut Game, t: &Territory, want: Disposition) {
        let species = species_that_is(want, brute(), t, SPOT);
        let wild = spawn(game, &species, SPOT + Vec2::new(250.0, 0.0));
        game.scan_fauna();
        assert!(game.fauna.stances.contains_key(&wild));
        game.bodies
            .iter_mut()
            .find(|b| b.id == wild)
            .unwrap()
            .health = 0.0;
        set_player(game, Vec2::ZERO, Vec2::ZERO);
        game.step(DT, Input::default());
    }

    #[test]
    fn killing_a_friendly_herd_costs_regard_and_says_so() {
        let t = horde();
        let mut game = stage(t);
        member_at(&mut game, &t, SPOT);
        let before = game.civ_regard(t.id);
        kill_one(&mut game, &t, Disposition::Friendly);
        let after = game.civ_regard(t.id);
        assert!(after < before - 0.5, "{before} -> {after}");
        assert!(after >= before - t::FRIEND_KILL_COST - 0.01);
        assert!(
            game.notices
                .iter()
                .any(|n| n.text.contains("valued this herd"))
        );
    }

    #[test]
    fn killing_a_hostile_species_earns_a_little_but_cannot_be_farmed() {
        let t = horde();
        let mut game = stage(t);
        member_at(&mut game, &t, SPOT);
        let before = game.civ_regard(t.id);
        kill_one(&mut game, &t, Disposition::Hostile);
        let first = game.civ_regard(t.id) - before;
        assert!(
            first > 0.0 && first <= t::HOSTILE_KILL_GAIN + 0.01,
            "{first}"
        );
        assert!(game.notices.iter().any(|n| n.text.contains("thank you")));
        let mut last = first;
        for _ in 0..40 {
            let was = game.civ_regard(t.id);
            kill_one(&mut game, &t, Disposition::Hostile);
            let gain = game.civ_regard(t.id) - was;
            // Never more than the first, and shrinking on the whole.
            assert!(gain <= first + 0.01);
            last = gain;
        }
        let total = game.civ_regard(t.id) - before;
        assert!(total <= t::GAIN_CAP + 0.01, "farmed {total}");
        assert!(last < first);
        // A new window pays again.
        game.time += t::GAIN_WINDOW + 1.0;
        let was = game.civ_regard(t.id);
        kill_one(&mut game, &t, Disposition::Hostile);
        assert!(game.civ_regard(t.id) > was);
    }

    #[test]
    fn neutral_kills_and_far_kills_cost_nothing() {
        let t = horde();
        let mut game = stage(t);
        member_at(&mut game, &t, SPOT);
        let species = species_that_is(Disposition::Neutral, brute(), &t, SPOT);
        let wild = spawn(&mut game, &species, SPOT + Vec2::new(250.0, 0.0));
        game.scan_fauna();
        game.bodies
            .iter_mut()
            .find(|b| b.id == wild)
            .unwrap()
            .health = 0.0;
        let before = game.civ_regard(t.id);
        game.step(DT, Input::default());
        assert_eq!(game.civ_regard(t.id), before);
    }

    #[test]
    fn the_hud_tags_and_the_star_map_read_the_same_affinities() {
        let t = horde();
        let mut game = stage(t);
        game.territory = Some(t);
        let friendly = species_that_is(Disposition::Friendly, brute(), &t, SPOT);
        let hostile = species_that_is(Disposition::Hostile, brute(), &t, SPOT);
        spawn(&mut game, &friendly, SPOT);
        spawn(&mut game, &hostile, SPOT + Vec2::new(50.0, 0.0));
        let tags = game.fauna_tags();
        assert!(tags.iter().any(|t| t.1 == Disposition::Friendly));
        assert!(tags.iter().any(|t| t.1 == Disposition::Hostile));
        let mood = game.sector_mood(t.capital);
        assert!(mood.is_some());
        assert_eq!(game.sector_mood(SectorId::ORIGIN), None);
    }
}
