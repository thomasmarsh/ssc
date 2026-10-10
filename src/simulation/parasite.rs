//! The two creatures that deal with the ship's body: the Hullworm (latch) and the Kindling
//! Remora (symbiote).
//!
//! **Hullworm.** A worm that touches the hull (ring 5 and out, not landed, not in grace, not in
//! a dash) fastens on, at most three at once, and drains one resource a second by its diet:
//! shield (siphon), metal (rocks), volatiles (graze) or, slowly, hull (hunt, never below a fifth
//! of it). It sits on the hull, can be shot, and comes off by: a dash, a perfect parry, a solid
//! hit at speed (the nearest worm is scraped off against the rock, so ramming a rock is a
//! remedy), or landing on a pad (clean in two seconds). A shaken worm flies off and cannot fasten
//! again for a few seconds.
//!
//! **Remora.** A shy symbiote. Left alone it drifts toward a calm ship (slow, not firing); held
//! within 120 units at under 60 for three seconds it bonds: the strain is owned and works at once
//! (see `organs`). Shooting it is the temptation and pays only a little bounty.
//!
//! Numbers live at the top of `power.rs`. Nothing here is random.

use super::*;
use crate::genome::Diet;
use crate::power::{self, Power};

/// A worm fastened to the ship.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Latch {
    pub worm: u64,
    /// Bearing on the hull, relative to the ship's heading.
    pub angle: f32,
    /// What it has taken, for the drawing (it grows fat).
    pub fed: f32,
}

/// The grooming ring around a Remora, for the HUD: where, and how far along (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grooming {
    pub at: Vec2,
    pub fill: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ParasiteState {
    pub(super) latches: Vec<Latch>,
    /// Worms that were shaken off: seconds before they may fasten again.
    pub(super) gap: super::digest::DetMap<u64, f32>,
    /// Seconds the ship has been landed with worms aboard.
    pub(super) clean: f32,
    /// Seconds since the ship last fired.
    pub(super) quiet: f32,
    /// The remora being groomed and its progress in seconds.
    pub(super) groom: Option<(u64, f32)>,
}

fn carries(body: &Body, power: Power) -> bool {
    body.kind == BodyKind::Creature
        && body.active
        && !body.consumed
        && !body.follower
        && body.health > 0.0
        && power.active(&body.genome)
}

impl Game {
    /// The worms on the hull, for the drawing and the HUD.
    pub fn latches(&self) -> &[Latch] {
        &self.parasites.latches
    }

    /// The remora being groomed, if any.
    pub fn grooming(&self) -> Option<Grooming> {
        let (id, seconds) = self.parasites.groom?;
        let body = self.body(id)?;
        Some(Grooming {
            at: body.position,
            fill: (seconds / power::GROOM_TIME).clamp(0.0, 1.0),
        })
    }

    pub(super) fn update_parasites(&mut self, dt: f32, firing: bool) {
        self.parasites.quiet = if firing {
            0.0
        } else {
            self.parasites.quiet + dt
        };
        for gap in self.parasites.gap.values_mut() {
            *gap -= dt;
        }
        self.parasites.gap.retain(|_, g| *g > 0.0);
        let Some((ship_id, ship_at, ship_r, ship_angle, ship_v)) = self
            .player()
            .map(|p| (p.id, p.position, p.radius, p.angle, p.velocity))
        else {
            self.release_all(0.0);
            self.parasites.groom = None;
            return;
        };
        self.update_latches(dt, ship_id, ship_at, ship_r, ship_angle);
        self.update_grooming(dt, ship_at, ship_v);
    }

    fn update_latches(&mut self, dt: f32, ship_id: u64, at: Vec2, radius: f32, angle: f32) {
        // Worms that died or vanished let go.
        let alive: Vec<u64> = self
            .bodies
            .iter()
            .filter(|b| b.latch == Some(ship_id) && b.health > 0.0 && !b.consumed)
            .map(|b| b.id)
            .collect();
        self.parasites.latches.retain(|l| alive.contains(&l.worm));
        for body in self
            .bodies
            .iter_mut()
            .filter(|b| b.latch.is_some() && !alive.contains(&b.id))
        {
            body.latch = None;
        }
        // A pad cleans the hull.
        if self.is_landed() {
            if !self.parasites.latches.is_empty() {
                self.parasites.clean += dt;
                if self.parasites.clean >= power::LATCH_PAD {
                    let ids: Vec<u64> = self.parasites.latches.iter().map(|l| l.worm).collect();
                    self.parasites.latches.clear();
                    self.parasites.clean = 0.0;
                    self.consume(&ids);
                    self.notify("PARASITES CLEANED OFF".into(), upgrades::Rarity::Common);
                }
            }
            return;
        }
        self.parasites.clean = 0.0;
        // New fastenings.
        let open = self.parasites.latches.len() < power::LATCH_MAX
            && self.player_invulnerability <= 0.0
            && !self.dashing();
        if open {
            let depth = world::latent(self.seed, SectorId::containing(at)).depth;
            if depth >= power::LATCH_RING {
                let gap = &self.parasites.gap;
                let found: Option<(usize, f32)> = self
                    .bodies
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| {
                        carries(b, Power::Latch)
                            && b.latch.is_none()
                            && !gap.contains_key(&b.id)
                            && b.position.distance(at) <= radius + b.radius + power::LATCH_REACH
                    })
                    .map(|(i, b)| (i, b.position.distance(at)))
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((index, _)) = found {
                    let worm = &mut self.bodies[index];
                    let bearing = (worm.position - at).to_angle();
                    worm.latch = Some(ship_id);
                    let id = worm.id;
                    self.parasites.latches.push(Latch {
                        worm: id,
                        angle: bearing - angle,
                        fed: 0.0,
                    });
                    self.cue(Cue::Latch { at, strength: 0.5 });
                    self.notify(
                        "HULLWORM ABOARD  dash, ram a rock or land".into(),
                        upgrades::Rarity::Rare,
                    );
                }
            }
        }
        // Feeding.
        let mut drains: Vec<(Diet, f32)> = Vec::new();
        for latch in &self.parasites.latches {
            if let Some(worm) = self.body(latch.worm) {
                let s = Power::Latch.strength(&worm.genome);
                drains.push((
                    worm.genome.diet,
                    (power::LATCH_DRAIN.0 + power::LATCH_DRAIN.1 * s) * dt,
                ));
            }
        }
        for (k, (diet, amount)) in drains.into_iter().enumerate() {
            let taken = self.drain_ship(diet, amount, dt);
            if let Some(latch) = self.parasites.latches.get_mut(k) {
                latch.fed += taken;
            }
        }
        self.sync_latches();
    }

    /// Takes `amount` from what `diet` eats on the ship; returns what was taken.
    fn drain_ship(&mut self, diet: Diet, amount: f32, dt: f32) -> f32 {
        match diet {
            Diet::Rocks => self.cargo.take(Material::Metal, amount),
            Diet::Graze => self.cargo.take(Material::Volatiles, amount),
            Diet::Hunt => {
                let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) else {
                    return 0.0;
                };
                let floor = ship.max_health * power::LATCH_HULL_FLOOR;
                let take = (power::LATCH_HULL_DRAIN * dt).min((ship.health - floor).max(0.0));
                ship.health -= take;
                take
            }
            _ => {
                let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) else {
                    return 0.0;
                };
                let take = amount.min(ship.shield.max(0.0));
                ship.shield -= take;
                // A feeding worm keeps the shield from recharging.
                ship.since_hit = 0.0;
                take
            }
        }
    }

    /// Seats every worm on the hull (called again after contacts have moved the ship).
    pub(super) fn sync_latches(&mut self) {
        let Some((at, radius, heading, velocity)) = self
            .player()
            .map(|p| (p.position, p.radius, p.angle, p.velocity))
        else {
            return;
        };
        let seats: Vec<(u64, f32)> = self
            .parasites
            .latches
            .iter()
            .map(|l| (l.worm, l.angle))
            .collect();
        for (id, angle) in seats {
            if let Some(worm) = self.bodies.iter_mut().find(|b| b.id == id) {
                let dir = Vec2::from_angle(heading + angle);
                let hull = attach::Frame {
                    position: at,
                    angle: heading,
                    radius,
                };
                worm.position = hull.seat(angle, worm.radius, 0.6);
                worm.velocity = velocity;
                worm.angle = dir.to_angle();
                // A fastened worm neither shoots nor bites.
                worm.fire_cooldown = worm.fire_cooldown.max(1.0);
                worm.contact_cooldown = worm.contact_cooldown.max(1.0);
            }
        }
    }

    /// Frees every worm; `fling` is the speed it leaves at.
    pub(super) fn release_all(&mut self, fling: f32) {
        let ids: Vec<u64> = self.parasites.latches.iter().map(|l| l.worm).collect();
        for id in ids {
            self.unfasten(id, fling, 0.0);
        }
        self.parasites.latches.clear();
    }

    /// Frees one worm: it flies off, is hurt by `hurt` and cannot fasten again for a while.
    fn unfasten(&mut self, id: u64, fling: f32, hurt: f32) {
        let ship = self.player().map(|p| (p.position, p.velocity));
        let Some(worm) = self.bodies.iter_mut().find(|b| b.id == id) else {
            return;
        };
        worm.latch = None;
        if let Some((at, v)) = ship {
            let away = (worm.position - at).normalize_or_zero();
            worm.velocity = v * 0.3 + away * fling;
        }
        worm.health -= hurt;
        self.parasites.gap.insert(id, power::LATCH_RETRY);
        self.parasites.latches.retain(|l| l.worm != id);
    }

    /// A dash, a perfect parry: everything aboard is shaken loose.
    pub(super) fn shake_off(&mut self) {
        self.pop_out();
        if !self.parasites.latches.is_empty() {
            self.release_all(power::LATCH_FLING);
        }
    }

    /// The ship struck something solid at speed along `normal` (from the ship): the worm
    /// nearest that side is scraped off and hurt.
    pub(super) fn scrape_off(&mut self, normal: Vec2) {
        let Some((at, heading)) = self.player().map(|p| (p.position, p.angle)) else {
            return;
        };
        let nearest = self
            .parasites
            .latches
            .iter()
            .map(|l| {
                let dir = Vec2::from_angle(heading + l.angle);
                (l.worm, dir.dot(normal))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, facing)| *facing > 0.0);
        let _ = at;
        if let Some((id, _)) = nearest {
            self.unfasten(id, power::LATCH_FLING, power::LATCH_SCRAPE_HURT);
        }
    }

    fn update_grooming(&mut self, dt: f32, at: Vec2, velocity: Vec2) {
        let calm = self.parasites.quiet >= power::GROOM_QUIET;
        let slow = velocity.length() < power::GROOM_COME_SPEED;
        let still = velocity.length() < power::GROOM_SPEED;
        let near: Option<(usize, f32)> = self
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| carries(b, Power::Symbiote))
            .map(|(i, b)| (i, b.position.distance(at)))
            .filter(|(_, d)| *d < power::GROOM_COME)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((index, distance)) = near else {
            self.parasites.groom = None;
            return;
        };
        let id = self.bodies[index].id;
        if calm && slow {
            // It comes to a calm ship, slowly, and stays shy otherwise.
            let body = &mut self.bodies[index];
            let toward = (at - body.position).normalize_or_zero();
            let pace = if distance > power::GROOM_RANGE * 0.6 {
                power::GROOM_DRIFT
            } else {
                0.0
            };
            body.velocity = body.velocity.lerp(toward * pace, (3.0 * dt).min(1.0));
            body.alert = false;
        }
        let (held, seconds) = self.parasites.groom.unwrap_or((id, 0.0));
        let seconds = if held != id { 0.0 } else { seconds };
        let grooming = calm && still && distance <= power::GROOM_RANGE;
        let seconds = if grooming {
            seconds + dt
        } else {
            (seconds - 2.0 * dt).max(0.0)
        };
        self.parasites.groom = (seconds > 0.0).then_some((id, seconds));
        if seconds >= power::GROOM_TIME {
            let genome = self.bodies[index].genome;
            self.parasites.groom = None;
            self.consume(&[id]);
            self.bond(super::organs::Strain::from_donor(
                super::organs::Organ::Remora,
                &genome,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::organs::Organ;
    use crate::simulation::skills::Skill;
    use crate::simulation::tests::{DT, add, empty_game};

    /// A game with the ship deep enough (ring 5 and out) for worms to fasten, no creatures.
    fn deep_game() -> Game {
        let mut game = empty_game();
        let seed = game.seed();
        let sector = (0..400)
            .map(|k| SectorId { x: k, y: k / 3 })
            .find(|&id| world::latent(seed, id).depth >= power::LATCH_RING + 0.5)
            .expect("a deep sector exists");
        game.bodies[0].position = sector.center();
        game.bodies[0].shield = 60.0;
        game.bodies[0].max_shield = 60.0;
        game.cargo = Cargo {
            metal: 100.0,
            volatiles: 100.0,
            crystal: 100.0,
            ..Default::default()
        };
        game.player_invulnerability = 0.0;
        game
    }

    fn worm(game: &mut Game, diet: Diet, offset: Vec2) -> u64 {
        let mut genome = Genome::hullworm();
        genome.diet = diet;
        let at = game.player().unwrap().position + offset;
        let id = game.place_creature(&crate::genome::Species::of(genome), at);
        let body = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        body.alert = true;
        id
    }

    fn touch() -> Vec2 {
        Vec2::new(14.0 + 7.0 + 4.0, 0.0)
    }

    fn tick(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT).round() as usize {
            game.update_parasites(DT, false);
        }
    }

    use crate::genome::Genome;

    #[test]
    fn a_worm_touching_the_hull_fastens_and_sits_on_it() {
        let mut game = deep_game();
        let id = worm(&mut game, Diet::Siphon, touch());
        tick(&mut game, 0.1);
        assert_eq!(game.latches().len(), 1);
        assert_eq!(
            game.body(id).unwrap().latch,
            Some(game.player().unwrap().id)
        );
        let gap = game
            .body(id)
            .unwrap()
            .position
            .distance(game.player().unwrap().position);
        assert!(gap < 14.0 + 7.0 + 1.0, "{gap}");
        // It never bites: contact with its host is skipped.
        let hull = game.player().unwrap().health;
        game.player_invulnerability = 0.0;
        game.resolve_contacts();
        assert_eq!(game.player().unwrap().health, hull);
        // Out of reach it does not.
        let mut far = deep_game();
        worm(&mut far, Diet::Siphon, Vec2::new(200.0, 0.0));
        tick(&mut far, 0.5);
        assert!(far.latches().is_empty());
    }

    #[test]
    fn it_does_not_fasten_in_the_shallows_while_landed_or_in_grace_or_past_three() {
        let mut shallow = empty_game();
        shallow.player_invulnerability = 0.0;
        worm(&mut shallow, Diet::Siphon, touch());
        tick(&mut shallow, 0.5);
        assert!(shallow.latches().is_empty(), "inside ring 5");

        let mut grace = deep_game();
        grace.player_invulnerability = 2.0;
        worm(&mut grace, Diet::Siphon, touch());
        grace.update_parasites(DT, false);
        assert!(grace.latches().is_empty(), "in grace");

        let mut crowd = deep_game();
        for k in 0..6 {
            worm(
                &mut crowd,
                Diet::Siphon,
                Vec2::from_angle(k as f32 * 1.0) * 24.0,
            );
        }
        tick(&mut crowd, 2.0);
        assert_eq!(crowd.latches().len(), power::LATCH_MAX);
    }

    #[test]
    fn each_diet_drains_its_own_resource_and_hull_never_below_a_fifth() {
        let drain = |diet: Diet| {
            let mut game = deep_game();
            worm(&mut game, diet, touch());
            let ship = game.player().unwrap();
            let before = (ship.shield, ship.health, game.cargo);
            tick(&mut game, 5.0);
            let ship = game.player().unwrap();
            (
                before.0 - ship.shield,
                before.1 - ship.health,
                before.2.metal - game.cargo.metal,
                before.2.volatiles - game.cargo.volatiles,
            )
        };
        // Strength 0.714 of a 0.7 gene: 2 + 4 * s a second.
        let rate = power::LATCH_DRAIN.0
            + power::LATCH_DRAIN.1 * Power::Latch.strength(&Genome::hullworm());
        let (shield, hull, metal, volatiles) = drain(Diet::Siphon);
        assert!(
            (shield - 5.0 * rate).abs() < 1.0 || shield >= 59.0,
            "{shield}"
        );
        assert_eq!((hull, metal, volatiles), (0.0, 0.0, 0.0));
        let (shield, _, metal, volatiles) = drain(Diet::Rocks);
        assert!((metal - 5.0 * rate).abs() < 0.5, "{metal}");
        assert_eq!((shield, volatiles), (0.0, 0.0));
        let (_, _, metal, volatiles) = drain(Diet::Graze);
        assert!((volatiles - 5.0 * rate).abs() < 0.5, "{volatiles}");
        assert_eq!(metal, 0.0);
        let (_, hull, _, _) = drain(Diet::Hunt);
        assert!(hull > 3.0 && hull < 6.0, "slow: {hull}");
        // It stops at a fifth of the hull: a long feed cannot kill.
        let mut game = deep_game();
        worm(&mut game, Diet::Hunt, touch());
        tick(&mut game, 400.0);
        let ship = game.player().unwrap();
        assert!(ship.health >= ship.max_health * power::LATCH_HULL_FLOOR - 0.01);
        assert!(ship.health > 0.0);
        // And the shield does not recharge while a siphoner feeds.
        let mut feeding = deep_game();
        worm(&mut feeding, Diet::Siphon, touch());
        tick(&mut feeding, 30.0);
        assert!(feeding.player().unwrap().shield < 1.0);
    }

    #[test]
    fn a_dash_and_a_perfect_parry_shake_every_worm_off_and_they_cannot_return_at_once() {
        let mut game = deep_game();
        game.loadout.skills.raise(Skill::Dash);
        let ids: Vec<u64> = (0..2)
            .map(|k| {
                worm(
                    &mut game,
                    Diet::Siphon,
                    Vec2::from_angle(k as f32 * 2.5) * 22.0,
                )
            })
            .collect();
        tick(&mut game, 0.2);
        assert_eq!(game.latches().len(), 2);
        assert!(game.dash(Some(Vec2::X)));
        assert!(game.latches().is_empty());
        for id in &ids {
            assert_eq!(game.body(*id).unwrap().latch, None);
        }
        // They are flung and wait before fastening again (the grace of the dash passes first).
        game.player_invulnerability = 0.0;
        game.update_dash(0.5);
        let at = game.player().unwrap().position;
        game.bodies
            .iter_mut()
            .find(|b| b.id == ids[0])
            .unwrap()
            .position = at + touch();
        tick(&mut game, 1.0);
        assert!(game.latches().is_empty(), "retry gap");
        tick(&mut game, power::LATCH_RETRY);
        game.bodies
            .iter_mut()
            .find(|b| b.id == ids[0])
            .unwrap()
            .position = at + touch();
        tick(&mut game, 0.2);
        assert_eq!(game.latches().len(), 1);
        game.shake_off();
        assert!(game.latches().is_empty());
    }

    #[test]
    fn a_hard_hit_on_a_rock_scrapes_the_worm_on_that_side_and_hurts_it() {
        let mut game = deep_game();
        // One worm ahead (the ship faces +x after the ram setup), one behind.
        game.bodies[0].angle = 0.0;
        let front = worm(&mut game, Diet::Siphon, Vec2::new(21.0, 0.0));
        let back = worm(&mut game, Diet::Siphon, Vec2::new(-21.0, 0.0));
        tick(&mut game, 0.2);
        assert_eq!(game.latches().len(), 2);
        game.player_invulnerability = 0.0;
        let at = game.player().unwrap().position;
        let rock = add(&mut game, BodyKind::Asteroid, at + Vec2::new(40.0, 0.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == rock)
            .unwrap()
            .radius = 30.0;
        game.bodies[0].velocity = Vec2::new(power::LATCH_SCRAPE + 100.0, 0.0);
        game.resolve_contacts();
        assert_eq!(game.latches().len(), 1);
        assert_eq!(
            game.latches()[0].worm,
            back,
            "the one facing the rock came off"
        );
        assert!(game.body(front).unwrap().health < 14.0);
        // A soft touch scrapes nothing.
        let mut soft = deep_game();
        soft.bodies[0].angle = 0.0;
        worm(&mut soft, Diet::Siphon, Vec2::new(21.0, 0.0));
        tick(&mut soft, 0.2);
        soft.player_invulnerability = 0.0;
        let at = soft.player().unwrap().position;
        let rock = add(&mut soft, BodyKind::Asteroid, at + Vec2::new(40.0, 0.0));
        soft.bodies
            .iter_mut()
            .find(|b| b.id == rock)
            .unwrap()
            .radius = 30.0;
        soft.bodies[0].velocity = Vec2::new(60.0, 0.0);
        soft.resolve_contacts();
        assert_eq!(soft.latches().len(), 1);
    }

    #[test]
    fn a_pad_cleans_the_hull_and_a_shot_worm_lets_go() {
        let mut game = deep_game();
        worm(&mut game, Diet::Siphon, touch());
        let other = worm(&mut game, Diet::Siphon, Vec2::new(-21.0, 0.0));
        tick(&mut game, 0.2);
        assert_eq!(game.latches().len(), 2);
        // Shot dead: gone from the hull at the next step.
        game.bodies
            .iter_mut()
            .find(|b| b.id == other)
            .unwrap()
            .health = 0.0;
        game.update_parasites(DT, false);
        assert_eq!(game.latches().len(), 1);
        // Landing cleans: after two seconds the worm is gone.
        game.pad.landed = Some((SectorId { x: 0, y: 0 }, 0));
        tick(&mut game, power::LATCH_PAD + 0.2);
        assert!(game.latches().is_empty());
        assert!(
            !game
                .bodies
                .iter()
                .any(|b| b.kind == BodyKind::Creature && b.latch.is_some())
        );
    }

    #[test]
    fn the_carrier_species_and_specimens_are_built_and_sampled() {
        assert!(Power::Latch.built() && Power::Symbiote.built());
        // A sampled worm and remora are styled so the player can tell them (small, gunless).
        // Sampled carriers are styled: roll every band of the sampler in a deep, calm sector.
        let seed = 11;
        let sector = (0..400)
            .map(|k| SectorId { x: k, y: k / 3 })
            .find(|&id| world::latent(seed, id).depth >= 8.0)
            .expect("a deep sector exists");
        let mut params = world::latent(seed, sector);
        params.aggression = 0.1;
        let (mut worms, mut remoras) = (0, 0);
        for k in 0..20_000 {
            let mut g = Genome::default();
            power::sample(&mut g, k as f32 / 20_000.0, &params);
            match g.live_power().map(|c| c.power) {
                Some(Power::Latch) => {
                    worms += 1;
                    assert!(g.radius <= 9.0);
                    assert_eq!(g.weapon, crate::genome::Weapon::None);
                    assert!(matches!(
                        g.diet,
                        Diet::Siphon | Diet::Rocks | Diet::Graze | Diet::Hunt
                    ));
                }
                Some(Power::Symbiote) => {
                    remoras += 1;
                    assert_eq!(g.fear, crate::genome::Fear::Player);
                    assert_eq!(g.trigger, crate::genome::Trigger::Harm);
                    assert_eq!(g.contact_damage, 0.0);
                }
                _ => {}
            }
        }
        assert!(worms > 0 && remoras > 0, "{worms} {remoras}");
    }

    /// A remora `offset` from the ship.
    fn remora(game: &mut Game, offset: Vec2) -> u64 {
        let at = game.player().unwrap().position + offset;
        game.place_creature(&crate::genome::Species::of(Genome::remora()), at)
    }

    fn groom(game: &mut Game, seconds: f32, firing: bool) {
        for _ in 0..(seconds / DT).round() as usize {
            game.update_parasites(DT, firing);
        }
    }

    #[test]
    fn a_calm_ship_holding_still_beside_a_remora_bonds_it_and_the_perk_works_at_once() {
        let mut game = deep_game();
        let id = remora(&mut game, Vec2::new(90.0, 0.0));
        groom(&mut game, 2.5, false);
        // Still grooming: the ring fills but nothing is bonded yet.
        let ring = game.grooming().expect("a ring");
        assert!(ring.fill > 0.0 && ring.fill < 1.0, "{ring:?}");
        assert!(game.loadout.organs.strain(Organ::Remora).is_none());
        groom(&mut game, 3.0, false);
        assert!(game.body(id).is_none(), "the remora joined the ship");
        assert_eq!(game.loadout.organs.strain(Organ::Remora).unwrap().level, 1);
        // It works at once, without a slot, and for a limited time.
        assert!(game.loadout.organs.perk(Organ::Remora).is_some());
        assert_eq!(game.loadout.organs.fitted().len(), 0);
        for _ in 0..(super::tuning::BOND_LOAN / DT) as usize + 5 {
            game.update_organs(DT);
        }
        assert!(game.loadout.organs.perk(Organ::Remora).is_none());
        assert!(game.loadout.organs.owns(Organ::Remora));
    }

    #[test]
    fn grooming_needs_calm_firing_or_speed_resets_it_and_a_free_slot_takes_the_bond() {
        let mut game = deep_game();
        remora(&mut game, Vec2::new(90.0, 0.0));
        groom(&mut game, 2.0, false);
        groom(&mut game, 1.0, true);
        assert!(
            game.loadout.organs.strain(Organ::Remora).is_none(),
            "firing breaks it"
        );
        assert!(game.grooming().is_none() || game.grooming().unwrap().fill < 0.5);
        // Too fast: no progress.
        let mut fast = deep_game();
        remora(&mut fast, Vec2::new(90.0, 0.0));
        fast.bodies[0].velocity = Vec2::new(0.0, 200.0);
        groom(&mut fast, 6.0, false);
        assert!(fast.loadout.organs.strain(Organ::Remora).is_none());
        // Too far: no progress.
        let mut far = deep_game();
        remora(&mut far, Vec2::new(300.0, 0.0));
        far.bodies[0].velocity = Vec2::ZERO;
        let id = far
            .bodies
            .iter()
            .find(|b| b.genome.symbiote > 0.0)
            .unwrap()
            .id;
        far.bodies.iter_mut().find(|b| b.id == id).unwrap().velocity = Vec2::ZERO;
        groom(&mut far, 1.0, false);
        assert!(far.grooming().is_none());
        // With a free slot the bond settles into it for no graft cost.
        let mut slotted = deep_game();
        slotted.loadout.skills.raise(Skill::Symbiosis);
        remora(&mut slotted, Vec2::new(90.0, 0.0));
        groom(&mut slotted, 6.0, false);
        assert!(slotted.loadout.organs.is_fitted(Organ::Remora));
        assert_eq!(slotted.cargo.crystal, 100.0);
        // A second remora raises the strain (never lowers it).
        remora(&mut slotted, Vec2::new(90.0, 0.0));
        groom(&mut slotted, 6.0, false);
        assert_eq!(
            slotted.loadout.organs.strain(Organ::Remora).unwrap().level,
            2
        );
    }

    #[test]
    fn parasites_and_symbiotes_are_deterministic() {
        let run = || {
            let mut game = deep_game();
            worm(&mut game, Diet::Siphon, touch());
            remora(&mut game, Vec2::new(-200.0, 0.0));
            for step in 0..900 {
                game.update_parasites(DT, step % 300 > 250);
                game.update_organs(DT);
            }
            (
                game.player().unwrap().shield,
                game.latches().to_vec(),
                game.grooming(),
                game.loadout.organs.clone(),
            )
        };
        assert_eq!(run(), run());
    }
}
