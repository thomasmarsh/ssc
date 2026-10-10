//! The tick, as named phases. `Game::step` is a short orchestration of the `phase_*` methods
//! below, in a fixed order; that order is behavior (RNG draw order, who sees whose position
//! this tick) and the pinned goldens fail if it moves. Any new per-tick system belongs in the
//! phase whose description fits it, never directly in `step`.
//!
//! With the `profile` cargo feature each phase's wall time is accumulated (see `Profile`);
//! without it the timing calls compile to nothing.

use super::fleet::DroneView;
use super::tuning;
use super::*;

/// One named stretch of the tick, in execution order. Used to label profile timings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Dev sync, hit stop, input shaping (jam and confusion), clocks, effect decay.
    Begin,
    /// Jam, lure, legacy, chart, sector streaming, civilizations, regions, realms.
    World,
    /// Loadout, organs, builders.
    Loadout,
    /// Per-body cooldowns, shield recharge, dust healing.
    BodyTimers,
    /// Farm, pads, ship control, mining, grip, regrowth, arms, shears.
    Ship,
    /// Wildlife and steering, civilization mining, roots, flocks, bases, turrets.
    Creatures,
    /// Brooding, grazing, food, metabolism, blight, tending, hunting, growth, reproduction.
    Ecology,
    /// Tethers, chains, apexes, adaptation, rifts, engulfing, powers, parasites, splits, song.
    ApexPowers,
    /// Weapon fire, shot cues, gravity wells.
    Weapons,
    /// Gravity and integration of free bodies.
    Physics,
    /// Drone impacts, roots, rift transit, containment, contacts, latches, chain limits.
    Resolve,
    /// Parry, dash, bullets, flock shots, mines, rune fields, husks, pickups, dev apply.
    Projectiles,
    /// Step accounting, diplomacy, apex tally, breakups, removal, pruning.
    Settle,
    /// Ping, jobs, agreements, damage and heartbeat cues.
    Upkeep,
}

impl Phase {
    /// Every phase, in tick order.
    pub const ALL: [Phase; 14] = [
        Phase::Begin,
        Phase::World,
        Phase::Loadout,
        Phase::BodyTimers,
        Phase::Ship,
        Phase::Creatures,
        Phase::Ecology,
        Phase::ApexPowers,
        Phase::Weapons,
        Phase::Physics,
        Phase::Resolve,
        Phase::Projectiles,
        Phase::Settle,
        Phase::Upkeep,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Phase::Begin => "begin",
            Phase::World => "world",
            Phase::Loadout => "loadout",
            Phase::BodyTimers => "body_timers",
            Phase::Ship => "ship",
            Phase::Creatures => "creatures",
            Phase::Ecology => "ecology",
            Phase::ApexPowers => "apex_powers",
            Phase::Weapons => "weapons",
            Phase::Physics => "physics",
            Phase::Resolve => "resolve",
            Phase::Projectiles => "projectiles",
            Phase::Settle => "settle",
            Phase::Upkeep => "upkeep",
        }
    }
}

/// Accumulated wall time per phase. Not state: never saved and never digested.
#[cfg(feature = "profile")]
#[derive(Debug, Default)]
pub struct Profile {
    nanos: [u64; Phase::ALL.len()],
    ticks: u64,
    mark: Option<std::time::Instant>,
}

#[cfg(feature = "profile")]
impl Profile {
    fn start(&mut self) {
        self.mark = Some(std::time::Instant::now());
    }

    fn end(&mut self, phase: Phase) {
        let now = std::time::Instant::now();
        if let Some(mark) = self.mark {
            self.nanos[phase as usize] += now.duration_since(mark).as_nanos() as u64;
        }
        self.mark = Some(now);
    }
}

/// Values carried between phases within one tick.
struct Tick {
    dt: f32,
    input: Input,
    /// Bullets alive at the start (new shots are cued against it).
    in_flight: usize,
    /// Shield and health at the start, less what jumps and mining drained.
    ship_before: Option<(f32, f32)>,
    sources: (Vec<Vec2>, Vec<(Vec2, f32)>, bool),
    /// Ship position after streaming, for distance travelled.
    start: Option<Vec2>,
    drones_before: Vec<DroneView>,
    /// Positions before integration, kept only when rifts exist.
    rift_before: Vec<(u64, Vec2)>,
    /// Positions before integration, kept only when mining drones exist.
    impact_before: HashMap<u64, Vec2>,
    shots: (usize, usize),
    electrolyzing: bool,
}

impl Game {
    /// Intended for a fixed 1/60-second caller. Rejects invalid durations and caps
    /// a single step at 50 ms, preventing a resumed window from causing huge jumps.
    pub fn step(&mut self, dt: f32, input: Input) {
        if self.game_over || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.05);
        self.profile_start();
        let Some(mut tick) = self.phase_begin(dt, input) else {
            return;
        };
        self.profile_end(Phase::Begin);
        self.phase_world(&mut tick);
        self.profile_end(Phase::World);
        self.phase_loadout(&tick);
        self.profile_end(Phase::Loadout);
        self.phase_body_timers(&tick);
        self.profile_end(Phase::BodyTimers);
        self.phase_ship(&mut tick);
        self.profile_end(Phase::Ship);
        self.phase_creatures(dt);
        self.profile_end(Phase::Creatures);
        self.phase_ecology(dt);
        self.profile_end(Phase::Ecology);
        self.phase_apex_powers(&tick);
        self.profile_end(Phase::ApexPowers);
        self.phase_weapons(&tick);
        self.profile_end(Phase::Weapons);
        self.phase_physics(&mut tick);
        self.profile_end(Phase::Physics);
        self.phase_resolve(&tick);
        self.profile_end(Phase::Resolve);
        self.phase_projectiles(dt);
        self.profile_end(Phase::Projectiles);
        self.phase_settle(&tick);
        self.profile_end(Phase::Settle);
        self.phase_upkeep(tick);
        self.profile_end(Phase::Upkeep);
        self.profile_tick();
    }

    /// Dev sync, hit stop, input shaping, then the clocks. `None` when a parry's hit stop
    /// freezes this tick.
    fn phase_begin(&mut self, dt: f32, input: Input) -> Option<Tick> {
        self.sync_dev();
        // A perfect parry freezes everything for a few ticks.
        if self.hold_hit_stop(dt) {
            return None;
        }
        // Mining and firing exclude each other: the beam wins while it is held.
        let input = Input {
            fire: input.fire && !input.mine,
            ..input
        };
        // A confused pilot's hands deliver a bent input, and a jammed gun does not fire.
        let input = self.scramble_input(input);
        let input = if self.jammed(JamSystem::Weapons) {
            if input.fire {
                self.cue(Cue::Refused);
            }
            Input {
                fire: false,
                ..input
            }
        } else {
            input
        };
        let in_flight = self.bullets.len();
        let ship_before = self.player().map(|p| (p.shield, p.health));
        let sources = self.incoming_sources();
        self.time += dt;
        self.societies.advance(dt);
        self.player_invulnerability = (self.player_invulnerability - dt).max(0.0);
        self.streak.tick(dt);
        self.feel.tick(dt);
        for effect in &mut self.effects {
            effect.remaining -= dt;
        }
        self.effects.retain(|effect| effect.remaining > 0.0);
        Some(Tick {
            dt,
            input,
            in_flight,
            ship_before,
            sources,
            start: None,
            drones_before: Vec::new(),
            rift_before: Vec::new(),
            impact_before: HashMap::new(),
            shots: (0, 0),
            electrolyzing: input.brake && input.mine,
        })
    }

    /// Jam, lure, legacy and chart clocks, sector streaming, then the slow world layers.
    fn phase_world(&mut self, tick: &mut Tick) {
        let dt = tick.dt;
        self.update_jam(dt, tick.input.fire);
        self.update_lure(dt);
        self.update_legacy(dt);
        let jumped = self.update_chart(dt);
        if let Some(before) = tick.ship_before.as_mut() {
            before.0 -= jumped;
        }
        self.stream_sectors();
        self.note_sector();
        tick.start = self.player().map(|p| p.position);
        self.update_civilizations(dt);
        self.update_region(dt);
        self.update_realm(dt);
    }

    fn phase_loadout(&mut self, tick: &Tick) {
        self.update_loadout(tick.dt, &tick.input);
        self.update_organs(tick.dt);
        self.update_builders(tick.dt);
    }

    fn phase_body_timers(&mut self, tick: &Tick) {
        let beaming = self.beam.is_some() || tick.electrolyzing;
        tick_body_timers(&mut self.bodies, tick.dt, self.stats.recharge, beaming);
    }

    /// Everything the player's ship does this tick: pads, control, mining, grip, arms.
    fn phase_ship(&mut self, tick: &mut Tick) {
        let (dt, input) = (tick.dt, tick.input);
        tick.drones_before = self.mining_drone_views();
        self.update_farm();
        self.update_pads(dt, &input);
        tick.shots = (self.bullets.len(), self.mines.len());
        self.control_player(dt, input);
        self.electrolysis = None;
        let drained = self.update_mining(dt, input.mine && !tick.electrolyzing);
        let drained = if tick.electrolyzing {
            drained + self.update_electrolysis(dt)
        } else {
            drained
        };
        self.update_grip(dt);
        self.update_regrowth(dt);
        if let Some(before) = tick.ship_before.as_mut() {
            before.0 -= drained;
        }
        self.update_arms(dt, input.fire);
        self.pad_noise(tick.shots.0, tick.shots.1);
        if self.stats.shears {
            // Shears cut a weak cord the moment it latches and wear a stout one through.
            for tether in self
                .tethers
                .iter_mut()
                .filter(|t| t.kind == TetherKind::Latch)
            {
                if tether.max_health <= tether::SHEARS_INSTANT {
                    tether.health = 0.0;
                } else if tether.attached() {
                    tether.health -= tether::SHEARS_RATE * dt;
                }
            }
        }
    }

    /// Movement and hunting of the living, plus civilization mining and roots. The developer
    /// freeze skips what makes enemies move, hunt and shoot.
    fn phase_creatures(&mut self, dt: f32) {
        let frozen = self.dev.freeze_enemies;
        if !frozen {
            self.update_wildlife(dt);
            self.steer_creatures(dt);
        }
        self.update_civ_mining(dt);
        self.update_roots(dt);
        if !frozen {
            self.update_flocks(dt);
            self.update_bases(dt);
            self.update_turrets(dt);
        }
    }

    /// Feeding, metabolism, hunting, growth and reproduction.
    fn phase_ecology(&mut self, dt: f32) {
        self.tend_broods(dt);
        self.graze();
        self.update_food(dt);
        self.update_metabolism(dt);
        self.graze_plankton();
        self.graze_plants(dt);
        self.update_blight(dt);
        self.update_tending(dt);
        if !self.dev.freeze_enemies {
            self.hunt(dt);
        }
        self.update_growth(dt);
        self.update_reproduction(dt);
        self.update_eggs(dt);
    }

    /// Tethers, chains, apex bodies and the powers of creatures.
    fn phase_apex_powers(&mut self, tick: &Tick) {
        let dt = tick.dt;
        self.update_tethers(dt);
        self.update_chains(dt);
        self.update_apexes(dt);
        self.update_adapt(dt);
        self.update_rifts(dt);
        self.update_engulf_hold(dt);
        self.update_powers(dt);
        // After steering, so a remora's drift to a calm ship wins over its shyness.
        self.update_parasites(dt, tick.input.fire);
        self.update_splits(dt);
        self.update_song_rings(dt);
    }

    fn phase_weapons(&mut self, tick: &Tick) {
        if !self.dev.freeze_enemies {
            self.fire_weapons();
        }
        self.cue_new_shots(tick.in_flight);
        self.update_wells(tick.dt);
    }

    /// Gravity, then integration of free bodies.
    fn phase_physics(&mut self, tick: &mut Tick) {
        let dt = tick.dt;
        self.apply_gravity(dt);
        if !self.rifts.is_empty() {
            tick.rift_before = self.bodies.iter().map(|b| (b.id, b.position)).collect();
        }
        if !tick.drones_before.is_empty() {
            tick.impact_before = self.bodies.iter().map(|b| (b.id, b.position)).collect();
        }
        let shove_cap = self.loadout.skills.shove_speed_cap();
        integrate_bodies(&mut self.bodies, dt, shove_cap);
    }

    /// Contacts and the constraints that follow them.
    fn phase_resolve(&mut self, tick: &Tick) {
        let dt = tick.dt;
        self.damage_drone_impacts(dt, &tick.drones_before, &tick.impact_before);
        // Rooted life rides its host, and again after contacts have shoved the host.
        self.sync_roots();
        self.transit_bodies(&tick.rift_before);
        self.contain_in_active_region();
        self.resolve_contacts();
        self.sync_roots();
        self.sync_latches();
        // After contacts, so an impact cannot leave a joint stretched past its limit.
        self.constrain_chains();
        self.damage_drone_contacts(dt);
    }

    /// Parry, dash and everything in flight or lying about.
    fn phase_projectiles(&mut self, dt: f32) {
        self.update_parry(dt);
        self.update_dash(dt);
        self.move_bullets(dt);
        self.shoot_flocks(dt);
        self.update_mines(dt);
        self.update_rune_fields(dt);
        self.update_husks();
        self.update_pickups(dt);
        self.apply_dev();
    }

    /// Distance and damage accounting for the step, diplomacy, then removal of the dead.
    fn phase_settle(&mut self, tick: &Tick) {
        let dt = tick.dt;
        let travelled = match (tick.start, self.player()) {
            (Some(from), Some(ship)) => from.distance(ship.position),
            _ => 0.0,
        };
        let taken = match (tick.ship_before, self.player()) {
            (Some((shield, health)), Some(ship)) => {
                (shield + health - ship.shield - ship.health.max(0.0)).max(0.0)
            }
            (Some((shield, health)), None) => shield + health,
            _ => 0.0,
        };
        self.note_step(dt, travelled, taken);
        self.chart_ship_damaged(taken);
        self.update_diplomacy(dt);
        self.update_apex();
        self.update_breakups(dt);
        self.remove_destroyed();
        self.prune_slings();
        self.cleanup_rifts();
    }

    /// Ping, jobs, agreements and the cues for what just happened to the ship.
    fn phase_upkeep(&mut self, tick: Tick) {
        let dt = tick.dt;
        self.update_ping(dt);
        self.update_jobs();
        self.update_agreements(dt);
        self.cue_player_damage(tick.ship_before, tick.sources);
        self.cue_heartbeat(dt);
    }

    #[cfg(feature = "profile")]
    fn profile_start(&mut self) {
        self.profile.start();
    }

    #[cfg(not(feature = "profile"))]
    #[inline(always)]
    fn profile_start(&mut self) {}

    #[cfg(feature = "profile")]
    fn profile_end(&mut self, phase: Phase) {
        self.profile.end(phase);
    }

    #[cfg(not(feature = "profile"))]
    #[inline(always)]
    fn profile_end(&mut self, _phase: Phase) {}

    #[cfg(feature = "profile")]
    fn profile_tick(&mut self) {
        self.profile.ticks += 1;
    }

    #[cfg(not(feature = "profile"))]
    #[inline(always)]
    fn profile_tick(&mut self) {}

    /// Mean wall milliseconds per tick of each phase, in tick order, over every full step
    /// taken by this game (steps that hit-stop or are rejected do not count).
    #[cfg(feature = "profile")]
    pub fn phase_timings(&self) -> Vec<(&'static str, f64)> {
        let ticks = self.profile.ticks.max(1) as f64;
        Phase::ALL
            .iter()
            .map(|&p| {
                (
                    p.name(),
                    self.profile.nanos[p as usize] as f64 / ticks / 1e6,
                )
            })
            .collect()
    }
}

/// Counts down every active body's cooldowns, recharges shields after quiet and heals
/// dust-grazers. `ship_recharge` is the ship's shield rate; `beaming` holds the ship's shield
/// off while it works a beam.
fn tick_body_timers(bodies: &mut [Body], dt: f32, ship_recharge: f32, beaming: bool) {
    for body in bodies.iter_mut().filter(|b| b.active) {
        body.fire_cooldown = (body.fire_cooldown - dt).max(0.0);
        body.contact_cooldown = (body.contact_cooldown - dt).max(0.0);
        body.panic = (body.panic - dt).max(0.0);
        body.since_hit += dt;
        body.provoked = (body.provoked - dt).max(0.0);
        body.shoved = (body.shoved - dt).max(0.0);
        body.sling_free = (body.sling_free - dt).max(0.0);
        body.sling_thrown = (body.sling_thrown - dt).max(0.0);
        body.rift_redirected = (body.rift_redirected - dt).max(0.0);
        body.rift_grace = (body.rift_grace - dt).max(0.0);
        body.rune_pushed = (body.rune_pushed - dt).max(0.0);
        body.shove_clock = (body.shove_clock - dt).max(0.0);
        body.grip_free = (body.grip_free - dt).max(0.0);
        if body.since_hit > tuning::SHIELD_RECHARGE_DELAY
            && !(beaming && body.kind == BodyKind::Player)
        {
            let rate = if body.kind == BodyKind::Player {
                ship_recharge
            } else {
                tuning::NPC_SHIELD_RATE
            };
            body.shield = (body.shield + dt * rate).min(body.max_shield);
        }
        if body.kind == BodyKind::Creature && body.genome.diet == Diet::Dust {
            body.health = (body.health + dt * tuning::DUST_HEAL).min(body.max_health);
        }
    }
}

/// Moves every free active body by its velocity and applies rock drag and spin.
fn integrate_bodies(bodies: &mut [Body], dt: f32, shove_cap: f32) {
    for body in bodies.iter_mut().filter(|b| b.active) {
        if body.shoved > 0.0 && body.kind == BodyKind::Asteroid {
            body.velocity = body.velocity.clamp_length_max(shove_cap);
        }
        if !is_fixed(body) {
            body.position += body.velocity * dt;
        }
        if body.kind == BodyKind::Asteroid && !body.pinned {
            // Flung rocks slowly lose their excess speed rather than ricocheting forever.
            let speed = body.velocity.length();
            if speed > tuning::ASTEROID_SPEED_FLOOR {
                body.velocity *= (tuning::ASTEROID_SPEED_FLOOR
                    + (speed - tuning::ASTEROID_SPEED_FLOOR) * (-tuning::ASTEROID_DRAG * dt).exp())
                    / speed;
            }
            body.angle += dt * tuning::ASTEROID_SPIN;
        } else if body.rock == RockKind::Planetoid {
            body.angle += dt * tuning::PLANETOID_SPIN;
        }
    }
}
