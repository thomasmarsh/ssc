//! Seamer doorways. Placement hashes never consume a simulation or generation stream.
use super::powers::PowerState;
use super::*;
use crate::power::Power;

pub const RADIUS: f32 = 70.0;
pub const WARNING: f32 = 1.2;
pub const LIFE: f32 = 8.0;
pub const GRACE: f32 = 0.6;
pub const SECTOR_CAP: usize = 2;
pub const GLOBAL_CAP: usize = 8;

/// Shared genes retain their ranges; Rift expresses them in its own units.
pub fn period(g: &Genome) -> f32 {
    12.0 + 18.0 * ((g.power_params(Power::Rift).period - 1.5) / 12.5).clamp(0.0, 1.0)
}
pub fn separation(g: &Genome) -> f32 {
    900.0 + 600.0 * ((g.power_params(Power::Rift).reach - 80.0) / 820.0).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rift {
    pub owner: u64,
    pub a: Vec2,
    pub b: Vec2,
    /// Warning counts down first, then eight seconds of usability.
    pub warning: f32,
    pub left: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiftTrace {
    pub from: Vec2,
    pub to: Vec2,
    pub left: f32,
}

/// Earliest outside-to-inside center crossing, in stable pair and mouth order.
/// The doorway translates, never rotates. Residual travel is spent at the exit.
pub(super) fn crossing(
    rifts: &[Rift],
    start: Vec2,
    end: Vec2,
    radius: f32,
) -> Option<(f32, Vec2, Vec2)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let dir = (end - start).try_normalize()?;
    let mut best = None;
    for r in rifts.iter().filter(|r| r.warning <= 0.0 && r.left > 0.0) {
        for (from, to) in [(r.a, r.b), (r.b, r.a)] {
            if start.distance_squared(from) <= RADIUS * RADIUS {
                continue;
            }
            if let Some(t) = segment_circle(start, end, from, RADIUS)
                && best.is_none_or(|(prior, _, _)| t < prior)
            {
                let entry = start.lerp(end, t);
                let lateral = entry - from - dir * (entry - from).dot(dir);
                let exit = to + lateral + dir * (RADIUS + radius + 8.0);
                best = Some((t, entry, exit));
            }
        }
    }
    best
}

/// All tangible bodies obstruct arrivals, including free rocks and other creatures.
pub(super) fn clear_path(
    bodies: &[Body],
    active: &[SectorId],
    start: Vec2,
    end: Vec2,
    radius: f32,
    excluded: &[u64],
) -> bool {
    start.is_finite()
        && end.is_finite()
        && active.contains(&SectorId::containing(start))
        && active.contains(&SectorId::containing(end))
        && !bodies.iter().any(|b| {
            b.active
                && b.health > 0.0
                && !b.consumed
                && !b.phased
                && !excluded.contains(&b.id)
                && segment_circle(start, end, b.position, radius + hit_radius(b) + 4.0).is_some()
        })
}

impl Game {
    fn rift_owner_live(b: &Body) -> bool {
        b.kind == BodyKind::Creature
            && b.active
            && b.health > 0.0
            && !b.consumed
            && !b.follower
            && b.position.is_finite()
            && Power::Rift.active(&b.genome)
    }
    pub(super) fn cleanup_rifts(&mut self) {
        let bodies = &self.bodies;
        let active = &self.active;
        self.rifts.retain(|r| {
            r.left > 0.0
                && r.a.is_finite()
                && r.b.is_finite()
                && active.contains(&SectorId::containing(r.a))
                && active.contains(&SectorId::containing(r.b))
                && bodies
                    .iter()
                    .any(|b| b.id == r.owner && Self::rift_owner_live(b))
        });
        self.rift_traces.retain(|t| {
            t.left > 0.0
                && active.contains(&SectorId::containing(t.from))
                && active.contains(&SectorId::containing(t.to))
        });
    }
    pub(super) fn update_rifts(&mut self, dt: f32) {
        self.cleanup_rifts();
        let mut cues = Vec::new();
        for r in &mut self.rifts {
            if r.warning > 0.0 {
                let before = r.warning;
                r.warning = (before - dt).max(0.0);
                // The full placement warning has elapsed before this activation cue.
                if r.warning == 0.0 {
                    cues.extend([Cue::RiftOpen { at: r.a }, Cue::RiftOpen { at: r.b }]);
                }
            } else {
                r.left -= dt;
            }
        }
        for t in &mut self.rift_traces {
            t.left -= dt;
        }
        for c in cues {
            self.cue(c);
        }
        self.cleanup_rifts();
    }
    pub(super) fn step_rift(&mut self, index: usize, state: &mut PowerState, dt: f32) {
        state.rift_clock -= dt;
        if state.rift_clock > 0.0 {
            return;
        }
        let owner = &self.bodies[index];
        let (id, at, g) = (owner.id, owner.position, owner.genome);
        state.rift_clock = period(&g);
        let cast = state.rift_casts;
        state.rift_casts = state.rift_casts.wrapping_add(1);
        if !Self::rift_owner_live(owner)
            || owner.phased
            || owner.contact_cooldown > 0.0
            || self.sanctuary && self.sector() == SectorId::ORIGIN
        {
            return;
        }
        self.cleanup_rifts();
        if self.rifts.len() >= GLOBAL_CAP || self.rifts.iter().any(|r| r.owner == id) {
            return;
        }
        let sector = SectorId::containing(at);
        if self
            .bodies
            .iter()
            .filter(|b| {
                Self::rift_owner_live(b) && b.id < id && SectorId::containing(b.position) == sector
            })
            .count()
            >= 2
        {
            return;
        }
        let distance = separation(&g);
        for k in 0..16 {
            let hash = world::hash2(self.seed ^ id ^ 0x5345_414D, cast as i32, k);
            let angle = (hash >> 40) as f32 / 16_777_216.0 * TAU;
            let axis = Vec2::from_angle(angle);
            let middle = at + axis.perp() * (180.0 + (k % 4) as f32 * 55.0);
            let (a, b) = (
                middle - axis * distance * 0.5,
                middle + axis * distance * 0.5,
            );
            if [a, b].iter().any(|&p| {
                !self.active.contains(&SectorId::containing(p))
                    || self.bodies.iter().any(|body| {
                        body.active
                            && is_fixed(body)
                            && body.position.distance(p) < body.radius + RADIUS + 100.0
                    })
                    || self.player().is_some_and(|ship| {
                        ship.position.distance(p) < RADIUS + ship.radius + 100.0
                    })
                    || self
                        .rifts
                        .iter()
                        .any(|r| r.a.distance(p) < 250.0 || r.b.distance(p) < 250.0)
                    || self
                        .rifts
                        .iter()
                        .filter(|r| {
                            SectorId::containing(r.a) == SectorId::containing(p)
                                || SectorId::containing(r.b) == SectorId::containing(p)
                        })
                        .count()
                        >= SECTOR_CAP
            }) {
                continue;
            }
            self.rifts.push(Rift {
                owner: id,
                a,
                b,
                warning: WARNING,
                left: LIFE,
            });
            self.cue(Cue::RiftTell { at: a });
            self.cue(Cue::RiftTell { at: b });
            return;
        }
    }
    pub(super) fn rift_trace(&mut self, from: Vec2, to: Vec2) {
        if self.rift_traces.len() < 32 {
            self.rift_traces.push(RiftTrace {
                from,
                to,
                left: 0.35,
            });
        }
        self.cue(Cue::RiftTransit { at: to });
    }
    /// Called after integration, before ordinary contacts. A whole chain or host travels.
    pub(super) fn transit_bodies(&mut self, before: &[(u64, Vec2)]) {
        self.cleanup_rifts();
        if self.rifts.is_empty() {
            return;
        }
        for &(id, start) in before {
            let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                continue;
            };
            let body = &self.bodies[index];
            if !body.active
                || body.phased
                || body.health <= 0.0
                || body.consumed
                || is_fixed(body)
                || body.follower
                || body.latch.is_some()
                || body.rift_grace > 0.0
                || body.kind == BodyKind::Player && self.is_landed()
            {
                continue;
            }
            let end = body.position;
            let Some((fraction, entry, exit)) = crossing(&self.rifts, start, end, body.radius)
            else {
                continue;
            };
            let chain = body.chain;
            let mut group: Vec<u64> = self
                .bodies
                .iter()
                .filter(|b| b.id == id || chain.is_some() && b.chain == chain)
                .map(|b| b.id)
                .collect();
            // Host passengers ride with their host, preserving roots and parasite latches.
            for _ in 0..28 {
                let prior = group.len();
                for b in &self.bodies {
                    if !group.contains(&b.id)
                        && (b.root.is_some_and(|r| group.contains(&r.host))
                            || b.latch.is_some_and(|host| group.contains(&host))
                            || b.chain.is_some_and(|chain| {
                                self.bodies.iter().any(|member| {
                                    member.chain == Some(chain) && group.contains(&member.id)
                                })
                            }))
                    {
                        group.push(b.id);
                    }
                }
                if prior == group.len() {
                    break;
                }
            }
            let shift = exit - entry;
            let safe = self
                .bodies
                .iter()
                .filter(|b| group.contains(&b.id))
                .all(|b| {
                    b.active
                        && b.health > 0.0
                        && !b.consumed
                        && !b.phased
                        && !b.pinned
                        && b.rift_grace <= 0.0
                        && clear_path(
                            &self.bodies,
                            &self.active,
                            b.position + shift - (end - entry),
                            b.position + shift,
                            b.radius,
                            &group,
                        )
                });
            // Solids before the mouth win over transit, even at very high speed.
            let prefix_blocked = self
                .bodies
                .iter()
                .filter(|b| group.contains(&b.id))
                .any(|part| {
                    let from = before
                        .iter()
                        .find(|(id, _)| *id == part.id)
                        .map_or(part.position, |(_, p)| *p);
                    let to = from.lerp(part.position, fraction);
                    self.bodies.iter().any(|obstacle| {
                        obstacle.active
                            && is_fixed(obstacle)
                            && !group.contains(&obstacle.id)
                            && segment_circle(
                                from,
                                to,
                                obstacle.position,
                                obstacle.radius + part.radius,
                            )
                            .is_some()
                    })
                });
            if !safe || prefix_blocked {
                continue;
            }
            for b in self.bodies.iter_mut().filter(|b| group.contains(&b.id)) {
                b.position += shift;
                b.rift_grace = GRACE;
                if b.kind == BodyKind::Asteroid
                    && b.shoved <= 0.0
                    && b.sling_thrown <= 0.0
                    && b.rune_pushed <= 0.0
                {
                    b.rift_redirected = 5.0;
                }
            }
            self.tethers.retain(|t| {
                let owner = group.contains(&t.owner);
                let other = t.other.is_some_and(|other| group.contains(&other));
                if t.kind == TetherKind::Latch {
                    !owner
                        && !group.iter().any(|id| {
                            self.bodies
                                .iter()
                                .any(|b| b.id == *id && b.kind == BodyKind::Player)
                        })
                } else {
                    owner == other
                }
            });
            if self
                .beam
                .as_ref()
                .is_some_and(|b| group.contains(&b.target))
                || self.bodies[index].kind == BodyKind::Player
            {
                self.beam = None;
                self.gripped = None;
            }
            self.rift_trace(entry, exit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Species;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    fn arena(warning: f32) -> (Game, u64) {
        let mut g = empty_game();
        set_player(&mut g, Vec2::new(0.0, -600.0), Vec2::ZERO);
        let owner = spawn(
            &mut g,
            &Species::of(Genome::seamer()),
            Vec2::new(500.0, 400.0),
        );
        g.bodies.iter_mut().find(|b| b.id == owner).unwrap().pinned = true;
        g.rifts.push(Rift {
            owner,
            a: Vec2::ZERO,
            b: Vec2::X * 1200.0,
            warning,
            left: LIFE,
        });
        g.drain_cues();
        (g, owner)
    }
    fn move_body(g: &mut Game, id: u64, start: Vec2, end: Vec2, velocity: Vec2) {
        let b = g.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.position = end;
        b.velocity = velocity;
        g.transit_bodies(&[(id, start)]);
    }
    #[test]
    fn specimen_maps_shared_ranges_to_authored_units_without_widening_genes() {
        let g = Genome::seamer();
        assert!((period(&g) - 20.0).abs() < 0.001);
        assert_eq!(separation(&g), 1200.0);
        assert_eq!(g.limited(), g);
        assert!(Power::Rift.built());
        assert_eq!(Power::Rift.first_ring(), 10);
        for (p, r, want_p, want_r) in [(1.5, 80.0, 12.0, 900.0), (14.0, 900.0, 30.0, 1500.0)] {
            let g = Genome {
                power_params: crate::power::params_for(crate::power::Power::Rift, p, r, 1.0),
                ..g
            };
            assert_eq!(period(&g), want_p);
            assert_eq!(separation(&g), want_r);
        }
    }
    #[test]
    fn placement_is_clear_spaced_rng_free_and_repeatable() {
        let trace = || {
            let (mut g, owner) = arena(0.0);
            g.rifts.clear();
            let i = g.bodies.iter().position(|b| b.id == owner).unwrap();
            let rock = add(&mut g, BodyKind::Asteroid, Vec2::new(700.0, 600.0));
            g.bodies.iter_mut().find(|b| b.id == rock).unwrap().pinned = true;
            let rng = g.rng.clone().f32();
            let variation = g.variation.clone().f32();
            let mut state = PowerState::default();
            g.step_rift(i, &mut state, DT);
            assert_eq!(g.rifts.len(), 1);
            let r = g.rifts[0];
            assert!((r.a.distance(r.b) - 1200.0).abs() < 0.01);
            assert!(
                r.a.distance(body(&g, rock).position) >= body(&g, rock).radius + RADIUS + 100.0
            );
            assert!(
                r.b.distance(body(&g, rock).position) >= body(&g, rock).radius + RADIUS + 100.0
            );
            assert_eq!(g.rng.f32(), rng);
            assert_eq!(g.variation.f32(), variation);
            assert_eq!(
                g.drain_cues()
                    .iter()
                    .filter(|c| matches!(c, Cue::RiftTell { .. }))
                    .count(),
                2
            );
            r
        };
        assert_eq!(trace(), trace());
    }
    #[test]
    fn full_warning_then_activation_and_eight_seconds_of_use() {
        let (mut g, _) = arena(WARNING);
        for _ in 0..71 {
            g.update_rifts(DT);
        }
        assert!(g.rifts[0].warning > 0.0);
        assert_eq!(g.rifts[0].left, LIFE);
        assert!(crossing(&g.rifts, -Vec2::X * 100.0, Vec2::X * 100.0, 3.0).is_none());
        g.update_rifts(2.0 * DT);
        assert_eq!(g.rifts[0].warning, 0.0);
        assert_eq!(
            g.drain_cues()
                .iter()
                .filter(|c| matches!(c, Cue::RiftOpen { .. }))
                .count(),
            2
        );
        g.update_rifts(7.9);
        assert_eq!(g.rifts.len(), 1);
        g.update_rifts(0.11);
        assert!(g.rifts.is_empty());
    }
    #[test]
    fn one_pair_per_owner_refuses_an_overlapping_cast_without_replacing_it() {
        let (mut g, owner) = arena(0.0);
        let old = g.rifts[0];
        let i = g.bodies.iter().position(|b| b.id == owner).unwrap();
        let mut s = PowerState::default();
        g.step_rift(i, &mut s, DT);
        assert_eq!(g.rifts, vec![old]);
        g.update_rifts(LIFE);
        s.rift_clock = 0.0;
        g.step_rift(i, &mut s, DT);
        assert_eq!(g.rifts.len(), 1);
        assert!(g.rifts[0].warning > 0.0);
    }
    #[test]
    fn ship_creature_and_free_rock_transit_both_ways_preserving_velocity_and_heading() {
        for kind in [BodyKind::Player, BodyKind::Creature, BodyKind::Asteroid] {
            for reverse in [false, true] {
                let (mut g, _) = arena(0.0);
                let center = if reverse {
                    Vec2::X * 1200.0
                } else {
                    Vec2::ZERO
                };
                let id = if kind == BodyKind::Player {
                    g.player().unwrap().id
                } else {
                    add(&mut g, kind, center - Vec2::X * 200.0)
                };
                let angle = body(&g, id).angle;
                let v = Vec2::new(12000.0, 75.0);
                move_body(
                    &mut g,
                    id,
                    center - Vec2::X * 200.0,
                    center + Vec2::X * 200.0,
                    v,
                );
                assert_eq!(body(&g, id).velocity, v);
                assert_eq!(body(&g, id).angle, angle);
                assert_eq!(body(&g, id).rift_grace, GRACE);
                assert!(if reverse {
                    body(&g, id).position.x < 500.0
                } else {
                    body(&g, id).position.x > 1400.0
                });
            }
        }
    }
    #[test]
    fn fast_bullets_cross_without_skipping_and_keep_range_pith_and_allegiance() {
        for friendly in [false, true] {
            for reverse in [false, true] {
                let (mut g, _) = arena(0.0);
                let center = if reverse {
                    Vec2::X * 1200.0
                } else {
                    Vec2::ZERO
                };
                let mut shot =
                    Bullet::hostile(center - Vec2::X * 200.0, Vec2::X * 24000.0, 3.0, 10.0);
                shot.friendly = friendly;
                shot.pith = 0.65;
                shot.pierce = 2;
                g.bullets.push(shot);
                g.move_bullets(DT);
                let b = &g.bullets[0];
                assert_eq!(b.velocity, Vec2::X * 24000.0);
                assert_eq!(b.friendly, friendly);
                assert_eq!(b.pith, 0.65);
                assert_eq!(b.remaining, 3.0 - DT);
                assert_eq!(b.pierce, 2);
                // Translation of origin excludes the seam distance from falloff and bubbles.
                assert!((b.position.distance(b.origin) - 400.0).abs() < 0.01);
                assert_eq!(b.rift_grace, GRACE);
                assert_eq!(g.rift_traces.len(), 1);
            }
        }
    }
    #[test]
    fn walls_planetoids_stations_and_free_obstacles_refuse_blocked_body_exits() {
        for kind in [0, 1, 2, 3] {
            let (mut g, _) = arena(0.0);
            let obstacle = add(
                &mut g,
                if kind == 2 {
                    BodyKind::Base
                } else {
                    BodyKind::Asteroid
                },
                Vec2::new(1300.0, 0.0),
            );
            let b = g.bodies.iter_mut().find(|b| b.id == obstacle).unwrap();
            b.radius = 50.0;
            b.pinned = kind != 3;
            b.rock = if kind == 1 {
                RockKind::Planetoid
            } else if kind == 0 {
                RockKind::Wall
            } else {
                RockKind::Plain
            };
            let id = g.player().unwrap().id;
            move_body(
                &mut g,
                id,
                -Vec2::X * 100.0,
                Vec2::X * 10.0,
                Vec2::X * 600.0,
            );
            assert_eq!(body(&g, id).position, Vec2::X * 10.0);
            assert_eq!(body(&g, id).rift_grace, 0.0);
            assert_eq!(body(&g, id).velocity, Vec2::X * 600.0);
        }
    }
    #[test]
    fn blocked_bullet_exit_refuses_without_spending_the_shot() {
        let (mut g, _) = arena(0.0);
        let wall = add(&mut g, BodyKind::Asteroid, Vec2::new(1285.0, 0.0));
        let b = g.bodies.iter_mut().find(|b| b.id == wall).unwrap();
        b.pinned = true;
        b.rock = RockKind::Wall;
        g.bullets
            .push(Bullet::friendly(-Vec2::X * 100.0, Vec2::X * 12000.0, 1.0));
        g.move_bullets(DT);
        assert!(g.bullets[0].position.distance(Vec2::X * 100.0) < 0.001);
        assert_eq!(g.bullets[0].rift_grace, 0.0);
        assert!(g.rift_traces.is_empty());
    }
    #[test]
    fn obstacles_before_the_entrance_win_and_exit_leg_hits_use_ordinary_rules() {
        for after in [false, true] {
            let (mut g, _) = arena(0.0);
            let victim = spawn(
                &mut g,
                &Species::bogey(),
                Vec2::new(if after { 1410.0 } else { -125.0 }, 0.0),
            );
            g.bodies.iter_mut().find(|b| b.id == victim).unwrap().health = 1.0;
            g.bodies.iter_mut().find(|b| b.id == victim).unwrap().shield = 0.0;
            g.bullets
                .push(Bullet::friendly(-Vec2::X * 200.0, Vec2::X * 24000.0, 1.0));
            g.move_bullets(DT);
            assert!(body(&g, victim).health <= 0.0);
            assert_eq!(!g.rift_traces.is_empty(), after);
            g.remove_destroyed();
            assert_eq!(g.run.kills, 1);
        }
    }
    #[test]
    fn grace_is_shared_across_pairs_and_requires_a_new_entrance() {
        let (mut g, _) = arena(0.0);
        let id = g.player().unwrap().id;
        move_body(
            &mut g,
            id,
            -Vec2::X * 100.0,
            Vec2::X * 10.0,
            Vec2::X * 400.0,
        );
        assert_eq!(g.rift_traces.len(), 1);
        // Turning back through either mouth cannot bounce during the grace.
        move_body(
            &mut g,
            id,
            Vec2::X * 1400.0,
            Vec2::X * 1200.0,
            -Vec2::X * 400.0,
        );
        assert_eq!(body(&g, id).position.x, 1200.0);
        g.bodies.iter_mut().find(|b| b.id == id).unwrap().rift_grace = 0.0;
        move_body(
            &mut g,
            id,
            Vec2::X * 1200.0,
            Vec2::X * 1210.0,
            Vec2::X * 400.0,
        );
        assert_eq!(g.rift_traces.len(), 1, "overlap is not a new entrance");
        move_body(
            &mut g,
            id,
            Vec2::X * 1400.0,
            Vec2::X * 1200.0,
            -Vec2::X * 400.0,
        );
        assert_eq!(g.rift_traces.len(), 2);
    }
    #[test]
    fn bullets_do_not_ping_pong_and_grace_expires_after_point_six_seconds() {
        let (mut g, _) = arena(0.0);
        g.bullets
            .push(Bullet::friendly(-Vec2::X * 100.0, Vec2::X * 12000.0, 3.0));
        g.move_bullets(DT);
        for _ in 0..35 {
            g.bullets[0].position = Vec2::X * 1400.0;
            g.bullets[0].velocity = -Vec2::X * 12000.0;
            g.move_bullets(DT);
            assert!(g.bullets[0].position.x > 1000.0);
        }
        g.bullets[0].position = Vec2::X * 1400.0;
        g.move_bullets(2.0 * DT);
        assert!(g.bullets[0].position.x < 0.0);
    }
    #[test]
    fn whole_jointed_creature_translates_with_every_part_and_velocity_intact() {
        let (mut g, _) = arena(0.0);
        let id = spawn(&mut g, &Species::of(Genome::serpent()), -Vec2::X * 150.0);
        let chain = body(&g, id).chain.unwrap();
        let before: Vec<_> = g
            .bodies
            .iter()
            .filter(|b| b.chain == Some(chain))
            .map(|b| (b.id, b.position, b.velocity))
            .collect();
        for b in g.bodies.iter_mut().filter(|b| b.chain == Some(chain)) {
            b.position += Vec2::X * 200.0;
        }
        g.transit_bodies(&before.iter().map(|&(id, p, _)| (id, p)).collect::<Vec<_>>());
        let shift = body(&g, id).position - before[0].1;
        assert!(shift.x > 1200.0);
        for &(part, at, v) in &before {
            assert!(body(&g, part).position.distance(at + shift) < 0.001);
            assert_eq!(body(&g, part).velocity, v);
            assert_eq!(body(&g, part).rift_grace, GRACE);
        }
        assert_eq!(g.rift_traces.len(), 1);
    }
    #[test]
    fn a_blocked_tail_or_frozen_part_refuses_the_entire_chain() {
        for frozen in [false, true] {
            let (mut g, _) = arena(0.0);
            let id = spawn(&mut g, &Species::of(Genome::serpent()), -Vec2::X * 150.0);
            let chain = body(&g, id).chain.unwrap();
            let tail = g
                .bodies
                .iter()
                .rfind(|b| b.chain == Some(chain))
                .unwrap()
                .id;
            if frozen {
                g.bodies.iter_mut().find(|b| b.id == tail).unwrap().active = false;
            } else {
                add(&mut g, BodyKind::Base, Vec2::new(1200.0, 0.0));
            }
            let before: Vec<_> = g.bodies.iter().map(|b| (b.id, b.position)).collect();
            for b in g.bodies.iter_mut().filter(|b| b.chain == Some(chain)) {
                b.position += Vec2::X * 200.0;
            }
            g.transit_bodies(&before);
            assert_eq!(body(&g, id).rift_grace, 0.0);
            assert!(g.rift_traces.is_empty());
        }
    }
    #[test]
    fn rooted_passengers_and_parasites_ride_hosts_while_external_cords_and_grips_release() {
        for ship in [false, true] {
            let (mut g, owner) = arena(0.0);
            let id = if ship {
                g.player().unwrap().id
            } else {
                add(&mut g, BodyKind::Asteroid, Vec2::ZERO)
            };
            let passenger = spawn(
                &mut g,
                &Species::of(Genome::serpent()),
                Vec2::new(10.0, 20.0),
            );
            let passenger_chain = body(&g, passenger).chain.unwrap();
            let passenger_before: Vec<_> = g
                .bodies
                .iter()
                .filter(|b| b.chain == Some(passenger_chain))
                .map(|b| (b.id, b.position, b.velocity))
                .collect();
            let p = g.bodies.iter_mut().find(|b| b.id == passenger).unwrap();
            if ship {
                p.latch = Some(id);
            } else {
                p.root = Some(Root {
                    host: id,
                    angle: 0.0,
                    socket: None,
                });
            }
            g.tethers.push(Tether::link(owner, id, &DEFAULT_TUNING));
            move_body(
                &mut g,
                id,
                -Vec2::X * 100.0,
                Vec2::X * 10.0,
                Vec2::X * 600.0,
            );
            assert!(body(&g, id).position.x > 1200.0);
            assert!(body(&g, passenger).position.x > 1200.0);
            assert_eq!(body(&g, passenger).rift_grace, GRACE);
            let shift = body(&g, passenger).position - passenger_before[0].1;
            for (part, at, velocity) in passenger_before {
                assert!(body(&g, part).position.distance(at + shift) < 0.001);
                assert_eq!(body(&g, part).velocity, velocity);
                assert_eq!(body(&g, part).rift_grace, GRACE);
            }
            assert!(g.tethers.is_empty());
            assert!(if ship {
                body(&g, passenger).latch == Some(id)
            } else {
                body(&g, passenger).root.unwrap().host == id
            });
        }
    }
    #[test]
    fn fixed_rooted_phased_and_landed_travelers_cannot_enter_independently() {
        for reason in 0..4 {
            let (mut g, owner) = arena(0.0);
            let id = g.player().unwrap().id;
            let b = g.bodies.iter_mut().find(|b| b.id == id).unwrap();
            match reason {
                0 => b.pinned = true,
                1 => {
                    b.root = Some(Root {
                        host: owner,
                        angle: 0.0,
                        socket: None,
                    })
                }
                2 => b.phased = true,
                _ => g.pad.landed = Some((SectorId::ORIGIN, 0)),
            }
            move_body(
                &mut g,
                id,
                -Vec2::X * 100.0,
                Vec2::X * 10.0,
                Vec2::X * 600.0,
            );
            assert_eq!(body(&g, id).rift_grace, 0.0);
        }
    }
    #[test]
    fn hostile_shots_still_hurt_ship_and_rocks_keep_hostile_attribution_after_transit() {
        let (mut g, _) = arena(0.0);
        set_player(&mut g, Vec2::new(1400.0, 0.0), Vec2::ZERO);
        let shield = g.player().unwrap().shield;
        g.bullets.push(Bullet::hostile(
            -Vec2::X * 200.0,
            Vec2::X * 24000.0,
            1.0,
            10.0,
        ));
        g.move_bullets(DT);
        assert_eq!(g.player().unwrap().shield, shield - 10.0);
        set_player(&mut g, Vec2::new(0.0, -600.0), Vec2::ZERO);
        let rock = add(&mut g, BodyKind::Asteroid, Vec2::ZERO);
        let stone = g.bodies.iter_mut().find(|b| b.id == rock).unwrap();
        stone.sling_thrown = 4.0;
        stone.health = 1000.0;
        move_body(
            &mut g,
            rock,
            -Vec2::X * 200.0,
            Vec2::X * 10.0,
            Vec2::X * 800.0,
        );
        assert_eq!(body(&g, rock).sling_thrown, 4.0);
        let victim = spawn(&mut g, &Species::bogey(), Vec2::new(1500.0, 0.0));
        g.bodies.iter_mut().find(|b| b.id == victim).unwrap().health = 1.0;
        g.bodies.iter_mut().find(|b| b.id == victim).unwrap().shield = 0.0;
        let ai = g.bodies.iter().position(|b| b.id == rock).unwrap();
        let bi = g.bodies.iter().position(|b| b.id == victim).unwrap();
        let (left, right) = g.bodies.split_at_mut(bi);
        impact::strike(
            &mut left[ai],
            &mut right[0],
            100.0,
            0.0,
            1.0,
            &DEFAULT_TUNING,
        );
        g.remove_destroyed();
        assert_eq!(g.run.kills, 0);
        assert_eq!(g.score, 0);
        assert!(g.pickups.is_empty());
    }
    #[test]
    fn owner_and_mouth_cleanup_is_silent_and_grace_has_no_entity_map() {
        for reason in 0..9 {
            let (mut g, owner) = arena(WARNING);
            let b = g.bodies.iter_mut().find(|b| b.id == owner).unwrap();
            match reason {
                0 => b.health = 0.0,
                1 => b.consumed = true,
                2 => b.active = false,
                3 => b.genome.rift = 0.0,
                4 => b.follower = true,
                5 => b.position.x = f32::NAN,
                6 => g.bodies.retain(|b| b.id != owner),
                7 => g.active.clear(),
                _ => g.rifts[0].b = Vec2::X * 60000.0,
            }
            g.cleanup_rifts();
            assert!(g.rifts.is_empty(), "reason {reason}");
            assert!(g.drain_cues().is_empty());
        }
    }
    #[test]
    fn owner_sector_and_global_budgets_are_independent_and_include_warnings() {
        for global in [false, true] {
            let (mut g, owner) = arena(WARNING);
            g.rifts.clear();
            let count = if global { GLOBAL_CAP } else { SECTOR_CAP };
            for k in 0..count {
                let at = Vec2::new(if global { (k + 1) as f32 * 6000.0 } else { 0.0 }, 1500.0);
                let keeper = spawn(&mut g, &Species::of(Genome::seamer()), at);
                g.active.push(SectorId::containing(at));
                g.rifts.push(Rift {
                    owner: keeper,
                    a: at,
                    b: at + Vec2::X * 1000.0,
                    warning: WARNING,
                    left: LIFE,
                });
            }
            let i = g.bodies.iter().position(|b| b.id == owner).unwrap();
            g.step_rift(i, &mut PowerState::default(), DT);
            assert_eq!(g.rifts.len(), count);
        }
    }
    #[test]
    fn only_two_heads_build_per_sector_and_failed_attempts_wait_a_period() {
        let (mut g, _) = arena(0.0);
        g.rifts.clear();
        spawn(
            &mut g,
            &Species::of(Genome::seamer()),
            Vec2::new(0.0, 400.0),
        );
        let third = spawn(
            &mut g,
            &Species::of(Genome::seamer()),
            Vec2::new(0.0, 800.0),
        );
        let i = g.bodies.iter().position(|b| b.id == third).unwrap();
        let mut state = PowerState::default();
        g.step_rift(i, &mut state, DT);
        assert!(g.rifts.is_empty());
        assert!((state.rift_clock - 20.0).abs() < 0.001);
    }
    #[test]
    fn bare_ship_can_leave_a_warning_without_dash_parry_or_damage() {
        let (mut g, owner) = arena(WARNING);
        set_player(&mut g, Vec2::ZERO, Vec2::ZERO);
        let id = g.player().unwrap().id;
        let hull = body(&g, id).health;
        for _ in 0..80 {
            g.bodies.retain(|b| b.id == id || b.id == owner);
            g.step(
                DT,
                Input {
                    move_direction: Some(Vec2::Y * 0.6),
                    ..Default::default()
                },
            );
        }
        assert!(body(&g, id).position.y > RADIUS + body(&g, id).radius);
        assert_eq!(body(&g, id).health, hull);
        assert_eq!(body(&g, id).rift_grace, 0.0);
    }
    #[test]
    fn full_step_trace_is_repeatable_and_contains_cast_open_transit_and_expiry() {
        let trace = || {
            let (mut g, owner) = arena(WARNING);
            let mut out = Vec::new();
            let mut opened = false;
            let mut cast = false;
            let id = g.player().unwrap().id;
            for k in 0..1920 {
                g.bodies.retain(|b| b.id == owner || b.id == id);
                if k == 90 {
                    set_player(&mut g, -Vec2::X * 76.0, Vec2::X * 460.0);
                }
                if k == 120 {
                    set_player(&mut g, Vec2::new(0.0, -600.0), Vec2::ZERO);
                }
                g.step(DT, Input::default());
                let cues = g.drain_cues();
                opened |= cues.iter().any(|c| matches!(c, Cue::RiftOpen { .. }));
                cast |= cues.iter().any(|c| matches!(c, Cue::RiftTell { .. }));
                out.push((
                    body(&g, id).position,
                    body(&g, id).velocity,
                    body(&g, id).rift_grace,
                    g.rifts.clone(),
                    g.rift_traces.clone(),
                ));
            }
            assert!(opened && cast);
            assert!(out.iter().any(|(_, _, grace, _, _)| *grace > 0.0));
            assert!(g.rifts.is_empty());
            out
        };
        assert_eq!(trace(), trace());
    }
    #[test]
    fn neutral_rocks_are_environmental_but_deliberate_shoves_keep_credit() {
        for claimed in [false, true] {
            let (mut g, _) = arena(0.0);
            let rock = add(&mut g, BodyKind::Asteroid, Vec2::ZERO);
            let r = g.bodies.iter_mut().find(|b| b.id == rock).unwrap();
            r.shoved = if claimed { 4.0 } else { 0.0 };
            r.health = 1000.0;
            move_body(
                &mut g,
                rock,
                -Vec2::X * 150.0,
                Vec2::X * 10.0,
                Vec2::X * 800.0,
            );
            assert_eq!(body(&g, rock).rift_redirected > 0.0, !claimed);
            let victim = spawn(&mut g, &Species::bogey(), Vec2::new(1500.0, 0.0));
            let v = g.bodies.iter_mut().find(|b| b.id == victim).unwrap();
            v.health = 1.0;
            v.shield = 0.0;
            let ai = g.bodies.iter().position(|b| b.id == rock).unwrap();
            let bi = g.bodies.iter().position(|b| b.id == victim).unwrap();
            let (left, right) = g.bodies.split_at_mut(bi);
            impact::strike(
                &mut left[ai],
                &mut right[0],
                100.0,
                0.0,
                1.0,
                &DEFAULT_TUNING,
            );
            g.remove_destroyed();
            assert_eq!(g.run.kills, u32::from(claimed));
            assert_eq!(g.score > 0, claimed);
            assert!(g.civs.hits.is_empty());
        }
    }
    #[test]
    fn mining_grip_releases_and_transit_visuals_stay_bounded() {
        let (mut g, _) = arena(0.0);
        let rock = add(&mut g, BodyKind::Asteroid, Vec2::ZERO);
        g.beam = Some(Beam {
            target: rock,
            end: Vec2::ZERO,
            material: Material::Metal,
            progress: 0.0,
            crop: false,
        });
        g.gripped = Some(rock);
        move_body(
            &mut g,
            rock,
            -Vec2::X * 150.0,
            Vec2::X * 10.0,
            Vec2::X * 800.0,
        );
        assert!(g.beam.is_none() && g.gripped.is_none());
        for _ in 0..100 {
            g.rift_trace(Vec2::ZERO, Vec2::X * 1200.0);
        }
        assert_eq!(g.rift_traces.len(), 32);
        g.update_rifts(0.36);
        assert!(g.rift_traces.is_empty());
    }
    #[test]
    fn unloading_cleans_pairs_and_reload_never_restores_old_mouths() {
        let (mut g, _) = arena(WARNING);
        g.teleport(Vec2::X * 60000.0);
        g.step(DT, Input::default());
        assert!(g.rifts.is_empty());
        g.teleport(Vec2::ZERO);
        g.step(DT, Input::default());
        assert!(g.rifts.is_empty());
    }
    #[test]
    fn short_lived_shots_spend_only_remaining_flight_after_the_jump() {
        let (mut g, _) = arena(0.0);
        let mut shot = Bullet::friendly(-Vec2::X * 100.0, Vec2::X * 12000.0, 0.01);
        shot.burst = 30.0;
        g.bullets.push(shot);
        g.move_bullets(DT);
        assert!(g.bullets.is_empty());
        assert_eq!(g.rift_traces.len(), 1);
        assert!(g.effects.iter().any(|e| e.position.x > 1200.0));
    }
    #[test]
    fn redirected_rock_credit_propagates_to_secondary_impacts_and_shards() {
        let (mut g, _) = arena(0.0);
        let rock = add(&mut g, BodyKind::Asteroid, Vec2::ZERO);
        let r = g.bodies.iter_mut().find(|b| b.id == rock).unwrap();
        r.health = 1000.0;
        r.radius = 40.0;
        move_body(
            &mut g,
            rock,
            -Vec2::X * 150.0,
            Vec2::X * 10.0,
            Vec2::X * 800.0,
        );
        let secondary = add(&mut g, BodyKind::Asteroid, Vec2::new(1500.0, 0.0));
        g.bodies
            .iter_mut()
            .find(|b| b.id == secondary)
            .unwrap()
            .health = 1000.0;
        let ai = g.bodies.iter().position(|b| b.id == rock).unwrap();
        let bi = g.bodies.iter().position(|b| b.id == secondary).unwrap();
        let (left, right) = g.bodies.split_at_mut(bi);
        impact::strike(
            &mut left[ai],
            &mut right[0],
            10.0,
            0.0,
            1.0,
            &DEFAULT_TUNING,
        );
        assert_eq!(body(&g, secondary).rift_redirected, 5.0);
        let parent = body(&g, rock).clone();
        let prior = g.bodies.len();
        g.shatter(&parent);
        assert!(g.bodies.len() > prior);
        assert!(g.bodies[prior..].iter().all(|b| b.rift_redirected == 5.0));
    }
    #[test]
    fn an_incoming_redirected_rock_stays_environmental_until_a_deliberate_ram() {
        let (mut g, _) = arena(0.0);
        let rock = add(&mut g, BodyKind::Asteroid, Vec2::ZERO);
        move_body(
            &mut g,
            rock,
            -Vec2::X * 150.0,
            Vec2::X * 10.0,
            Vec2::X * 800.0,
        );
        let mut stone = body(&g, rock).clone();
        let mut ship = g.player().unwrap().clone();
        ship.velocity = Vec2::ZERO;
        shove::on_contact(
            &mut ship,
            &mut stone,
            Vec2::X,
            -800.0,
            1000.0,
            &g.loadout.skills,
            &DEFAULT_TUNING,
        );
        assert_eq!(stone.rift_redirected, 5.0);
        ship.velocity = Vec2::X * 800.0;
        shove::on_contact(
            &mut ship,
            &mut stone,
            Vec2::X,
            -800.0,
            1000.0,
            &g.loadout.skills,
            &DEFAULT_TUNING,
        );
        assert_eq!(stone.rift_redirected, 0.0);
        assert!(stone.shoved > 0.0);
    }
}
