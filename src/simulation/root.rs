//! Rooted life. A creature whose genome says so clings to a rock or planetoid: it sits at a
//! fixed anchor on the host's surface (so it rides the host's drift and slow turn), does not
//! steer, and defends itself with its genome's weapon (only toward the open side) and a
//! stinging contact. It feeds from the host's lichen (shared among everyone on it) and from
//! plankton that drifts by, and grows like any juvenile. What happens next is the habit gene
//! (`Genome::habit`): a rooted young one lets go at maturity, or earlier when it is big
//! enough, hungry or crowded by its own genes' thresholds, and goes free as an adult; a
//! lifelong rooter never lets go except when its host is lost, when it falls free, dazed, and
//! looks for another rock. Eggs of rooting mothers are laid on a rock and hatch rooted.
//!
//! The link is only an id: a host that is destroyed, eaten or unloaded is found missing the
//! next tick and everything on it is released, so nothing dangles.

use super::attach::{self, Frame};
use super::*;
use crate::genome::Habit;

/// Seconds a creature is dazed (scattering, not firing) after its host is lost.
pub const DAZE: f32 = 2.5;
/// Seconds after letting go before a creature may cling again, so it can get away.
pub const REATTACH_DELAY: f32 = 5.0;
/// How long a brood released by the loss of its host stays on the hunt, whatever it saw.
pub const SWARM_RAGE: f32 = 30.0;
/// Fraction of its radius at which a rooted body's center stands off the surface (so it
/// sits nearly on it, a little sunk in).
pub const STAND: f32 = 0.9;
/// How near a free creature must come to a rock's surface to take hold, and how far it
/// looks for one.
const REACH: f32 = 45.0;
pub const SEEK_RANGE: f32 = 700.0;
/// Fraction of a host's rim that rooters may fill.
const RIM_FILL: f32 = 0.9;
/// Energy per second a host yields to everyone on it: its plankton rate times this and a
/// plankton's nutrition. One rooter takes at most `MAX_FEED` of its capacity per second.
const HOST_YIELD: f32 = 12.0;
const MAX_FEED: f32 = 0.01;
/// How hard a freshly released creature is pushed away from its host.
const KICK: f32 = 60.0;
/// A defender fires only into this half-plane: the dot of aim and outward must exceed it.
pub(super) const FIRE_ARC: f32 = -0.1;

/// A creature's hold on a host: its id, and the angle around it in the host's own frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Root {
    pub host: u64,
    pub angle: f32,
}

/// A rock a creature may cling to: free or planetoid, not a crystal, a husk or a nest stone.
pub fn can_host(rock: &Body) -> bool {
    rock.kind == BodyKind::Asteroid
        && rock.health > 0.0
        && match rock.rock {
            RockKind::Planetoid => true,
            RockKind::Plain | RockKind::Ice | RockKind::Ore => !rock.pinned,
            RockKind::Crystal | RockKind::Husk | RockKind::Wall => false,
        }
}

/// A living creature whose genome has host slots: its residents ride it like rooters ride a
/// rock, and are let go the next tick it is found missing (dead, eaten or unloaded).
pub fn carries_residents(body: &Body) -> bool {
    body.kind == BodyKind::Creature
        && body.health > 0.0
        && !body.consumed
        && body.genome.hosted.is_some()
}

/// Where a creature of `radius` anchored at `angle` (host frame) sits on `host`.
pub fn place(host: &Body, angle: f32, radius: f32) -> Vec2 {
    Frame::of(host).seat(angle, radius, STAND)
}

/// How many creatures of `radius` fit around a host's rim.
pub fn capacity(host_radius: f32, radius: f32) -> usize {
    attach::capacity(host_radius, radius, RIM_FILL)
}

impl Body {
    /// How the creature will live once grown: a juvenile is judged by the genome it grows into.
    pub fn habit(&self) -> Habit {
        self.adult.as_ref().unwrap_or(&self.genome).habit()
    }

    /// True while the body is clinging to a host.
    pub fn is_rooted(&self) -> bool {
        self.root.is_some()
    }

    /// Whether a free creature wants a host right now: a lifelong rooter always, a rooting
    /// young one while it is still small and fed (`detach_size`, `detach_hunger` decide when
    /// it leaves, so it does not just hop back on).
    pub(super) fn wants_host(&self) -> bool {
        self.kind == BodyKind::Creature
            && !self.follower
            && self.chain.is_none()
            && self.root.is_none()
            && self.unrooted <= 0.0
            && self.panic <= 0.0
            && !self.alert
            && match self.habit() {
                Habit::Free => false,
                Habit::Life => true,
                Habit::Juvenile => {
                    self.adult.is_some()
                        && self.growth < self.genome.detach_size * 0.9
                        && self.energy_fraction() > self.genome.detach_hunger + 0.2
                }
            }
    }
}

/// A host as the rooting pass sees it.
#[derive(Clone, Copy)]
struct Host {
    position: Vec2,
    velocity: Vec2,
    angle: f32,
    radius: f32,
    /// Energy per second the host yields to everyone clinging to it.
    yield_rate: f32,
    /// A creature with host slots rather than a rock: only its own residents ride it, no
    /// free creature ever takes hold of it.
    carrier: bool,
}

impl Host {
    fn frame(&self) -> Frame {
        Frame {
            position: self.position,
            angle: self.angle,
            radius: self.radius,
        }
    }
}

impl Game {
    fn hosts(&self) -> HashMap<u64, Host> {
        self.bodies
            .iter()
            .filter(|b| can_host(b) || carries_residents(b))
            .map(|b| {
                (
                    b.id,
                    Host {
                        carrier: b.kind == BodyKind::Creature,
                        position: b.position,
                        velocity: b.velocity,
                        angle: b.angle,
                        radius: b.radius,
                        yield_rate: food::fertility(b)
                            .map_or(0.0, |(rate, _, _)| rate * food::NUTRITION * HOST_YIELD),
                    },
                )
            })
            .collect()
    }

    /// Carries every rooted creature and egg with its host: position on the anchor, the
    /// host's velocity, and facing outward unless it is aiming at something.
    pub(super) fn sync_roots(&mut self) {
        if !self.bodies.iter().any(|b| b.root.is_some())
            && self.eggs.iter().all(|e| e.host.is_none())
        {
            return;
        }
        let hosts = self.hosts();
        let steering = self.pad.aiming();
        for body in self.bodies.iter_mut().filter(|b| b.root.is_some()) {
            let Some(root) = body.root else { continue };
            let Some(host) = hosts.get(&root.host) else {
                continue;
            };
            let outward = host.angle + root.angle;
            body.position = host.frame().seat(root.angle, body.radius, STAND);
            body.velocity = host.velocity;
            // A landed ship faces outward and turns with its world unless its pilot steers.
            let own = body.alert || (body.kind == BodyKind::Player && steering);
            if !own {
                body.angle = outward;
            }
        }
        for egg in self.eggs.iter_mut() {
            let Some(id) = egg.host else { continue };
            match hosts.get(&id) {
                Some(host) => {
                    egg.position = host.frame().seat(egg.anchor, egg.radius, STAND);
                    egg.velocity = host.velocity;
                }
                // The host is gone: the egg drifts free.
                None => egg.host = None,
            }
        }
    }

    /// One pass per tick: lets go of lost hosts, applies the detach genes, feeds, and lets
    /// free creatures that want a rock take hold of one they touch.
    pub(super) fn update_roots(&mut self, dt: f32) {
        for body in self.bodies.iter_mut().filter(|b| b.unrooted > 0.0) {
            body.unrooted = (body.unrooted - dt).max(0.0);
        }
        let hosts = self.hosts();
        let mut counts: HashMap<u64, usize> = HashMap::new();
        for body in self.bodies.iter().filter(|b| b.root.is_some()) {
            if let Some(root) = body.root {
                *counts.entry(root.host).or_default() += 1;
            }
        }
        let mut released: Vec<(usize, bool)> = Vec::new();
        for (index, body) in self.bodies.iter_mut().enumerate() {
            let Some(root) = body.root else { continue };
            // A landed ship is held by its pad (see `pads`), not by a creature's habits.
            if !body.active || body.kind != BodyKind::Creature {
                continue;
            }
            let Some(host) = hosts.get(&root.host) else {
                released.push((index, true));
                continue;
            };
            let n = counts.get(&root.host).copied().unwrap_or(1).max(1);
            let g = body.genome;
            if body.adult.is_some() && body.habit() == Habit::Juvenile {
                let hungry = g.detach_hunger > 0.0
                    && g.forages()
                    && !body.provisioned
                    && body.energy_fraction() < g.detach_hunger;
                let grown = body.growth >= g.detach_size;
                let crowded = n as f32 > g.detach_crowd * capacity(host.radius, body.radius) as f32;
                if grown || hungry || crowded {
                    if let Some(count) = counts.get_mut(&root.host) {
                        *count = count.saturating_sub(1);
                    }
                    released.push((index, false));
                    continue;
                }
            }
            if g.forages() && !body.provisioned {
                let share = (host.yield_rate / n as f32).min(body.max_energy * MAX_FEED);
                body.feed(share * dt);
            }
        }
        for (index, dazed) in released {
            self.release(index, dazed);
        }
        // Free creatures that want a rock take hold of one they touch.
        let seekers: Vec<usize> = self
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.active && b.wants_host())
            .map(|(i, _)| i)
            .collect();
        for index in seekers {
            self.try_attach(index, &hosts, &counts);
            if let Some(root) = self.bodies[index].root {
                *counts.entry(root.host).or_default() += 1;
            }
        }
    }

    fn try_attach(
        &mut self,
        index: usize,
        hosts: &HashMap<u64, Host>,
        counts: &HashMap<u64, usize>,
    ) {
        let (at, radius) = (self.bodies[index].position, self.bodies[index].radius);
        let mut near: Vec<(f32, u64)> = hosts
            .iter()
            .filter(|(_, h)| !h.carrier)
            .map(|(&id, h)| (at.distance(h.position) - h.radius - radius * STAND, id))
            .filter(|&(gap, id)| {
                gap < REACH
                    && counts.get(&id).copied().unwrap_or(0) < capacity(hosts[&id].radius, radius)
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, id) in near {
            let host = hosts[&id];
            let wanted = (at - host.position).to_angle() - host.angle;
            if let Some(angle) = self.free_anchor(id, host.radius, wanted, radius, None) {
                let place = host.frame().seat(angle, radius, STAND);
                let body = &mut self.bodies[index];
                body.root = Some(Root { host: id, angle });
                body.position = place;
                body.velocity = host.velocity;
                body.chain = None;
                return;
            }
        }
    }

    /// The anchor nearest `wanted` (host frame) where a body of `size` fits on host `host`
    /// without touching another rooter or egg, searching a little each way. `skip` is a body
    /// that does not count (the one being moved).
    pub(super) fn free_anchor(
        &self,
        host: u64,
        host_radius: f32,
        wanted: f32,
        size: f32,
        skip: Option<u64>,
    ) -> Option<f32> {
        let taken: Vec<(f32, f32)> = self
            .bodies
            .iter()
            .filter(|b| Some(b.id) != skip)
            .filter_map(|b| {
                b.root
                    .filter(|r| r.host == host)
                    .map(|r| (r.angle, b.radius))
            })
            .chain(
                self.eggs
                    .iter()
                    .filter(|e| e.host == Some(host))
                    .map(|e| (e.anchor, e.radius)),
            )
            .collect();
        attach::free_angle(&taken, host_radius, wanted, size)
    }

    /// Lets go of the host: the creature is pushed gently off and may not cling again for a
    /// while. `dazed` (a lost host) also leaves it scattering and unable to fire for a moment.
    pub(super) fn release(&mut self, index: usize, dazed: bool) {
        let host = self.bodies[index]
            .root
            .and_then(|r| self.body(r.host))
            .map(|h| (h.position, h.angle, h.velocity));
        self.let_go(index, host, dazed);
    }

    /// `release` with the host's position, angle and velocity supplied (it may already be
    /// out of the world).
    fn let_go(&mut self, index: usize, host: Option<(Vec2, f32, Vec2)>, dazed: bool) {
        let body = &mut self.bodies[index];
        let Some(root) = body.root.take() else { return };
        body.unrooted = REATTACH_DELAY;
        let from = match host {
            Some((position, angle, velocity)) => {
                let out = Vec2::from_angle(angle + root.angle);
                body.velocity = velocity + out * KICK;
                body.wander = out.to_angle();
                position
            }
            None => body.position - Vec2::from_angle(body.angle),
        };
        if dazed && crate::hosted::is_brood(&body.genome) {
            // Orphaned young do not scatter, they swarm the one near.
            body.provoked = SWARM_RAGE;
        } else if dazed {
            body.panic = DAZE;
            body.panic_from = from;
        }
    }

    /// A host is being destroyed: everything clinging to it falls free with its momentum.
    pub(super) fn release_from(&mut self, rock: &Body) {
        let host = Some((rock.position, rock.angle, rock.velocity));
        for index in 0..self.bodies.len() {
            if self.bodies[index].root.is_some_and(|r| r.host == rock.id) {
                self.let_go(index, host, true);
            }
        }
        for egg in self.eggs.iter_mut().filter(|e| e.host == Some(rock.id)) {
            egg.host = None;
        }
    }

    /// A rock where a newborn of `size` could be placed near `at`: the parent's own host if
    /// it has one, else the nearest rock whose surface is within `reach`. Returns the host id
    /// and the anchor (host frame).
    pub(super) fn root_site(
        &self,
        at: Vec2,
        size: f32,
        prefer: Option<u64>,
        reach: f32,
    ) -> Option<(u64, f32)> {
        let hosts = self.hosts();
        let mut near: Vec<(f32, u64)> = hosts
            .iter()
            .filter(|(_, h)| !h.carrier)
            .map(|(&id, h)| (at.distance(h.position) - h.radius, id))
            .filter(|&(gap, id)| gap < reach || Some(id) == prefer)
            .collect();
        near.sort_by(|a, b| {
            (Some(b.1) == prefer)
                .cmp(&(Some(a.1) == prefer))
                .then(a.0.total_cmp(&b.0))
                .then(a.1.cmp(&b.1))
        });
        for (_, id) in near.into_iter().take(3) {
            let host = hosts[&id];
            let wanted = (at - host.position).to_angle() - host.angle;
            if let Some(angle) = self.free_anchor(id, host.radius, wanted, size, None) {
                return Some((id, angle));
            }
        }
        None
    }

    /// Clings `body` (not yet added to the world) to `host` at `angle`.
    pub(super) fn root_body(&self, body: &mut Body, host: u64, angle: f32) {
        let Some(rock) = self.body(host) else { return };
        body.root = Some(Root { host, angle });
        body.position = place(rock, angle, body.radius);
        body.velocity = rock.velocity;
        body.angle = rock.angle + angle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Weapon};
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};

    const FAR: Vec2 = Vec2::new(0.0, 2400.0);

    fn rooter(root: f32) -> Species {
        Species::of(Genome {
            radius: 12.0,
            hull: 40.0,
            mass: 6.0,
            root,
            root_defense: 0.8,
            detach_size: 1.0,
            weapon: Weapon::Projectile,
            fire_period: 1.0,
            weapon_range: 600.0,
            sight: 800.0,
            ..Genome::default()
        })
    }

    fn rock_at(game: &mut Game, at: Vec2, radius: f32, velocity: Vec2) -> u64 {
        let id = add(game, BodyKind::Asteroid, at);
        let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        rock.radius = radius;
        rock.mass = radius * 0.6;
        rock.health = radius * 1.6;
        rock.max_health = rock.health;
        rock.velocity = velocity;
        id
    }

    /// A rooter placed on a host at `angle`.
    fn rooted_on(game: &mut Game, species: &Species, host: u64, angle: f32) -> u64 {
        let mut creature = game.make_creature(species, FAR);
        game.root_body(&mut creature, host, angle);
        let id = creature.id;
        game.bodies.push(creature);
        id
    }

    fn young_on(game: &mut Game, species: &Species, host: u64, angle: f32) -> u64 {
        let mut child = game.newborn(
            species.lineage,
            1,
            Phenotype::default(),
            species.genome,
            None,
            FAR,
        );
        game.root_body(&mut child, host, angle);
        let id = child.id;
        game.bodies.push(child);
        id
    }

    fn quiet(game: &mut Game) {
        game.food_clock = 1e9;
    }

    #[test]
    fn habit_comes_from_the_root_gene_and_a_spine_limits_it() {
        let g = |root: f32, segments: u8| Genome {
            root,
            segments,
            ..Genome::default()
        };
        assert_eq!(g(0.0, 1).habit(), Habit::Free);
        assert_eq!(g(0.5, 1).habit(), Habit::Juvenile);
        assert_eq!(g(0.9, 1).habit(), Habit::Life);
        assert_eq!(
            g(0.9, 5).habit(),
            Habit::Juvenile,
            "a spine cannot cling for life"
        );
        // Every authored HOME species is free.
        for s in [
            Species::bogey(),
            Species::lunatic(),
            Species::smarty(),
            Species::fatso(),
            Species::leech(),
        ] {
            assert_eq!(s.genome.habit(), Habit::Free);
        }
    }

    #[test]
    fn a_rooted_creature_rides_a_moving_turning_host() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 60.0, Vec2::new(30.0, 0.0));
        let id = rooted_on(&mut game, &rooter(0.9), host, 0.7);
        let gap = |game: &Game| {
            let (h, c) = (body(game, host), body(game, id));
            h.position.distance(c.position) - h.radius - c.radius * STAND
        };
        let start = body(&game, id).position;
        for _ in 0..180 {
            game.step(DT, Input::default());
        }
        let (h, c) = (body(&game, host), body(&game, id));
        assert!(h.position.x > 80.0, "the host drifts");
        assert!(
            c.position.x > start.x + 50.0 - 70.0,
            "carried along: {:?}",
            c.position
        );
        assert!(
            gap(&game).abs() < 0.5,
            "stays on the surface: {}",
            gap(&game)
        );
        assert!(h.angle > 0.5, "the host turns: {}", h.angle);
        // Its anchor keeps the same bearing in the host's frame.
        let bearing = ((c.position - h.position).to_angle() - h.angle - 0.7).rem_euclid(TAU);
        assert!(bearing.min(TAU - bearing) < 0.02, "bearing {bearing}");
        assert!(c.root.is_some());
        assert_eq!(c.velocity, h.velocity);
    }

    #[test]
    fn rooted_creatures_defend_with_their_weapon_but_only_toward_open_space() {
        let mut game = empty_game();
        quiet(&mut game);
        let host = rock_at(&mut game, FAR, 60.0, Vec2::ZERO);
        // Anchor on the side facing the ship (host frame angle 0 faces +x at host angle 0).
        let facing = rooted_on(&mut game, &rooter(0.9), host, 0.0);
        let behind = rooted_on(&mut game, &rooter(0.9), host, PI);
        game.bodies.iter_mut().find(|b| b.id == host).unwrap().angle = 0.0;
        set_player(&mut game, FAR + Vec2::new(400.0, 0.0), Vec2::ZERO);
        game.player_invulnerability = 0.0;
        let mut shots = 0;
        for _ in 0..240 {
            game.step(DT, Input::default());
            shots = shots.max(game.bullets.iter().filter(|b| !b.friendly).count());
        }
        assert!(shots > 0, "the creature facing the ship fires");
        assert!(body(&game, facing).alert);
        let _ = behind;
        // The one on the far side of the rock holds its fire.
        let mut game2 = empty_game();
        quiet(&mut game2);
        let host = rock_at(&mut game2, FAR, 60.0, Vec2::ZERO);
        let _ = rooted_on(&mut game2, &rooter(0.9), host, PI);
        set_player(&mut game2, FAR + Vec2::new(400.0, 0.0), Vec2::ZERO);
        for _ in 0..240 {
            game2.step(DT, Input::default());
            assert!(
                game2.bullets.iter().all(|b| b.friendly),
                "no shot through its host"
            );
        }
    }

    #[test]
    fn rooted_creatures_sting_on_contact_harder_than_free_ones() {
        let sting = |root: f32| {
            let species = Species::of(Genome {
                contact_damage: 10.0,
                root,
                root_defense: 1.0,
                ..rooter(root).genome
            });
            let mut game = empty_game();
            quiet(&mut game);
            let host = rock_at(&mut game, FAR, 60.0, Vec2::ZERO);
            let id = if root > 0.0 {
                rooted_on(&mut game, &species, host, 0.0)
            } else {
                spawn(&mut game, &species, FAR + Vec2::new(300.0, 0.0))
            };
            contact_damage(body(&game, id))
        };
        assert!(sting(0.9) > 2.0 * sting(0.0));
    }

    #[test]
    fn rooted_creatures_feed_from_the_host_and_grow() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 70.0, Vec2::ZERO);
        let species = Species::of(Genome {
            diet: Diet::Rocks,
            ..rooter(0.5).genome
        });
        let id = young_on(&mut game, &species, host, 0.0);
        game.bodies.iter_mut().find(|b| b.id == id).unwrap().energy = 20.0;
        let (radius, energy) = (body(&game, id).radius, body(&game, id).energy);
        for _ in 0..60 * 20 {
            game.step(DT, Input::default());
        }
        let c = body(&game, id);
        assert!(
            c.energy > energy + 3.0,
            "fed by lichen: {} -> {}",
            energy,
            c.energy
        );
        assert!(c.radius > radius, "grew: {} -> {}", radius, c.radius);
        assert!(c.root.is_some(), "still clinging before maturity");
        // A barren host (crystal) yields nothing.
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let rock = rock_at(&mut game, FAR, 70.0, Vec2::ZERO);
        game.bodies.iter_mut().find(|b| b.id == rock).unwrap().rock = RockKind::Plain;
        let barren = young_on(&mut game, &species, rock, 0.0);
        game.bodies
            .iter_mut()
            .find(|b| b.id == barren)
            .unwrap()
            .energy = 20.0;
        game.bodies.iter_mut().find(|b| b.id == rock).unwrap().rock = RockKind::Crystal;
        // A crystal cannot host: the creature is released rather than fed.
        game.step(DT, Input::default());
        assert!(body(&game, barren).root.is_none());
    }

    #[test]
    fn rooted_juveniles_detach_at_maturity_and_go_free() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 70.0, Vec2::ZERO);
        let species = rooter(0.5);
        let id = young_on(&mut game, &species, host, 0.0);
        assert!(body(&game, id).root.is_some());
        let time = growth::maturity_time(&species.genome);
        for _ in 0..(time * 60.0) as usize + 120 {
            game.step(DT, Input::default());
        }
        let c = body(&game, id);
        assert_eq!(c.adult, None, "matured");
        assert!(c.root.is_none(), "and let go");
        assert!(c.unrooted > 0.0 || c.position.distance(body(&game, host).position) > 70.0);
        // And never clings again as an adult, however close the rock.
        for _ in 0..60 * 10 {
            game.step(DT, Input::default());
        }
        assert!(body(&game, id).root.is_none());
    }

    #[test]
    fn lifelong_rooters_stay_through_maturity_and_the_detach_genes_do_not_apply() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 80.0, Vec2::ZERO);
        let species = Species::of(Genome {
            detach_size: 0.4,
            detach_hunger: 0.5,
            detach_crowd: 0.3,
            ..rooter(0.9).genome
        });
        let id = young_on(&mut game, &species, host, 0.0);
        let time = growth::maturity_time(&species.genome);
        for _ in 0..(time * 60.0) as usize + 120 {
            game.step(DT, Input::default());
        }
        let c = body(&game, id);
        assert_eq!(c.adult, None);
        assert!(c.root.is_some(), "rooted for life");
        assert_eq!(c.radius, species.genome.radius);
    }

    #[test]
    fn young_detach_early_when_big_hungry_or_crowded() {
        let early = |tweak: fn(&mut Genome), setup: fn(&mut Game, u64, u64)| {
            let mut game = empty_game();
            quiet(&mut game);
            set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
            let host = rock_at(&mut game, FAR, 70.0, Vec2::ZERO);
            let mut genome = rooter(0.5).genome;
            tweak(&mut genome);
            let species = Species::of(genome);
            let id = young_on(&mut game, &species, host, 0.0);
            setup(&mut game, host, id);
            game.update_roots(DT);
            body(&game, id).root.is_none()
        };
        // Too big.
        assert!(early(
            |g| g.detach_size = 0.5,
            |game, _, id| game.bodies.iter_mut().find(|b| b.id == id).unwrap().growth = 0.6
        ));
        assert!(!early(
            |g| g.detach_size = 0.9,
            |game, _, id| { game.bodies.iter_mut().find(|b| b.id == id).unwrap().growth = 0.6 }
        ));
        // Hungry at a host that cannot feed it.
        assert!(early(
            |g| {
                g.diet = Diet::Rocks;
                g.detach_hunger = 0.3;
            },
            |game, _, id| {
                let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                b.energy = b.max_energy * 0.1;
            }
        ));
        // Crowded: a host with many neighbors on it.
        assert!(early(
            |g| g.detach_crowd = 0.3,
            |game, host, _| {
                for k in 1..12 {
                    let _ = rooted_on(game, &rooter(0.9), host, k as f32 * 0.5);
                }
            }
        ));
    }

    #[test]
    fn a_destroyed_host_releases_what_clings_to_it_dazed() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 60.0, Vec2::new(20.0, 0.0));
        let a = rooted_on(&mut game, &rooter(0.9), host, 0.0);
        let b = rooted_on(&mut game, &rooter(0.9), host, 2.0);
        game.bodies
            .iter_mut()
            .find(|x| x.id == host)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert!(game.bodies.iter().all(|x| x.id != host), "the rock is gone");
        for id in [a, b] {
            let c = body(&game, id);
            assert!(c.root.is_none(), "released");
            assert!(c.panic > 0.0, "dazed");
            assert!(c.velocity.length() > 20.0, "falls free");
        }
        // Dazed creatures do not fire, and cannot reattach at once.
        assert!(body(&game, a).unrooted > 0.0);
        // A host that simply vanishes (unloaded, eaten) is handled the same way.
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 60.0, Vec2::ZERO);
        let c = rooted_on(&mut game, &rooter(0.9), host, 0.0);
        game.bodies.retain(|x| x.id != host);
        game.step(DT, Input::default());
        assert!(body(&game, c).root.is_none());
    }

    #[test]
    fn a_fallen_lifelong_rooter_finds_another_rock() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let first = rock_at(&mut game, FAR, 50.0, Vec2::ZERO);
        let second = rock_at(&mut game, FAR + Vec2::new(300.0, 0.0), 60.0, Vec2::ZERO);
        let c = rooted_on(&mut game, &rooter(0.9), first, 0.0);
        game.bodies
            .iter_mut()
            .find(|x| x.id == first)
            .unwrap()
            .health = 0.0;
        for _ in 0..60 * 30 {
            game.step(DT, Input::default());
            if body(&game, c).root.is_some_and(|r| r.host == second) {
                return;
            }
        }
        panic!("never reattached: {:?}", body(&game, c).root);
    }

    #[test]
    fn rooters_do_not_collide_with_their_host_or_move_it() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 40.0, Vec2::ZERO);
        let c = rooted_on(&mut game, &rooter(0.9), host, 1.0);
        let at = body(&game, host).position;
        for _ in 0..120 {
            game.step(DT, Input::default());
        }
        assert!(body(&game, host).velocity.length() < 1.0, "host not shoved");
        assert!(body(&game, host).position.distance(at) < 2.0);
        assert!(body(&game, c).root.is_some());
        assert!(!game.bodies.iter().any(|b| b.health <= 0.0));
    }

    #[test]
    fn rooters_are_not_prey_and_can_be_shot_down_for_score_and_loot() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 60.0, Vec2::ZERO);
        let c = rooted_on(&mut game, &rooter(0.9), host, 0.0);
        assert!(!food::huntable(body(&game, c)));
        let before = game.score;
        game.bodies.iter_mut().find(|x| x.id == c).unwrap().health = 0.0;
        game.step(DT, Input::default());
        assert!(game.bodies.iter().all(|x| x.id != c));
        assert!(game.score > before, "scores like any kill");
        // The host survives its tenant.
        assert!(game.bodies.iter().any(|x| x.id == host));
    }

    #[test]
    fn eggs_of_rooting_mothers_are_laid_on_the_rock_and_hatch_rooted() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 60.0, Vec2::new(100.0, 0.0));
        let genome = Genome {
            diet: Diet::Graze,
            birth: crate::genome::Birth::Egg,
            ..rooter(0.5).genome
        };
        let species = Species::of(genome);
        let (site, angle) = game
            .root_site(FAR + Vec2::new(0.0, 80.0), 6.0, None, 200.0)
            .expect("a rock is near");
        assert_eq!(site, host);
        let mut egg = growth::Egg::laid(species.lineage, 1, genome, Phenotype::default());
        egg.set_host(host, angle);
        egg.incubation = 1.0;
        game.eggs.push(egg);
        game.sync_roots();
        let laid = game.eggs[0].position;
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert!(
            game.eggs[0].position.distance(laid) > 20.0,
            "the egg rides the rock"
        );
        assert!(game.eggs[0].position.distance(body(&game, host).position) < 75.0);
        for _ in 0..90 {
            game.step(DT, Input::default());
        }
        assert!(game.eggs.is_empty(), "hatched");
        let child = game
            .bodies
            .iter()
            .find(|b| b.kind == BodyKind::Creature)
            .expect("a hatchling");
        assert!(child.adult.is_some());
        assert_eq!(child.root.map(|r| r.host), Some(host), "and clings");
    }

    #[test]
    fn a_free_species_is_unaffected_by_rocks() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host = rock_at(&mut game, FAR, 60.0, Vec2::ZERO);
        let species = rooter(0.0);
        let c = spawn(&mut game, &species, FAR + Vec2::new(0.0, 80.0));
        for _ in 0..60 * 10 {
            game.step(DT, Input::default());
            assert!(body(&game, c).root.is_none());
        }
        assert!(game.bodies.iter().any(|b| b.id == host));
    }

    #[test]
    fn rooted_spawns_are_deterministic_scale_with_the_host_and_stay_in_budget() {
        let mut planets = 0;
        let mut rooted_total = 0;
        // The opening rings hold no rooters; from ring three they live on rocks and planetoids.
        for x in 3..24 {
            for y in -6..6 {
                let id = SectorId { x, y };
                let a = world::generate(42, id);
                assert_eq!(a, world::generate(42, id), "deterministic");
                // A rooter is one body while it clings.
                let rooted = a.iter().filter(|s| s.rooted.is_some()).count() as u32;
                let original: u32 = a
                    .iter()
                    .filter(|s| s.rooted.is_none())
                    .filter_map(|s| s.species)
                    .map(|s| s.genome.parts())
                    .sum();
                // Rooted residents only ever fill what the original population left of the
                // sector's creature budget.
                assert!(
                    original + rooted <= world::SECTOR_BODY_BUDGET.max(original),
                    "budget: {original} + {rooted}"
                );
                for (i, s) in a.iter().enumerate() {
                    assert_eq!(s.index as usize, i);
                    let Some(r) = s.rooted else { continue };
                    let host = &a[r.host as usize];
                    if host.apex.is_some() {
                        // An elder's resident: covered by the host-slot tests below.
                        continue;
                    }
                    assert!(r.host < s.index && host.kind == BodyKind::Asteroid);
                    assert!(
                        !matches!(host.rock, RockKind::Crystal | RockKind::Husk),
                        "no crystal or husk hosts"
                    );
                    assert!(
                        !host.pinned || host.rock == RockKind::Planetoid,
                        "no nest stones"
                    );
                    let size = s.species.unwrap().genome.radius;
                    assert!(size <= host.radius.unwrap() * 0.55 + 1e-3);
                    assert!(s.species.unwrap().genome.habit() != Habit::Free);
                    rooted_total += 1;
                }
                if let Some(planet) = a.iter().position(|s| s.rock == RockKind::Planetoid) {
                    planets += 1;
                    let n = a
                        .iter()
                        .filter(|s| s.rooted.is_some_and(|r| r.host as usize == planet))
                        .count();
                    let used: u32 = a
                        .iter()
                        .filter(|s| s.rooted.is_none())
                        .filter_map(|s| s.species)
                        .map(|s| s.genome.parts())
                        .sum();
                    let lived_in = !crate::range::ecology(42, id).presence.is_empty();
                    if used < world::SECTOR_BODY_BUDGET && lived_in {
                        assert!(n >= 1, "every planetoid with room has a community: {id:?}");
                    }
                }
            }
        }
        assert!(
            planets > 20 && rooted_total > 100,
            "{planets} {rooted_total}"
        );
        // HOME is untouched.
        assert!(
            world::generate(42, SectorId::ORIGIN)
                .iter()
                .all(|s| s.rooted.is_none())
        );
    }

    #[test]
    fn a_planetoid_community_loads_attached_and_the_world_stays_under_the_caps() {
        // Find a nearby sector with a planetoid and go there.
        let (id, planet) = (1..30)
            .flat_map(|x| (-5..5).map(move |y| SectorId { x, y }))
            .find_map(|id| {
                let a = world::generate(42, id);
                let p = a.iter().position(|s| s.rock == RockKind::Planetoid)?;
                (a.iter()
                    .filter(|s| s.rooted.is_some_and(|r| r.host as usize == p))
                    .count()
                    >= 4)
                    .then_some((id, p))
            })
            .expect("a populated planetoid exists");
        let spawns = world::generate(42, id);
        let mut game = Game::new(42);
        game.teleport(
            spawns[planet].position + Vec2::new(0.0, spawns[planet].radius.unwrap() + 300.0),
        );
        for _ in 0..120 {
            game.step(DT, Input::default());
        }
        let host = game
            .bodies
            .iter()
            .find(|b| b.origin == Some((id, planet as u32)))
            .expect("planetoid loaded");
        let clingers = game
            .bodies
            .iter()
            .filter(|b| b.root.is_some_and(|r| r.host == host.id))
            .count();
        assert!(clingers >= 3, "{clingers} residents");
        assert!(game.bodies.len() < MAX_BODIES);
        for b in game.bodies.iter().filter(|b| b.root.is_some()) {
            let h = game.body(b.root.unwrap().host).unwrap();
            let gap = b.position.distance(h.position) - h.radius - b.radius * STAND;
            assert!(gap.abs() < 0.5, "{gap}");
        }
        // The planetoid is still indestructible with tenants on it.
        let planet_id = host.id;
        game.bodies
            .iter_mut()
            .find(|b| b.id == planet_id)
            .unwrap()
            .health = 0.0;
        // damage() ignores planetoids; only direct health writes could break it, which never
        // happens in play. Restore and make sure shots do nothing.
        game.bodies
            .iter_mut()
            .find(|b| b.id == planet_id)
            .unwrap()
            .health = 1000.0;
        let mut shot = Bullet::friendly(game.body(planet_id).unwrap().position, Vec2::ZERO, 1.0);
        shot.damage = 1e6;
        game.bullets.push(shot);
        game.step(DT, Input::default());
        assert!(game.body(planet_id).unwrap().health > 0.0);
    }

    #[test]
    fn rooting_never_breaks_when_unloaded_and_reloaded() {
        // Walk away from a populated planetoid and back: no panic, no dangling roots.
        let mut game = Game::new(7);
        let mut seen = false;
        for k in 0..6 {
            game.teleport(Vec2::new(
                6000.0 * (k % 3) as f32 + 3000.0,
                6000.0 * (k / 3) as f32,
            ));
            for _ in 0..90 {
                game.step(DT, Input::default());
            }
            for b in &game.bodies {
                if let Some(r) = b.root {
                    seen = true;
                    assert!(game.body(r.host).is_some(), "no dangling host");
                }
            }
        }
        assert!(seen, "met some rooted life");
    }

    /// A hosted apex with its residents: the sector, the host's spawn index.
    fn hosted_sector() -> (SectorId, usize) {
        (-14..=14)
            .flat_map(|x| (-14..=14).map(move |y| SectorId { x, y }))
            .find_map(|id| {
                let a = world::generate(42, id);
                let p = a.iter().position(|s| {
                    s.apex.is_some() && s.species.is_some_and(|sp| sp.genome.hosted.is_some())
                })?;
                a.iter()
                    .any(|s| s.rooted.is_some_and(|r| r.host as usize == p))
                    .then_some((id, p))
            })
            .expect("a hosted elder exists nearby")
    }

    #[test]
    fn hosted_elders_are_deterministic_capped_and_inside_the_budget() {
        let mut hosts = 0;
        for seed in [1_u64, 42, 7] {
            for x in -14..=14 {
                for y in -14..=14 {
                    let id = SectorId { x, y };
                    let a = world::generate(seed, id);
                    assert_eq!(a, world::generate(seed, id), "deterministic");
                    for (p, host) in a.iter().enumerate() {
                        let Some(hosted) = host.species.and_then(|sp| sp.genome.hosted) else {
                            continue;
                        };
                        hosts += 1;
                        let kids: Vec<_> = a
                            .iter()
                            .filter(|s| s.rooted.is_some_and(|r| r.host as usize == p))
                            .collect();
                        assert!(
                            kids.len() <= usize::from(crate::hosted::MAX_RESIDENTS),
                            "cap"
                        );
                        assert!(kids.len() <= usize::from(hosted.count));
                        for k in &kids {
                            assert!(k.index as usize > p, "residents follow their host");
                            assert_eq!(k.species.unwrap().genome.parts(), 1);
                            assert!(k.species.unwrap().genome.hosted.is_none());
                            assert_eq!(k.species.unwrap().genome.learner, 0.0);
                        }
                        assert!(world::bodies_used(&a) <= world::SECTOR_BODY_BUDGET.max(1));
                    }
                }
            }
        }
        assert!(hosts > 5, "some elders carry residents: {hosts}");
        // HOME never does.
        assert!(
            world::generate(42, SectorId::ORIGIN)
                .iter()
                .all(|s| s.species.is_none_or(|sp| sp.genome.hosted.is_none()))
        );
    }

    #[test]
    fn residents_load_attached_to_their_elder_and_are_released_when_it_dies_or_unloads() {
        let (id, p) = hosted_sector();
        let spawns = world::generate(42, id);
        let mut game = Game::new(42);
        game.teleport(spawns[p].position + Vec2::new(0.0, 900.0));
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        let host = game
            .bodies
            .iter()
            .find(|b| b.origin == Some((id, p as u32)))
            .expect("elder loaded")
            .id;
        let riders = |game: &Game| -> Vec<u64> {
            game.bodies
                .iter()
                .filter(|b| b.root.is_some_and(|r| r.host == host))
                .map(|b| b.id)
                .collect()
        };
        let ids = riders(&game);
        assert!(!ids.is_empty(), "residents ride the elder");
        assert!(game.bodies.len() < MAX_BODIES);
        // Free creatures never take hold of an elder: nothing but residents is on it.
        for &r in &ids {
            let (c, h) = (body(&game, r), body(&game, host));
            let gap = c.position.distance(h.position) - h.radius - c.radius * STAND;
            assert!(gap.abs() < 1.0, "seated on the rim: {gap}");
        }
        // Death: the next tick nothing dangles.
        let mut dead = Game::new(42);
        dead.teleport(spawns[p].position + Vec2::new(0.0, 900.0));
        for _ in 0..30 {
            dead.step(DT, Input::default());
        }
        assert_eq!(riders(&dead), ids, "loading is deterministic");
        dead.bodies
            .iter_mut()
            .find(|b| b.id == host)
            .unwrap()
            .health = 0.0;
        dead.bodies.retain(|b| b.id != host);
        dead.step(DT, Input::default());
        for &r in &ids {
            if let Some(c) = dead.bodies.iter().find(|b| b.id == r) {
                assert!(c.root.is_none(), "released");
            }
        }
        // Unload: walk far away; no root ever points at a missing body.
        for k in 1..=8 {
            game.teleport(Vec2::new(6000.0 * 3.0 * k as f32, 0.0) + spawns[p].position);
            for _ in 0..60 {
                game.step(DT, Input::default());
            }
            for b in &game.bodies {
                if let Some(r) = b.root {
                    assert!(game.body(r.host).is_some(), "no dangling host");
                }
            }
        }
    }

    #[test]
    fn residents_ride_a_moving_carrier_and_free_creatures_never_take_hold_of_it() {
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host_species = Species::of(Genome {
            radius: 40.0,
            hull: 300.0,
            mass: 100.0,
            hosted: Some(crate::hosted::Hosted {
                count: 2,
                relation: crate::hosted::Relation::Brood,
            }),
            ..Genome::default()
        });
        let mut host = game.make_creature(&host_species, FAR);
        host.velocity = Vec2::new(30.0, 0.0);
        let host_id = host.id;
        game.bodies.push(host);
        let kind = Species::of(crate::hosted::resident(
            &host_species.genome,
            crate::hosted::Relation::Brood,
        ));
        let a = rooted_on(&mut game, &kind, host_id, 0.5);
        // A free rooting-capable creature touching the elder does not cling to it.
        let mut drifter = game.make_creature(&rooter(0.9), FAR + Vec2::new(60.0, 0.0));
        drifter.velocity = Vec2::ZERO;
        let drifter = {
            let id = drifter.id;
            game.bodies.push(drifter);
            id
        };
        for _ in 0..120 {
            game.step(DT, Input::default());
        }
        let (h, c) = (body(&game, host_id), body(&game, a));
        let gap = c.position.distance(h.position) - h.radius - c.radius * STAND;
        assert!(gap.abs() < 0.5, "rides the surface: {gap}");
        assert!(c.root.is_some_and(|r| r.host == host_id));
        assert!(
            body(&game, drifter).root.is_none_or(|r| r.host != host_id),
            "an elder is not a rock"
        );
    }

    #[test]
    fn a_brood_swarms_when_its_host_is_lost_and_other_residents_scatter() {
        use crate::hosted::{Hosted, Relation, resident};
        let mut game = empty_game();
        quiet(&mut game);
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        let host_species = Species::of(Genome {
            radius: 40.0,
            hull: 300.0,
            mass: 100.0,
            hosted: Some(Hosted {
                count: 2,
                relation: Relation::Brood,
            }),
            ..Genome::default()
        });
        let host = game.make_creature(&host_species, FAR);
        let host_id = host.id;
        game.bodies.push(host);
        let brood = Species::of(resident(&host_species.genome, Relation::Brood));
        let friend = Species::of(resident(&host_species.genome, Relation::Symbiote));
        let b = rooted_on(&mut game, &brood, host_id, 0.5);
        let f = rooted_on(&mut game, &friend, host_id, 2.5);
        game.step(DT, Input::default());
        assert_eq!(body(&game, b).provoked, 0.0, "calm while carried");
        game.bodies.retain(|x| x.id != host_id);
        game.step(DT, Input::default());
        let (b, f) = (body(&game, b), body(&game, f));
        assert!(b.root.is_none() && f.root.is_none(), "both released");
        assert!(b.provoked > 0.0 && b.panic == 0.0, "the brood swarms");
        assert!(f.provoked == 0.0 && f.panic > 0.0, "the symbiote scatters");
    }

    #[test]
    fn residents_of_a_slain_elder_do_not_return_when_its_sector_reloads() {
        let (id, p) = hosted_sector();
        let spawns = world::generate(42, id);
        let resident_indices: Vec<u32> = spawns
            .iter()
            .filter(|s| s.rooted.is_some_and(|r| r.host as usize == p))
            .map(|s| s.index)
            .collect();
        let near = spawns[p].position + Vec2::new(0.0, 900.0);
        let here = |game: &Game| {
            game.bodies
                .iter()
                .filter(|b| {
                    b.origin
                        .is_some_and(|(s, i)| s == id && resident_indices.contains(&i))
                })
                .count()
        };
        let mut game = Game::new(42);
        game.teleport(near);
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert!(here(&game) > 0, "residents load with a living elder");
        game.bodies
            .iter_mut()
            .find(|b| b.origin == Some((id, p as u32)))
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        game.teleport(Vec2::new(6000.0 * 12.0, 0.0) + near);
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert_eq!(here(&game), 0, "unloaded");
        game.teleport(near);
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert_eq!(here(&game), 0, "the slain elder's residents stay gone");
    }
}
