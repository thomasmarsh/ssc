//! Deterministic, headless gameplay. Coordinates are world units, time is seconds,
//! and angles point along (cos(angle), sin(angle)). Rendering owns no game rules.
//!
//! The world is an unbounded grid of sectors (see `world`). Sectors touched by the
//! player's active region are generated on demand and simulated; bodies elsewhere are
//! frozen, and sectors far from the player are dropped and regenerated on return.

mod arms;
pub mod arsenal;
mod brain;
mod chain;
mod civ;
mod civmine;
mod creature;
mod cues;
mod ecology;
mod food;
mod fortress;
mod growth;
mod guide;
mod loot;
mod mining;
mod pads;
mod ping;
mod root;
pub mod run;
mod tether;
pub mod upgrades;
mod weapons;

pub use brain::Brain;
pub use chain::{Chain, Part};
pub use civ::{CIV_CAP, Raid, RaidStage, TerritoryReport, verdict};
pub use civmine::Cache;
pub use cues::Cue;
pub use ecology::{BaseState, GUARDIAN_COST, TURRET_ANGLES};
pub use food::{FOOD_RADIUS, Food, fertility};
pub use growth::Egg;
pub use guide::{Bearing, GuideKind, MAX_MINERAL_ARROWS, MAX_THREAT_ARROWS, proximity};
pub use loot::{Notice, Pickup};
pub use mining::{Beam, Cargo, Lode, Material};
pub use pads::{
    Bench, BenchPanel, BenchRow, BenchTab, HIDE_SIGHT, KIT_PRICE, LAND_RANGE, MAX_PADS, PAD_HP,
    Pad, PadHint, PadKey, PadState, STASH_CAP, price_text,
};
pub use ping::{ECHO_LIFE, Echo, EchoKind, PING_COOLDOWN, PING_RANGE, RING_SPEED};
pub use root::{Root, STAND as ROOT_STAND};
pub use tether::{Cord, STRONG_CORD, Tether, TetherKind};
use upgrades::{Item, Loadout, Stats};
pub use weapons::{Mine, Shape};

use crate::genome::{Diet, Genome, Species};
use crate::territory::{CivRole, Fall, Territory};
use crate::world::{self, Phenotype, Rng, SectorId, SectorParams};
use crate::world::{BaseKind, RockKind};
use bevy::prelude::Vec2;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

const MAX_BULLETS: usize = 512;
const MAX_EFFECTS: usize = 128;
/// Hard ceiling on loaded bodies; shattering and breeding stop short of it.
const MAX_BODIES: usize = 1500;
/// A destroyed rock splits into pieces this much smaller, unless they would be tiny.
/// Radians per second a planetoid turns.
const PLANETOID_SPIN: f32 = 0.02;
const SHARD_FACTOR: f32 = 0.62;
const MIN_SHARD_RADIUS: f32 = 13.0;
/// Rocks colliding faster than this (closing speed) take damage from the impact.
const ROCK_SHATTER_SPEED: f32 = 300.0;
/// Creatures bred by a base wander no farther than this from it before turning back.
const HOME_LEASH: f32 = 800.0;
/// Enemies this close to a destroyed base lose their bearings for a while.
const ECOSYSTEM_RADIUS: f32 = 2200.0;
const PLAYER_SPEED: f32 = 460.0;
/// A wall segment's hull per unit of radius on top of a rock's 1.6, before depth.
pub const WALL_HULL: f32 = 5.0;
/// A destroyed crystal's burst.
const CRYSTAL_BLAST: f32 = 150.0;
const CRYSTAL_DAMAGE: f32 = 30.0;
/// An inhabited rock hatches when the ship comes this close.
const HUSK_TRIGGER: f32 = 320.0;
/// Damage per ram level at a leisurely approach; faster impacts hurt more.
const RAM_DAMAGE: f32 = 16.0;
/// Fling strength per level of lunatic field (a Lunatic's own is 1).
const AURA_FLING: f32 = 0.45;
/// Spread shots fan this far apart and each carry a share of full damage; flank and
/// stern guns likewise.
const SPREAD_ANGLE: f32 = 0.14;
const SPREAD_SHARE: f32 = 0.7;
const SIDE_SHARE: f32 = 0.6;
const NEEDLE_SHARE: f32 = 0.28;
/// Seekers notice targets this close, and bend this fast per level.
const SEEK_RANGE: f32 = 650.0;
const SEEK_TURN: f32 = 2.4;
/// Health per second a dust-grazing creature recovers.
const DUST_HEAL: f32 = 1.5;
/// A body touched by a flinging creature is thrown at least this fast (scaled by the
/// fling gene), and never faster than the cap.
const FLING_SPEED: f32 = 520.0;
const FLING_MAX_SPEED: f32 = 1000.0;
/// Whatever the genes say, nothing is thrown faster than this.
const FLING_HARD_CAP: f32 = 1400.0;
/// Half-extent of the region around the player whose sectors are simulated. It must
/// exceed the close combat view so loading and freezing happen off screen there.
/// Wider overview cameras do not expand this region or change simulation rules.
pub const ACTIVE_HALF: Vec2 = Vec2::new(2200.0, 1500.0);
/// Sectors farther than this (in sectors) from the player are unloaded.
const UNLOAD_DISTANCE: u32 = 2;
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub thrust: f32,
    pub turn: f32,
    pub brake: bool,
    pub fire: bool,
    /// Hold the mining beam. It excludes firing: while held, the guns stay quiet.
    pub mine: bool,
    /// Optional heading vector, allowing pointer/controller aiming without input APIs.
    pub aim_direction: Option<Vec2>,
    /// Optional thrust vector (length 0..1) independent of heading, for twin-stick play.
    /// When present it replaces `thrust`, which only pushes along the heading.
    pub move_direction: Option<Vec2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Player,
    /// A living thing; its genome decides what it is and does.
    Creature,
    Asteroid,
    BlackHole,
    /// An ecosystem base: immovable, breeds creatures and harvests debris.
    Base,
}

/// The player's augmentations as the physics sees them; neutral for everything else.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rig {
    /// Multiplier on damage taken.
    pub guard: f32,
    /// Ramming level: contact hurts what the ship touches.
    pub ram: u8,
    /// Lunatic field level: what touches the ship is flung.
    pub aura: u8,
    /// Gravity wells barely tug and cannot hurt.
    pub ballast: bool,
}

impl Default for Rig {
    fn default() -> Self {
        Self {
            guard: 1.0,
            ram: 0,
            aura: 0,
            ballast: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Body {
    pub id: u64,
    pub kind: BodyKind,
    pub position: Vec2,
    pub velocity: Vec2,
    pub angle: f32,
    pub radius: f32,
    pub health: f32,
    pub max_health: f32,
    pub shield: f32,
    pub max_shield: f32,
    pub mass: f32,
    /// Which sector spawn this body came from, if any; used to remember kills.
    pub origin: Option<(SectorId, u32)>,
    /// Fixed in place regardless of impacts (the stones of a nest).
    pub pinned: bool,
    /// Jointed creatures: the chain this body belongs to, whether it trails a head, and
    /// its index within the creature (0 is the head), which decides hardpoints.
    pub chain: Option<u32>,
    pub follower: bool,
    pub part: u8,
    /// Bases only.
    pub base: Option<BaseState>,
    /// Where a base-bred creature considers home.
    pub home: Option<Vec2>,
    /// Seconds of disarray left after a base fell, and where the disaster was.
    pub panic: f32,
    pub panic_from: Vec2,
    /// True while this enemy is hunting the player rather than going about its business.
    pub alert: bool,
    /// The environment's expression of the genome: weights from the sector that spawned it.
    pub genes: Phenotype,
    /// Creatures: the heritable description of everything the body is and does.
    pub genome: Genome,
    /// Creatures: lineage id of its species (schooling is with the same lineage).
    pub species: u64,
    /// A juvenile's tending parent.
    pub parent: Option<u64>,
    /// Badly hurt and attacking with abandon (creatures with a rage gene).
    pub enraged: bool,
    /// False when the body sits in a sector outside the active region.
    pub active: bool,
    pub rig: Rig,
    /// Asteroids: what the rock is made of, and what lives in a husk.
    pub rock: RockKind,
    /// Asteroids: the ore it holds (derived from size, see `mining`).
    pub lode: Lode,
    pub den: Option<(Species, u8)>,
    /// Creatures: stored energy, up to `max_energy` (which scales with size). It drains with
    /// time and movement and is restored by eating; see `food`.
    pub energy: f32,
    pub max_energy: f32,
    /// Creatures: fed by the base that bred them, so they never tire or starve.
    pub provisioned: bool,
    /// Eaten or starved: leaves without score, loot or a fanfare.
    pub consumed: bool,
    /// Juveniles: the genome this body grows into (`genome` is the juvenile's until then),
    /// seconds since birth and progress toward maturity in [0, 1]. See `growth`.
    pub adult: Option<Genome>,
    pub age: f32,
    pub growth: f32,
    /// Creatures: how many births removed from its founding species it is.
    pub generation: u16,
    /// Learners only: the net that predicts where the ship will be. Boxed so a body that does
    /// not learn pays one pointer; see `brain`.
    pub brain: Option<Box<Brain>>,
    /// Creatures: the host rock or planetoid this one clings to, if any; see `root`.
    pub root: Option<Root>,
    /// Wall segments and turrets of a fortified city; see `fortress`.
    pub fort: Option<crate::fortress::FortPart>,
    /// Seconds before a creature that let go of a host may cling again.
    unrooted: f32,
    /// Seconds until an adult may next try to reproduce.
    breed_clock: f32,
    /// Seconds spent with no energy at all.
    starving: f32,
    /// Seconds until a predator may bite again.
    bite_clock: f32,
    wander: f32,
    /// Rotation of a spiral emitter.
    spin: f32,
    fire_cooldown: f32,
    contact_cooldown: f32,
    since_hit: f32,
    /// Seconds a harm that does no damage (a mining beam on its host) still counts.
    provoked: f32,
    brood_timer: f32,
}

#[derive(Clone, Debug)]
pub struct Bullet {
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    pub friendly: bool,
    pub remaining: f32,
    pub damage: f32,
    /// Bodies it can still pass through, how hard it bends toward targets, and the level of
    /// its burst on impact (friendly shots only).
    pub pierce: u8,
    pub homing: u8,
    pub blast: u8,
    pub shape: Shape,
    /// Hostile shots that steer toward the ship, in radians per second.
    pub seek: f32,
    /// Radius of the explosion where the shot ends (zero for none).
    pub burst: f32,
    /// A friendly shot that reaches it destroys it (missiles).
    pub fragile: bool,
    /// Bodies already pierced, so a shot inside one does not strike it every tick.
    struck: [u64; 4],
}

impl Bullet {
    pub fn friendly(position: Vec2, velocity: Vec2, remaining: f32) -> Self {
        Self {
            position,
            velocity,
            radius: 3.0,
            friendly: true,
            remaining,
            damage: Stats::BASE.damage,
            pierce: 0,
            homing: 0,
            blast: 0,
            shape: Shape::Pellet,
            seek: 0.0,
            burst: 0.0,
            fragile: false,
            struck: [0; 4],
        }
    }

    pub fn hostile(position: Vec2, velocity: Vec2, remaining: f32, damage: f32) -> Self {
        Self {
            radius: 4.0,
            friendly: false,
            damage,
            ..Self::friendly(position, velocity, remaining)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Impact,
    Explosion,
    Respawn,
    /// A birth or hatching.
    Birth,
    /// A juvenile coming of age.
    Mature,
    /// Two mates pairing, a faint pulse at each.
    Pair,
}

#[derive(Clone, Debug)]
pub struct Effect {
    pub position: Vec2,
    pub radius: f32,
    pub remaining: f32,
    pub lifetime: f32,
    pub kind: EffectKind,
}

pub struct Game {
    pub bodies: Vec<Body>,
    pub bullets: Vec<Bullet>,
    pub effects: Vec<Effect>,
    /// Sound-worthy events since the adapter last called `drain_cues`.
    pub cues: Vec<Cue>,
    pub tethers: Vec<Tether>,
    pub chains: BTreeMap<u32, Chain>,
    /// Drops waiting to be collected.
    pub pickups: Vec<Pickup>,
    pub mines: Vec<Mine>,
    /// What is bolted to the ship, and the stats that follow from it.
    pub loadout: Loadout,
    pub stats: Stats,
    /// Recent things worth telling the player about (pickups, wrecks).
    pub notices: Vec<Notice>,
    /// What the ship carries, and the mining beam if it is on.
    pub cargo: Cargo,
    /// Field repair, pad kits, landing pads, the bench and what the enemy knows of them.
    pad: PadState,
    /// The sonar ring, its echoes and their cache; see `ping`.
    ping: ping::PingState,
    pub beam: Option<Beam>,
    pub score: u64,
    /// Counters for this run, and the extirpations it caused; see `run`.
    pub run: run::RunStats,
    lore: run::Lore,
    pub lives: u32,
    pub game_over: bool,
    pub time: f32,
    pub player_invulnerability: f32,
    /// Last known player position; anchors the active region while the player is dead.
    pub focus: Vec2,
    /// Drifting plankton: food for grazers. Not bodies, so it never collides or counts as one.
    pub food: Vec<Food>,
    /// Eggs waiting to hatch. Not bodies: they drift, can be shot, unload with their
    /// sector and count against the body budget.
    pub eggs: Vec<Egg>,
    /// The territory the ship is in, its display name and its raid clock; see `civ`.
    pub territory: Option<Territory>,
    pub territory_name: String,
    pub raid: Option<Raid>,
    territory_sector: Option<SectorId>,
    /// Lineage -> (territory, role) of every civilization lineage met, and the capital bases
    /// by spawn, the territories met, their lasting falls and their shared brains.
    civ_lineages: HashMap<u64, (u64, CivRole)>,
    civ_bases: HashMap<(SectorId, u32), (u64, CivRole)>,
    /// Walls and turrets of fortified cities by spawn: (territory, role).
    civ_works: HashMap<(SectorId, u32), (u64, CivRole)>,
    /// Per-territory mining: miners, the stash at the capital and the ore budget.
    civ_mining: BTreeMap<u64, civmine::Mining>,
    civ_colors: HashMap<u64, [f32; 3]>,
    civ_territories: HashMap<u64, Territory>,
    civ_fall: HashMap<u64, Fall>,
    civ_brains: HashMap<u64, Box<Brain>>,
    civ_clock: f32,
    civ_rng: Rng,
    seed: u64,
    rng: Rng,
    /// Loot has its own stream, so drops never disturb the gameplay one.
    loot: Rng,
    /// Individual variation of creatures born during play (base broods, hatchlings, litters).
    variation: Rng,
    /// Where new plankton buds, on its own stream.
    growth: Rng,
    /// Reproduction (timing, spacing, rare mode flips) has its own stream too.
    breeding: Rng,
    /// Seconds until the next plankton regrowth pass.
    food_clock: f32,
    /// Cooldowns of the ship's missile pods, mine layer and nova pulse.
    arm_clock: [f32; 3],
    /// Seconds before another weapon switch is accepted (a tiny debounce).
    switch_clock: f32,
    /// Seconds the HUD keeps announcing the last switch or dry fall-back.
    pub arsenal_flash: f32,
    next_id: u64,
    next_chain: u32,
    /// Spawns destroyed so far, per sector, so a sector reloads as it was left.
    fallen: HashMap<SectorId, HashSet<u32>>,
    /// Ore taken from rocks (planetoid budget spent), by spawn, quantized; see `mining`.
    mined: HashMap<(SectorId, u32), f32>,
    /// Seconds the beam has held its target, the target, and the throttle on full-hold notes.
    mine_clock: f32,
    mine_target: Option<u64>,
    mine_note: f32,
    loaded: HashSet<SectorId>,
    active: Vec<SectorId>,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut game = Self {
            bodies: Vec::new(),
            bullets: Vec::with_capacity(MAX_BULLETS),
            effects: Vec::with_capacity(MAX_EFFECTS),
            cues: Vec::new(),
            tethers: Vec::new(),
            chains: BTreeMap::new(),
            pickups: Vec::new(),
            mines: Vec::new(),
            arm_clock: [0.0; 3],
            switch_clock: 0.0,
            arsenal_flash: 0.0,
            loadout: Loadout::default(),
            stats: Stats::BASE,
            notices: Vec::new(),
            cargo: Cargo::default(),
            pad: PadState::default(),
            ping: ping::PingState::default(),
            beam: None,
            mined: HashMap::new(),
            mine_clock: 0.0,
            mine_target: None,
            mine_note: 0.0,
            score: 0,
            run: run::RunStats::default(),
            lore: run::Lore::default(),
            lives: 3,
            game_over: false,
            time: 0.0,
            player_invulnerability: 2.0,
            focus: Vec2::ZERO,
            food: Vec::new(),
            eggs: Vec::new(),
            territory: None,
            territory_name: String::new(),
            raid: None,
            territory_sector: None,
            civ_lineages: HashMap::new(),
            civ_bases: HashMap::new(),
            civ_works: HashMap::new(),
            civ_mining: BTreeMap::new(),
            civ_colors: HashMap::new(),
            civ_territories: HashMap::new(),
            civ_fall: HashMap::new(),
            civ_brains: HashMap::new(),
            civ_clock: 0.0,
            civ_rng: Rng::new(seed ^ crate::territory::TERRITORY_SALT),
            seed,
            rng: Rng::new(seed),
            loot: Rng::new(seed ^ loot::LOOT_SALT),
            variation: Rng::new(seed ^ crate::genome::INDIVIDUAL_SALT),
            growth: Rng::new(seed ^ world::FOOD_SALT),
            breeding: Rng::new(seed ^ growth::BREED_SALT),
            food_clock: 0.0,
            next_id: 1,
            next_chain: 1,
            fallen: HashMap::new(),
            loaded: HashSet::new(),
            active: Vec::new(),
        };
        game.stream_sectors();
        game.spawn_player(Vec2::ZERO);
        game
    }

    /// Replays the initial seed so a restart is useful for comparing tuning changes.
    pub fn reset(&mut self) {
        *self = Self::new(self.seed);
    }

    pub fn player(&self) -> Option<&Body> {
        self.bodies
            .iter()
            .find(|body| body.kind == BodyKind::Player)
    }

    pub fn sector(&self) -> SectorId {
        SectorId::containing(self.focus)
    }

    pub fn body(&self, id: u64) -> Option<&Body> {
        self.bodies.iter().find(|body| body.id == id)
    }

    /// Moves the player instantly. Used by smoke runs and tests to visit distant places.
    pub fn teleport(&mut self, position: Vec2) {
        if let Some(player) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            player.position = position;
            player.velocity = Vec2::ZERO;
        }
        self.focus = position;
    }

    /// Latent parameters of the sector the player is in.
    pub fn params(&self) -> SectorParams {
        world::latent(self.seed, self.sector())
    }

    /// Enemies currently being simulated.
    pub fn active_enemies(&self) -> usize {
        self.bodies
            .iter()
            .filter(|body| body.active && body.kind == BodyKind::Creature)
            .count()
    }

    /// Intended for a fixed 1/60-second caller. Rejects invalid durations and caps
    /// a single step at 50 ms, preventing a resumed window from causing huge jumps.
    pub fn step(&mut self, dt: f32, input: Input) {
        if self.game_over || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.05);
        // Mining and firing exclude each other: the beam wins while it is held.
        let input = Input {
            fire: input.fire && !input.mine,
            ..input
        };
        let in_flight = self.bullets.len();
        let mut ship_before = self.player().map(|p| (p.shield, p.health));
        self.time += dt;
        self.player_invulnerability = (self.player_invulnerability - dt).max(0.0);
        for effect in &mut self.effects {
            effect.remaining -= dt;
        }
        self.effects.retain(|effect| effect.remaining > 0.0);
        self.update_ping(dt);
        self.stream_sectors();
        self.note_sector();
        let start = self.player().map(|p| p.position);
        self.update_civilizations(dt);
        self.update_loadout(dt, &input);
        let recharge = self.stats.recharge;
        let beaming = self.beam.is_some();
        for body in self.bodies.iter_mut().filter(|b| b.active) {
            body.fire_cooldown = (body.fire_cooldown - dt).max(0.0);
            body.contact_cooldown = (body.contact_cooldown - dt).max(0.0);
            body.panic = (body.panic - dt).max(0.0);
            body.since_hit += dt;
            body.provoked = (body.provoked - dt).max(0.0);
            if body.since_hit > 2.0 && !(beaming && body.kind == BodyKind::Player) {
                let rate = if body.kind == BodyKind::Player {
                    recharge
                } else {
                    6.0
                };
                body.shield = (body.shield + dt * rate).min(body.max_shield);
            }
            if body.kind == BodyKind::Creature && body.genome.diet == Diet::Dust {
                body.health = (body.health + dt * DUST_HEAL).min(body.max_health);
            }
        }
        self.update_pads(dt, &input);
        let shots = (self.bullets.len(), self.mines.len());
        self.control_player(dt, input);
        let drained = self.update_mining(dt, input.mine);
        if let Some(before) = ship_before.as_mut() {
            before.0 -= drained;
        }
        self.update_arms(dt, input.fire);
        self.pad_noise(shots.0, shots.1);
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
        self.steer_creatures(dt);
        self.update_civ_mining(dt);
        self.update_roots(dt);
        self.update_bases(dt);
        self.update_turrets(dt);
        self.tend_broods(dt);
        self.graze();
        self.update_food(dt);
        self.update_metabolism(dt);
        self.graze_plankton();
        self.hunt(dt);
        self.update_growth(dt);
        self.update_reproduction(dt);
        self.update_eggs(dt);
        self.update_tethers(dt);
        self.update_chains(dt);
        self.fire_weapons();
        self.cue_new_shots(in_flight);
        self.apply_gravity(dt);
        for body in self.bodies.iter_mut().filter(|b| b.active) {
            if !is_fixed(body) {
                body.position += body.velocity * dt;
            }
            if body.kind == BodyKind::Asteroid && !body.pinned {
                // Flung rocks slowly lose their excess speed rather than ricocheting forever.
                let speed = body.velocity.length();
                if speed > 120.0 {
                    body.velocity *= (120.0 + (speed - 120.0) * (-0.5 * dt).exp()) / speed;
                }
                body.angle += dt * 0.3;
            } else if body.rock == RockKind::Planetoid {
                body.angle += dt * PLANETOID_SPIN;
            }
        }
        // Rooted life rides its host, and again after contacts have shoved the host.
        self.sync_roots();
        self.contain_in_active_region();
        self.resolve_contacts();
        self.sync_roots();
        // After contacts, so an impact cannot leave a joint stretched past its limit.
        self.constrain_chains();
        self.move_bullets(dt);
        self.update_mines(dt);
        self.update_husks();
        self.update_pickups(dt);
        let travelled = match (start, self.player()) {
            (Some(from), Some(ship)) => from.distance(ship.position),
            _ => 0.0,
        };
        let taken = match (ship_before, self.player()) {
            (Some((shield, health)), Some(ship)) => {
                (shield + health - ship.shield - ship.health.max(0.0)).max(0.0)
            }
            (Some((shield, health)), None) => shield + health,
            _ => 0.0,
        };
        self.note_step(dt, travelled, taken);
        self.remove_destroyed();
        self.cue_player_damage(ship_before);
    }

    /// Loads sectors the player can reach, unloads distant ones, and flags which
    /// bodies take part in this tick.
    fn stream_sectors(&mut self) {
        if let Some(player) = self.player() {
            self.focus = player.position;
        }
        let home = self.sector();
        self.active = SectorId::overlapping(self.focus, ACTIVE_HALF);
        for id in self.active.clone() {
            if self.loaded.insert(id) {
                self.populate(id);
                self.populate_food(id);
            }
        }
        self.loaded
            .retain(|id| id.chebyshev_distance(home) <= UNLOAD_DISTANCE);
        self.mines.retain(|m| {
            SectorId::containing(m.position).chebyshev_distance(home) <= UNLOAD_DISTANCE
        });
        self.pickups.retain(|p| {
            SectorId::containing(p.position).chebyshev_distance(home) <= UNLOAD_DISTANCE
        });
        let loaded = &self.loaded;
        self.food.retain(|f| {
            let at = SectorId::containing(f.position);
            at.chebyshev_distance(home) <= UNLOAD_DISTANCE || loaded.contains(&at)
        });
        self.eggs.retain(|e| {
            let at = SectorId::containing(e.position);
            at.chebyshev_distance(home) <= UNLOAD_DISTANCE || loaded.contains(&at)
        });
        self.bodies.retain(|body| {
            body.kind == BodyKind::Player
                || SectorId::containing(body.position).chebyshev_distance(home) <= UNLOAD_DISTANCE
                || loaded.contains(&SectorId::containing(body.position))
        });
        for body in &mut self.bodies {
            body.active = body.kind == BodyKind::Player
                || self.active.contains(&SectorId::containing(body.position));
        }
    }

    /// Bodies freeze the moment they leave the active sectors, which used to leave
    /// wandering creatures lined up along the border. Turn them back at the edge instead.
    fn contain_in_active_region(&mut self) {
        let Some((min, max)) = self.active_bounds() else {
            return;
        };
        for body in self.bodies.iter_mut().filter(|b| b.active) {
            if body.kind == BodyKind::Player || body.follower || is_fixed(body) {
                continue;
            }
            let mut turned = false;
            let p = &mut body.position;
            let v = &mut body.velocity;
            if p.x < min.x || p.x > max.x {
                p.x = p.x.clamp(min.x, max.x);
                v.x = if p.x == min.x { v.x.abs() } else { -v.x.abs() };
                turned = true;
            }
            if p.y < min.y || p.y > max.y {
                p.y = p.y.clamp(min.y, max.y);
                v.y = if p.y == min.y { v.y.abs() } else { -v.y.abs() };
                turned = true;
            }
            if turned && body.kind == BodyKind::Creature {
                body.wander = body.velocity.y.atan2(body.velocity.x);
            }
        }
    }

    /// Corners of the rectangle the active sectors cover.
    fn active_bounds(&self) -> Option<(Vec2, Vec2)> {
        let first = self.active.first()?;
        let (mut low, mut high) = (*first, *first);
        for id in &self.active {
            low = SectorId {
                x: low.x.min(id.x),
                y: low.y.min(id.y),
            };
            high = SectorId {
                x: high.x.max(id.x),
                y: high.y.max(id.y),
            };
        }
        let margin = Vec2::splat(world::SECTOR_SIZE / 2.0 - 1.0);
        Some((low.center() - margin, high.center() + margin))
    }

    fn populate(&mut self, id: SectorId) {
        let fallen = self.fallen.get(&id).cloned().unwrap_or_default();
        // A spawn whose creature wandered off but is still loaded must not be duplicated.
        let present: HashSet<u32> = self
            .bodies
            .iter()
            .filter_map(|b| b.origin.filter(|(q, _)| *q == id).map(|(_, i)| i))
            .collect();
        let mut made: HashMap<u32, u64> = HashMap::new();
        if let Some(t) = world::territory(self.seed, id) {
            self.register_territory(t);
        }
        for spawn in world::generate(self.seed, id) {
            if fallen.contains(&spawn.index) || present.contains(&spawn.index) {
                continue;
            }
            // A ruined civilization does not come back, though its walls stand as ruins.
            if let Some(tag) = spawn.civ
                && tag.role != CivRole::Wall
                && self.civ_standing(tag.territory) == crate::territory::Standing::Fallen
            {
                continue;
            }
            // Fortresses give way before the body cap: pieces are left out (opening the
            // wall, never closing it) once the world is nearly full.
            if spawn.fort.is_some()
                && self.bodies.len() + self.food.len() + self.eggs.len() + fortress::RESERVE
                    >= MAX_BODIES
            {
                continue;
            }
            let mut body = match &spawn.species {
                Some(species) => match spawn.rooted.filter(|r| r.growth < 1.0) {
                    // A young rooter clings from the start, partway grown.
                    Some(rooting) => {
                        let mut young = self.newborn(
                            species.lineage,
                            species.generation,
                            spawn.phenotype,
                            species.genome,
                            None,
                            spawn.position,
                        );
                        young.growth = rooting.growth;
                        young.shape_to_growth();
                        young
                    }
                    None => self.make_creature(species, spawn.position),
                },
                None => self.make_body(spawn.kind, spawn.position),
            };
            if let Some(radius) = spawn.radius {
                body.radius = radius;
                body.mass = radius * 0.6;
                body.health = radius * 1.6;
                body.max_health = body.health;
            }
            if body.kind == BodyKind::Asteroid {
                body.rock = spawn.rock;
                body.den = spawn.den;
                let (toughness, density) = match spawn.rock {
                    RockKind::Plain => (1.0, 1.0),
                    RockKind::Ice => (0.7, 0.8),
                    RockKind::Ore => (1.6, 2.2),
                    RockKind::Crystal => (0.55, 1.0),
                    RockKind::Husk => (1.5, 1.2),
                    // Heavy enough to read as fixed; it never takes damage anyway.
                    RockKind::Planetoid => (1.0, 6.0),
                    // Tough, and tougher deeper in; a breach takes real effort.
                    RockKind::Wall => (WALL_HULL * spawn.phenotype.threat.max(1.0).sqrt(), 40.0),
                };
                body.health *= toughness;
                body.max_health = body.health;
                body.mass *= density;
            }
            body.velocity = spawn.velocity;
            body.genes = spawn.phenotype;
            body.pinned = spawn.pinned;
            body.origin = Some((id, spawn.index));
            self.apply_mined(&mut body);
            body.angle = self.rng.f32() * TAU;
            body.wander = body.angle;
            body.fire_cooldown = 1.0 + self.rng.f32() * 2.0;
            if let (Some(brood), Some(guardian)) = (spawn.brood, spawn.guardian) {
                let timer = 2.0 + self.rng.f32() * 4.0;
                let kind = spawn.base_kind.unwrap_or(BaseKind::Hive);
                body.health = kind.hull();
                body.max_health = body.health;
                body.base = Some(BaseState::new(brood, guardian, timer).of_kind(kind, spawn.arms));
            } else if spawn.base_kind == Some(BaseKind::Turret) {
                body.health = BaseKind::Turret.hull();
                body.max_health = body.health;
                body.base = Some(BaseState::turret(spawn.arms, spawn.index));
            }
            body.fort = spawn.fort;
            if let Some(crate::fortress::FortPart {
                kind: crate::fortress::PartKind::Turret { facing, .. },
                ..
            }) = spawn.fort
            {
                body.angle = facing;
            }
            if body.kind == BodyKind::Creature
                && (body.genome.social == crate::genome::Social::Dweller
                    || body.genome.nest == crate::genome::Nest::Base)
            {
                body.home = Some(spawn.position);
            }
            if let (Some(rooting), Some(&host)) =
                (spawn.rooted, spawn.rooted.and_then(|r| made.get(&r.host)))
            {
                self.root_body(&mut body, host, rooting.angle);
            }
            if let Some(tag) = spawn.civ {
                if matches!(tag.role, CivRole::Wall | CivRole::Turret) {
                    self.civ_works
                        .insert((id, spawn.index), (tag.territory, tag.role));
                } else if spawn.base_kind.is_some() {
                    self.civ_bases
                        .insert((id, spawn.index), (tag.territory, tag.role));
                }
                self.civ_dress(&mut body, tag, spawn.species.as_ref());
            }
            let head = self.add_body(body);
            made.insert(spawn.index, head);
            if let Some(&partner) = spawn.link.and_then(|i| made.get(&i)) {
                self.tethers.push(Tether::link(partner, head));
            }
        }
        self.reload_pads(id);
    }

    /// Remembers that a spawn was destroyed. A chain counts only once every segment is gone.
    fn record_fallen(&mut self, body: &Body) {
        let Some((sector, index)) = body.origin else {
            return;
        };
        if body
            .chain
            .is_some_and(|c| self.bodies.iter().any(|b| b.chain == Some(c)))
        {
            return;
        }
        self.fallen.entry(sector).or_default().insert(index);
    }

    /// Breaks a destroyed rock into smaller free-flying pieces.
    fn shatter(&mut self, rock: &Body) {
        let radius = rock.radius * SHARD_FACTOR;
        if radius < MIN_SHARD_RADIUS || self.bodies.len() >= MAX_BODIES {
            return;
        }
        // Ice splinters into more pieces.
        let pieces =
            if rock.radius >= 45.0 { 3 } else { 2 } + u32::from(rock.rock == RockKind::Ice);
        let start = self.rng.range(0.0, TAU);
        for piece in 0..pieces {
            if self.bodies.len() >= MAX_BODIES {
                break;
            }
            let angle = start + piece as f32 * TAU / pieces as f32 + self.rng.range(-0.3, 0.3);
            let direction = Vec2::from_angle(angle);
            let mut shard = self.make_body(
                BodyKind::Asteroid,
                rock.position + direction * rock.radius * 0.5,
            );
            shard.radius = radius;
            shard.rock = if rock.rock == RockKind::Husk {
                RockKind::Plain
            } else {
                rock.rock
            };
            shard.mass = radius
                * 0.6
                * if shard.rock == RockKind::Ore {
                    2.2
                } else {
                    1.0
                };
            shard.lode = Self::fragment_lode(rock, pieces, radius);
            shard.health = radius * 1.6 * 0.8;
            shard.max_health = shard.health;
            shard.velocity = rock.velocity + direction * self.rng.range(70.0, 150.0);
            shard.angle = self.rng.range(0.0, TAU);
            self.bodies.push(shard);
        }
    }

    fn control_player(&mut self, dt: f32, input: Input) {
        // The trigger pull is billed first: a dry profile falls back to stock fire right
        // away, so the same shot still goes out.
        let ready = input.fire
            && self.bullets.len() < MAX_BULLETS
            && self.player().is_some_and(|p| p.fire_cooldown <= 0.0);
        if ready {
            self.pay_volley();
        }
        let stats = self.stats;
        let Some(player) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) else {
            return;
        };
        if let Some(aim) = input
            .aim_direction
            .filter(|aim| aim.is_finite() && aim.length_squared() > 0.001)
        {
            player.angle = aim.y.atan2(aim.x);
        } else if input.turn.is_finite() {
            player.angle += input.turn.clamp(-1.0, 1.0) * stats.turn * dt;
        }
        player.angle = player.angle.rem_euclid(TAU);
        let direction = Vec2::new(player.angle.cos(), player.angle.sin());
        let thrust = if input.thrust.is_finite() {
            input.thrust.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let push = match input.move_direction.filter(|m| m.is_finite()) {
            Some(m) => m.clamp_length_max(1.0),
            None => direction * thrust,
        };
        player.velocity += push * stats.thrust * dt;
        player.velocity *= (-dt * if input.brake { 5.0 } else { 0.07 }).exp();
        // Thrust cannot exceed top speed, but a fling is allowed to carry the ship past it
        // and bleeds off smoothly instead of snapping back.
        let speed = player.velocity.length();
        if speed > stats.top_speed {
            let kept = stats.top_speed + (speed - stats.top_speed) * (-2.0 * dt).exp();
            player.velocity *= kept / speed;
        }
        if input.fire && player.fire_cooldown <= 0.0 && self.bullets.len() < MAX_BULLETS {
            let (position, velocity, radius) = (player.position, player.velocity, player.radius);
            // A needler trades rate of fire for a dense burst.
            player.fire_cooldown = stats.fire_period * if stats.needles > 0 { 1.5 } else { 1.0 };
            for (offset, share, shape) in volley(&stats, &mut self.rng) {
                if self.bullets.len() >= MAX_BULLETS {
                    break;
                }
                let aim = Vec2::from_angle(player.angle + offset);
                let needle = shape == Shape::Needle;
                let speed = stats.shot_speed * if needle { 1.5 } else { 1.0 };
                let mut shot = Bullet::friendly(
                    position + aim * (radius + 5.0),
                    velocity + aim * speed,
                    stats.shot_life,
                );
                shot.shape = shape;
                if needle {
                    shot.radius = 2.0;
                }
                shot.damage = stats.damage * share;
                shot.pierce = stats.pierce;
                shot.homing = stats.homing;
                shot.blast = stats.blast;
                self.bullets.push(shot);
            }
        }
    }

    fn apply_gravity(&mut self, dt: f32) {
        let holes: Vec<_> = self
            .bodies
            .iter()
            .filter(|b| b.active && b.kind == BodyKind::BlackHole)
            .map(|b| b.position)
            .collect();
        let invulnerability = self.player_invulnerability;
        for body in self.bodies.iter_mut().filter(|b| b.active) {
            if is_fixed(body) {
                continue;
            }
            for &position in &holes {
                let offset = position - body.position;
                let distance_squared = offset.length_squared();
                if distance_squared < 550.0 * 550.0 {
                    let pull = offset * (7_000_000.0 / (distance_squared + 2500.0).powf(1.5));
                    // Negative mass is repelled by gravity; ballast mostly shrugs it off.
                    let ballast = if body.rig.ballast { 0.2 } else { 1.0 };
                    body.velocity += pull.clamp_length_max(350.0) * dt * mass_sign(body) * ballast;
                    body.velocity = body.velocity.clamp_length_max(650.0);
                    if !body.rig.ballast && distance_squared < (body.radius + 28.0).powi(2) {
                        damage(body, 35.0 * dt, invulnerability);
                    }
                }
            }
        }
    }

    fn resolve_contacts(&mut self) {
        let invulnerability = self.player_invulnerability;
        let mut flings = Vec::new();
        let mut rammed = 0.0;
        for i in 0..self.bodies.len() {
            let (before, after) = self.bodies.split_at_mut(i + 1);
            let a = &mut before[i];
            if !a.active {
                continue;
            }
            for b in after.iter_mut().filter(|b| b.active) {
                let offset = b.position - a.position;
                let radius = a.radius + b.radius;
                let distance_squared = offset.length_squared();
                if distance_squared >= radius * radius {
                    continue;
                }
                let distance = distance_squared.sqrt();
                let normal = if distance > 0.001 {
                    offset / distance
                } else {
                    Vec2::X
                };
                // Segments of one creature are joined, not colliding.
                if a.chain.is_some() && a.chain == b.chain {
                    continue;
                }
                if clings(a, b) {
                    continue;
                }
                let inverse_a = inverse_mass(a);
                let inverse_b = inverse_mass(b);
                let inverse_sum = inverse_a + inverse_b;
                if inverse_sum <= 0.0 {
                    continue;
                }
                let separation = normal * (radius - distance + 0.01) / inverse_sum;
                a.position -= separation * inverse_a;
                b.position += separation * inverse_b;
                // Contact fling is a continuous trait: whichever body flings harder throws the
                // other (equals bounce), and negative mass adds to it.
                let (fling_a, fling_b) = (fling_strength(a), fling_strength(b));
                let flinger = if fling_a > fling_b && !is_fixed(b) {
                    Some(true)
                } else if fling_b > fling_a && !is_fixed(a) {
                    Some(false)
                } else {
                    None
                };
                let closing_speed = (b.velocity - a.velocity).dot(normal);
                if let Some(a_flings) = flinger {
                    // The flinger throws whatever touches it along a wild vector and
                    // recoils. A cooldown stops repeat flings.
                    let (crazy, victim, normal) = if a_flings {
                        (&mut *a, &mut *b, normal)
                    } else {
                        (&mut *b, &mut *a, -normal)
                    };
                    if crazy.contact_cooldown <= 0.0 {
                        let strength = fling_strength(crazy);
                        let relative = (victim.velocity - crazy.velocity).length();
                        let angle = self.rng.range(-1.0, 1.0) * crazy.genome.fling_chaos;
                        let direction = Vec2::from_angle(angle).rotate(normal);
                        let speed = ((FLING_SPEED + relative * 0.8).min(FLING_MAX_SPEED)
                            * strength)
                            .min(FLING_HARD_CAP);
                        victim.velocity = direction * speed;
                        if victim.kind == BodyKind::Asteroid {
                            damage(victim, 40.0 * strength, 0.0);
                        }
                        crazy.velocity = -direction * speed * 0.4;
                        crazy.contact_cooldown = 0.4;
                        flings.push(victim.position);
                    }
                } else if closing_speed < 0.0 {
                    let impulse = normal * (-1.7 * closing_speed / inverse_sum);
                    a.velocity -= impulse * inverse_a;
                    b.velocity += impulse * inverse_b;
                    if a.kind == BodyKind::Asteroid && b.kind == BodyKind::Asteroid {
                        let hit = (-closing_speed - ROCK_SHATTER_SPEED).max(0.0) * 0.35;
                        damage(a, hit, 0.0);
                        damage(b, hit, 0.0);
                    }
                }
                if a.kind == BodyKind::Player && a.contact_cooldown <= 0.0 {
                    rammed += ram_contact(a, b, closing_speed, invulnerability);
                }
                if b.kind == BodyKind::Player && b.contact_cooldown <= 0.0 {
                    rammed += ram_contact(b, a, closing_speed, invulnerability);
                }
            }
        }
        self.run.damage_dealt += rammed;
        for position in flings {
            self.effect(position, 30.0, 0.3, EffectKind::Impact);
        }
    }

    fn move_bullets(&mut self, dt: f32) {
        self.shoot_eggs(dt);
        let mut impacts = Vec::new();
        // (where, radius, damage, body already struck, from the ship's side)
        let mut blasts: Vec<(Vec2, f32, f32, u64, bool)> = Vec::new();
        let cords = self.cord_segments();
        let ship = self.player().map(|p| p.position);
        // Seekers home on hostile creatures and bases; gather them only if any are in flight.
        let targets: Vec<Vec2> = if self.bullets.iter().any(|b| b.friendly && b.homing > 0) {
            self.bodies
                .iter()
                .filter(|b| b.active && matches!(b.kind, BodyKind::Creature | BodyKind::Base))
                .map(|b| b.position)
                .collect()
        } else {
            Vec::new()
        };
        for bullet in &mut self.bullets {
            if bullet.friendly && bullet.homing > 0 {
                steer_seeker(bullet, &targets, dt);
            } else if !bullet.friendly
                && bullet.seek > 0.0
                && let Some(ship) = ship
            {
                let turn = bullet.velocity.angle_to(ship - bullet.position);
                let limit = bullet.seek * dt;
                bullet.velocity =
                    Vec2::from_angle(turn.clamp(-limit, limit)).rotate(bullet.velocity);
            }
            let previous = bullet.position;
            bullet.position += bullet.velocity * dt;
            bullet.remaining -= dt;
            let mut hit: Option<(usize, f32)> = None;
            for (index, body) in self.bodies.iter().enumerate().filter(|(_, b)| b.active) {
                let target = if bullet.friendly {
                    body.kind != BodyKind::Player
                } else {
                    matches!(
                        body.kind,
                        BodyKind::Player | BodyKind::Asteroid | BodyKind::BlackHole
                    )
                };
                if !target || body.health <= 0.0 || bullet.struck.contains(&body.id) {
                    continue;
                }
                if let Some(fraction) = segment_circle(
                    previous,
                    bullet.position,
                    body.position,
                    body.radius + bullet.radius,
                ) && hit.is_none_or(|(_, best)| fraction < best)
                {
                    hit = Some((index, fraction));
                }
            }
            // Mines use the swept path too: a rail particle can cross one in a tick.
            if bullet.friendly {
                let mine_hit = self
                    .mines
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| !m.friendly)
                    .filter_map(|(i, m)| {
                        segment_circle(previous, bullet.position, m.position, 12.0 + bullet.radius)
                            .map(|t| (i, t))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((index, fraction)) = mine_hit
                    && hit.is_none_or(|(_, body_fraction)| fraction < body_fraction)
                {
                    self.mines[index].fuse = Some(self.mines[index].fuse.unwrap_or(0.05).min(0.05));
                    bullet.remaining = 0.0;
                    impacts.push(previous.lerp(bullet.position, fraction));
                    continue;
                }
            }
            if hit.is_none() && bullet.friendly {
                for &(index, from, to) in &cords {
                    if tether::segment_distance(previous, bullet.position, from, to)
                        < bullet.radius + 3.0
                    {
                        self.tethers[index].health -= tether::CORD_BULLET_DAMAGE;
                        bullet.remaining = 0.0;
                        impacts.push(bullet.position);
                        break;
                    }
                }
            }
            if let Some((index, fraction)) = hit {
                let body = &mut self.bodies[index];
                // Enemy fire is stopped by a fortress wall but never wears it down.
                let dealt = if !bullet.friendly && body.rock == RockKind::Wall {
                    0.0
                } else {
                    damage(body, bullet.damage, self.player_invulnerability)
                };
                if bullet.friendly && matches!(body.kind, BodyKind::Creature | BodyKind::Base) {
                    self.run.damage_dealt += dealt;
                }
                if !is_fixed(body) {
                    body.velocity +=
                        bullet.velocity.normalize_or_zero() * (180.0 / body.mass) * mass_sign(body);
                }
                let at = previous.lerp(bullet.position, fraction);
                if bullet.blast > 0 {
                    blasts.push((
                        at,
                        blast_radius(bullet.blast),
                        bullet.damage * 0.5,
                        body.id,
                        true,
                    ));
                }
                if bullet.burst > 0.0 {
                    blasts.push((
                        at,
                        bullet.burst,
                        bullet.damage * 0.6,
                        body.id,
                        bullet.friendly,
                    ));
                    bullet.burst = 0.0;
                }
                if bullet.pierce > 0 && !matches!(body.rock, RockKind::Planetoid | RockKind::Wall) {
                    // Passes through (but never through a planetoid or a wall: they swallow every shot), remembering what it struck.
                    if let Some(slot) = bullet.struck.iter_mut().find(|id| **id == 0) {
                        *slot = body.id;
                    }
                    bullet.pierce -= 1;
                } else {
                    bullet.remaining = 0.0;
                }
                impacts.push(at);
            }
        }
        // Fragile shots (missiles) are destroyed by any friendly shot that reaches them.
        let fragile: Vec<usize> = self
            .bullets
            .iter()
            .enumerate()
            .filter(|(_, b)| !b.friendly && b.fragile && b.remaining > 0.0)
            .map(|(i, _)| i)
            .collect();
        if !fragile.is_empty() {
            for shooter in 0..self.bullets.len() {
                if !self.bullets[shooter].friendly || self.bullets[shooter].remaining <= 0.0 {
                    continue;
                }
                for &target in &fragile {
                    let (a, b) = (&self.bullets[shooter], &self.bullets[target]);
                    if b.remaining > 0.0
                        && segment_circle(
                            (a.position - a.velocity * dt) - (b.position - b.velocity * dt),
                            a.position - b.position,
                            Vec2::ZERO,
                            a.radius + b.radius + 5.0,
                        )
                        .is_some()
                    {
                        let at = b.position;
                        let (damage, burst) = (b.damage, b.burst);
                        self.bullets[target].remaining = 0.0;
                        self.bullets[target].burst = 0.0;
                        if burst > 0.0 {
                            blasts.push((at, burst, damage * 0.6, 0, false));
                        }
                        self.bullets[shooter].remaining = 0.0;
                        impacts.push(at);
                        break;
                    }
                }
            }
        }
        // Shots that run out of flight burst where they end.
        for bullet in &self.bullets {
            if bullet.remaining <= 0.0 && bullet.burst > 0.0 {
                blasts.push((
                    bullet.position,
                    bullet.burst,
                    bullet.damage * 0.6,
                    0,
                    bullet.friendly,
                ));
            }
        }
        self.bullets.retain(|b| b.remaining > 0.0);
        let invulnerability = self.player_invulnerability;
        for (at, radius, amount, direct, friendly) in blasts {
            for body in self
                .bodies
                .iter_mut()
                .filter(|b| b.active && b.id != direct)
            {
                if body.position.distance(at) >= radius + body.radius {
                    continue;
                }
                if friendly && body.kind != BodyKind::Player {
                    let dealt = damage(body, amount, 0.0);
                    if matches!(body.kind, BodyKind::Creature | BodyKind::Base) {
                        self.run.damage_dealt += dealt;
                    }
                } else if !friendly && body.kind == BodyKind::Player {
                    damage(body, amount, invulnerability);
                }
            }
            self.effect(at, radius * 0.6, 0.3, EffectKind::Explosion);
        }
        for position in impacts {
            self.effect(position, 16.0, 0.22, EffectKind::Impact);
        }
    }

    fn remove_destroyed(&mut self) {
        let destroyed: Vec<Body> = self
            .bodies
            .iter()
            .filter(|b| b.health <= 0.0)
            .cloned()
            .collect();
        if destroyed.is_empty() {
            return;
        }
        self.bodies.retain(|body| body.health > 0.0);
        let mut lost_player = None;
        for body in &destroyed {
            let (kind, position, radius) = (body.kind, body.position, body.radius);
            if body.consumed {
                // Eaten or starved: gone without a bang, a score or a drop.
                self.effect(position, radius, 0.25, EffectKind::Impact);
                self.record_fallen(body);
                self.note_creature_lost(body, true);
                continue;
            }
            self.effect(position, radius * 2.5, 0.65, EffectKind::Explosion);
            let turret = body
                .base
                .as_ref()
                .is_some_and(|b| b.kind == BaseKind::Turret);
            self.score = self.score.saturating_add(match kind {
                BodyKind::Creature => (body.genome.bounty * body.genes.threat) as u64,
                BodyKind::Asteroid if body.rock == RockKind::Wall => 0,
                BodyKind::Asteroid => 25,
                BodyKind::Base if turret => (60.0 * body.genes.threat) as u64,
                BodyKind::Base => 500,
                _ => 0,
            });
            match kind {
                BodyKind::Player => lost_player = Some(position),
                BodyKind::Asteroid if body.rock == RockKind::Crystal => {
                    // Crystal does not break, it bursts: everything near is hurt, ship included.
                    self.explode(position, CRYSTAL_BLAST, CRYSTAL_DAMAGE, false);
                    self.explode(position, CRYSTAL_BLAST, CRYSTAL_DAMAGE, true);
                }
                // A wall segment just comes down: no shards, no loot.
                BodyKind::Asteroid if body.rock == RockKind::Wall => {}
                BodyKind::Asteroid => {
                    self.release_from(body);
                    self.shatter(body);
                }
                BodyKind::Base if turret => {}
                BodyKind::Base => self.base_destroyed(position),
                _ => {}
            }
            if kind != BodyKind::Player {
                self.civ_destroyed(body);
                self.drop_loot(body);
                self.siphon(body);
            }
            self.record_fallen(body);
            if kind == BodyKind::Base && !turret {
                self.run.bases += 1;
            }
            self.note_creature_lost(body, false);
        }
        if let Some(position) = lost_player {
            self.run.deaths += 1;
            self.run.recap = run::RECAP_SECONDS;
            self.lives = self.lives.saturating_sub(1);
            self.bullets.retain(|bullet| bullet.friendly);
            self.tethers.retain(|t| t.kind != TetherKind::Latch);
            let insured = self.insurance_pays();
            self.shed_on_death(position, insured);
            if self.lives == 0 {
                self.game_over = true;
            } else if !self.respawn_at_pad(position) {
                self.spawn_player(position);
            }
        }
    }

    /// Respawns near `origin`, preferring the least crowded of a few nearby candidates;
    /// shield time covers the escape.
    fn spawn_player(&mut self, origin: Vec2) {
        let offsets = [
            Vec2::ZERO,
            Vec2::new(-350.0, -220.0),
            Vec2::new(350.0, 220.0),
            Vec2::new(-350.0, 220.0),
            Vec2::new(350.0, -220.0),
        ];
        let clearance = |p: Vec2| {
            self.bodies
                .iter()
                .map(|body| p.distance(body.position) - body.radius)
                .fold(f32::INFINITY, f32::min)
        };
        let position = offsets
            .into_iter()
            .map(|offset| origin + offset)
            .max_by(|a, b| clearance(*a).total_cmp(&clearance(*b)))
            .unwrap_or(origin);
        self.spawn_player_exact(position);
    }

    /// Puts a fresh ship exactly at `position`, refitted and under shield time.
    fn spawn_player_exact(&mut self, position: Vec2) {
        let mut player = self.make_body(BodyKind::Player, position);
        player.angle = FRAC_PI_2;
        self.bodies.push(player);
        self.refresh_stats();
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
            ship.health = ship.max_health;
            ship.shield = ship.max_shield;
        }
        self.focus = position;
        self.player_invulnerability = 2.5;
        self.effect(position, 50.0, 1.0, EffectKind::Respawn);
    }

    fn make_body(&mut self, kind: BodyKind, position: Vec2) -> Body {
        let (radius, health, shield, mass) = match kind {
            BodyKind::Player => (14.0, 100.0, 60.0, 10.0),
            // Creatures are reshaped from their genome by `make_creature`.
            BodyKind::Creature => (14.0, 30.0, 0.0, 8.0),
            BodyKind::Asteroid => (35.0, 80.0, 0.0, 25.0),
            BodyKind::BlackHole => (20.0, f32::INFINITY, 0.0, f32::INFINITY),
            BodyKind::Base => (55.0, 450.0, 0.0, f32::INFINITY),
        };
        let id = self.next_id;
        self.next_id += 1;
        Body {
            id,
            kind,
            position,
            velocity: Vec2::ZERO,
            angle: 0.0,
            radius,
            health,
            max_health: health,
            shield,
            max_shield: shield,
            mass,
            origin: None,
            pinned: false,
            chain: None,
            follower: false,
            part: 0,
            base: None,
            home: None,
            panic: 0.0,
            panic_from: Vec2::ZERO,
            alert: false,
            enraged: false,
            genes: Phenotype::default(),
            genome: Genome::default(),
            species: 0,
            parent: None,
            active: true,
            rig: Rig::default(),
            rock: RockKind::Plain,
            lode: Lode::default(),
            den: None,
            energy: 0.0,
            max_energy: 0.0,
            provisioned: false,
            consumed: false,
            adult: None,
            age: 0.0,
            growth: 0.0,
            generation: 0,
            brain: None,
            root: None,
            fort: None,
            unrooted: 0.0,
            breed_clock: 0.0,
            starving: 0.0,
            bite_clock: 0.0,
            wander: 0.0,
            spin: 0.0,
            fire_cooldown: 0.0,
            contact_cooldown: 0.0,
            since_hit: 0.0,
            provoked: 0.0,
            brood_timer: 0.0,
        }
    }

    fn effect(&mut self, position: Vec2, radius: f32, lifetime: f32, kind: EffectKind) {
        let cue = match kind {
            EffectKind::Impact => Some(Cue::Impact { at: position }),
            EffectKind::Explosion => Some(Cue::Explosion {
                at: position,
                radius,
            }),
            EffectKind::Respawn => Some(Cue::Respawn { at: position }),
            // Births and coming of age are seen, not heard.
            EffectKind::Birth | EffectKind::Mature | EffectKind::Pair => None,
        };
        if let Some(cue) = cue {
            self.cue(cue);
        }
        if self.effects.len() < MAX_EFFECTS {
            self.effects.push(Effect {
                position,
                radius,
                remaining: lifetime,
                lifetime,
                kind,
            });
        }
    }
}

/// True when a disc of `extent` around `center` reaches into the view: a camera at `camera`
/// seeing `half` units each way, plus `margin`. Visibility depends on the body's whole extent,
/// never just its center, so a huge body whose edge is on screen is always drawn.
pub fn extent_in_view(center: Vec2, extent: f32, camera: Vec2, half: Vec2, margin: f32) -> bool {
    let reach = half + Vec2::splat(margin + extent.max(0.0));
    (center - camera).abs().cmplt(reach).all()
}

/// Bodies that never move: gravity wells, bases and the stones of a nest.
fn is_fixed(body: &Body) -> bool {
    body.pinned || body.root.is_some() || matches!(body.kind, BodyKind::BlackHole | BodyKind::Base)
}

/// True when one of the two clings to the other, or both cling to the same host: they never
/// collide, so a rooter neither shoves its rock nor its neighbors.
fn clings(a: &Body, b: &Body) -> bool {
    match (a.root, b.root) {
        (Some(x), Some(y)) => x.host == y.host || x.host == b.id || y.host == a.id,
        (Some(x), None) => x.host == b.id,
        (None, Some(y)) => y.host == a.id,
        (None, None) => false,
    }
}

fn inverse_mass(body: &Body) -> f32 {
    if is_fixed(body) { 0.0 } else { 1.0 / body.mass }
}

/// Mutable access to two distinct elements at once.
fn pair_mut<T>(items: &mut [T], a: usize, b: usize) -> (&mut T, &mut T) {
    assert_ne!(a, b);
    if a < b {
        let (left, right) = items.split_at_mut(b);
        (&mut left[a], &mut right[0])
    } else {
        let (left, right) = items.split_at_mut(a);
        (&mut right[0], &mut left[b])
    }
}

/// Recently damaged, with shield or hull still not fully recovered.
fn is_hurt(body: &Body) -> bool {
    body.since_hit < 4.0 && (body.health < body.max_health || body.shield < body.max_shield)
}

fn contact_damage(body: &Body) -> f32 {
    match body.kind {
        BodyKind::Creature => {
            // A rooted creature defends its place: its sting is sharper.
            let sting = if body.root.is_some() {
                1.0 + body.genome.root_defense * crate::genome::ROOT_STING
            } else {
                1.0
            };
            body.genome.contact_damage * body.genes.sharpness() * sting
        }
        BodyKind::BlackHole => 22.0,
        BodyKind::Base => 15.0,
        // A planetoid is a gentle wall: ships and creatures simply bounce off it.
        BodyKind::Asteroid if body.rock == RockKind::Planetoid => 0.0,
        BodyKind::Asteroid if body.rock == RockKind::Wall => 6.0,
        BodyKind::Asteroid => 12.0,
        BodyKind::Player => 0.0,
    }
}

/// How hard a body throws what touches it: the fling gene plus a push for negative mass.
fn fling_strength(body: &Body) -> f32 {
    match body.kind {
        BodyKind::Creature => body.genome.fling_strength(),
        BodyKind::Player => f32::from(body.rig.aura) * AURA_FLING,
        _ => 0.0,
    }
}

/// -1 for negative-mass creatures, which gravity repels and shots pull.
fn mass_sign(body: &Body) -> f32 {
    if body.kind == BodyKind::Creature && body.genome.mass < 0.0 {
        -1.0
    } else {
        1.0
    }
}

fn damage(body: &mut Body, amount: f32, player_invulnerability: f32) -> f32 {
    if body.kind == BodyKind::BlackHole
        || body.rock == RockKind::Planetoid
        || (body.kind == BodyKind::Player && player_invulnerability > 0.0)
    {
        return 0.0;
    }
    // Armor softens what the ship takes; deep-space fauna is simply harder to kill.
    let amount = match body.kind {
        BodyKind::Player => amount * body.rig.guard,
        BodyKind::Creature => amount / body.genes.threat.max(1.0),
        // Stations are meant to be taken down, so depth toughens them more gently.
        BodyKind::Base => amount / body.genes.threat.max(1.0).sqrt(),
        _ => amount,
    };
    let absorbed = body.shield.min(amount);
    body.shield -= absorbed;
    body.health -= amount - absorbed;
    body.since_hit = 0.0;
    amount
}

/// Contact between the ship and `other`: the ship is hurt unless its lunatic field is on,
/// and rams whatever it hits when fitted for it.
fn ram_contact(ship: &mut Body, other: &mut Body, closing_speed: f32, invulnerability: f32) -> f32 {
    let harm = if ship.rig.aura > 0 {
        0.0
    } else {
        contact_damage(other)
    };
    damage(ship, harm, invulnerability);
    ship.contact_cooldown = 0.65;
    if ship.rig.ram > 0 && other.kind != BodyKind::Player {
        let force = (0.6 + closing_speed.abs() / 400.0).min(1.6);
        let dealt = damage(other, RAM_DAMAGE * f32::from(ship.rig.ram) * force, 0.0);
        if matches!(other.kind, BodyKind::Creature | BodyKind::Base) {
            return dealt;
        }
    }
    0.0
}

/// The shots one trigger pull sends out: (angle from the nose, share of full damage).
fn volley(stats: &Stats, rng: &mut Rng) -> Vec<(f32, f32, Shape)> {
    let mut shots = if stats.needles > 0 {
        // Many thin particles in a narrow cone: small each, heavy in mass.
        (0..5 + 4 * usize::from(stats.needles))
            .map(|_| (rng.range(-0.07, 0.07), NEEDLE_SHARE, Shape::Needle))
            .collect()
    } else {
        vec![(0.0, 1.0, Shape::Pellet)]
    };
    for n in 1..=stats.spread {
        let fan = f32::from(n) * SPREAD_ANGLE;
        shots.push((fan, SPREAD_SHARE, Shape::Pellet));
        shots.push((-fan, SPREAD_SHARE, Shape::Pellet));
    }
    for n in 0..stats.broadside {
        let angle = FRAC_PI_2 + f32::from(n) * FRAC_PI_4;
        shots.push((angle, SIDE_SHARE, Shape::Pellet));
        shots.push((-angle, SIDE_SHARE, Shape::Pellet));
    }
    if stats.tailgun > 0 {
        shots.push((PI, SIDE_SHARE, Shape::Pellet));
    }
    shots
}

/// Radius of a friendly shot's burst.
fn blast_radius(level: u8) -> f32 {
    40.0 + 25.0 * f32::from(level)
}

/// Bends a seeker toward the nearest target in front of it, at a rate set by its level.
fn steer_seeker(bullet: &mut Bullet, targets: &[Vec2], dt: f32) {
    let heading = bullet.velocity.normalize_or_zero();
    let best = targets
        .iter()
        .map(|&t| t - bullet.position)
        .filter(|d| d.length() < SEEK_RANGE && heading.dot(d.normalize_or_zero()) > 0.5)
        .min_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
    if let Some(toward) = best {
        let turn = heading.angle_to(toward);
        let limit = f32::from(bullet.homing) * SEEK_TURN * dt;
        bullet.velocity = Vec2::from_angle(turn.clamp(-limit, limit)).rotate(bullet.velocity);
    }
}

/// First intersection along a swept projectile, avoiding tunneling at high speeds.
fn segment_circle(start: Vec2, end: Vec2, center: Vec2, radius: f32) -> Option<f32> {
    let relative = start - center;
    let c = relative.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let movement = end - start;
    let a = movement.length_squared();
    if a < 0.000001 {
        return None;
    }
    let b = relative.dot(movement);
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let fraction = (-b - discriminant.sqrt()) / a;
    (0.0..=1.0).contains(&fraction).then_some(fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) const DT: f32 = 1.0 / 60.0;

    /// Only the player remains, so tests control exactly what exists.
    pub(super) fn empty_game() -> Game {
        let mut game = Game::new(42);
        game.bodies.retain(|b| b.kind == BodyKind::Player);
        game.bullets.clear();
        game.effects.clear();
        game.tethers.clear();
        game.chains.clear();
        game.food.clear();
        game.eggs.clear();
        game.bodies[0].position = Vec2::ZERO;
        game.player_invulnerability = 0.0;
        game
    }

    pub(super) fn add(game: &mut Game, kind: BodyKind, position: Vec2) -> u64 {
        let body = game.make_body(kind, position);
        let id = body.id;
        game.bodies.push(body);
        id
    }

    /// Adds a creature of the given species, with its whole body if it has one.
    pub(super) fn spawn(game: &mut Game, species: &Species, position: Vec2) -> u64 {
        let body = game.make_creature(species, position);
        game.add_body(body)
    }

    pub(super) fn body(game: &Game, id: u64) -> &Body {
        game.bodies.iter().find(|b| b.id == id).unwrap()
    }

    pub(super) fn set_player(game: &mut Game, position: Vec2, velocity: Vec2) {
        let player = game
            .bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap();
        player.position = position;
        player.velocity = velocity;
    }

    /// A pinned planetoid of the given radius at `at`, as the generator makes them.
    fn planetoid_at(game: &mut Game, at: Vec2, radius: f32) -> u64 {
        let id = add(game, BodyKind::Asteroid, at);
        let w = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        w.rock = RockKind::Planetoid;
        w.radius = radius;
        w.pinned = true;
        w.mass = 900.0;
        id
    }

    #[test]
    fn extent_in_view_uses_the_whole_body_not_its_center() {
        let half = Vec2::new(900.0, 450.0);
        let camera = Vec2::ZERO;
        // A center far off screen: culled for a speck, drawn for a world whose edge shows.
        let center = Vec2::new(1300.0, 0.0);
        assert!(!extent_in_view(center, 10.0, camera, half, 120.0));
        assert!(extent_in_view(center, 700.0, camera, half, 120.0));
        // Edge exactly on screen (center distance minus radius inside the view).
        for dx in [901.0, 1200.0, 1590.0] {
            assert!(extent_in_view(
                Vec2::new(dx, 300.0),
                700.0,
                camera,
                half,
                0.0
            ));
        }
        assert!(!extent_in_view(
            Vec2::new(1700.0, 0.0),
            700.0,
            camera,
            half,
            0.0
        ));
        // Per axis: a tall world above the view.
        assert!(extent_in_view(
            Vec2::new(0.0, 1100.0),
            700.0,
            camera,
            half,
            0.0
        ));
        assert!(!extent_in_view(
            Vec2::new(0.0, 1200.0),
            700.0,
            camera,
            half,
            0.0
        ));
    }

    /// Every kind of shot, at several speeds and angles, from just outside and far away: the
    /// planetoid's surface swallows it, and nothing ever ends up inside.
    #[test]
    fn planetoids_absorb_every_projectile() {
        let radius = 300.0;
        let at = Vec2::new(0.0, 0.0);
        let mut failures = Vec::new();
        for kind in 0..8 {
            for speed in [150.0, 520.0, 1400.0, 4000.0] {
                for (start, aim) in [(330.0, 0.0), (900.0, 0.4), (1800.0, 2.0), (1200.0, -2.6)] {
                    let mut game = empty_game();
                    game.stream_sectors();
                    set_player(&mut game, Vec2::new(-4000.0, 4000.0), Vec2::ZERO);
                    game.bodies[0].health = 1e9;
                    game.player_invulnerability = 1e9;
                    planetoid_at(&mut game, at, radius);
                    let from = Vec2::from_angle(aim) * start;
                    // Aim at a point on the disc, not always the center.
                    let target = Vec2::from_angle(aim + 1.0) * radius * 0.6;
                    let direction = (target - from).normalize();
                    let mut bullet = if kind < 5 {
                        Bullet::friendly(from, direction * speed, 60.0)
                    } else {
                        Bullet::hostile(from, direction * speed, 60.0, 5.0)
                    };
                    match kind {
                        1 => bullet.pierce = 3,
                        2 => bullet.blast = 2,
                        3 => bullet.homing = 3,
                        4 => bullet.burst = 80.0,
                        6 => {
                            bullet.radius = 1.8;
                            bullet.shape = Shape::Needle;
                        }
                        7 => {
                            bullet.radius = 6.0;
                            bullet.burst = 75.0;
                            bullet.fragile = true;
                        }
                        _ => {}
                    }
                    game.bullets.push(bullet);
                    let mut inside = false;
                    for _ in 0..3600 {
                        game.step(DT, Input::default());
                        inside |= game
                            .bullets
                            .iter()
                            .any(|b| b.position.distance(at) < radius);
                        if game.bullets.is_empty() {
                            break;
                        }
                    }
                    if inside || !game.bullets.is_empty() {
                        failures.push((kind, speed, start, aim, inside));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "leaked through: {failures:?}");
    }

    #[test]
    fn planetoids_stop_mines_and_ships_and_creatures_at_any_speed() {
        let radius = 250.0;
        let mut game = empty_game();
        game.stream_sectors();
        let id = planetoid_at(&mut game, Vec2::ZERO, radius);
        let at = body(&game, id).position;
        // A mine that starts inside, and one that drifts in fast.
        game.mines.push(Mine {
            position: Vec2::new(100.0, 0.0),
            velocity: Vec2::ZERO,
            friendly: false,
            age: 0.0,
            fuse: None,
            damage: 0.0,
            blast: 0.0,
        });
        game.mines.push(Mine {
            position: Vec2::new(-radius - 20.0, 0.0),
            velocity: Vec2::new(900.0, 0.0),
            friendly: false,
            age: 0.0,
            fuse: None,
            damage: 0.0,
            blast: 0.0,
        });
        let creature = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, -radius - 60.0));
        game.player_invulnerability = 1e9;
        for speed in [650.0, 1000.0, 2500.0] {
            set_player(
                &mut game,
                Vec2::new(radius + 80.0, 0.0),
                Vec2::new(-speed, 0.0),
            );
            {
                let c = game.bodies.iter_mut().find(|b| b.id == creature).unwrap();
                c.position = Vec2::new(0.0, -radius - 60.0);
                c.velocity = Vec2::new(0.0, speed);
            }
            for _ in 0..90 {
                game.step(DT, Input::default());
                let ship = game.player().unwrap();
                assert!(
                    ship.position.distance(at) >= radius,
                    "ship inside at {speed}"
                );
                let c = body(&game, creature);
                assert!(
                    c.position.distance(at) >= radius,
                    "creature inside at {speed}"
                );
            }
        }
        assert!(
            game.mines.iter().all(|m| m.position.distance(at) >= radius),
            "mines stay out: {:?}",
            game.mines.iter().map(|m| m.position).collect::<Vec<_>>()
        );
    }

    #[test]
    fn thrust_and_brake() {
        let mut game = empty_game();
        for _ in 0..60 {
            game.step(
                DT,
                Input {
                    thrust: 1.0,
                    ..Default::default()
                },
            );
        }
        let speed = game.player().unwrap().velocity.length();
        assert!(speed > 300.0 && speed <= PLAYER_SPEED);
        for _ in 0..30 {
            game.step(
                DT,
                Input {
                    brake: true,
                    ..Default::default()
                },
            );
        }
        assert!(game.player().unwrap().velocity.length() < speed * 0.15);
    }

    #[test]
    fn flying_across_sector_edges_loads_neighbors_and_unloads_the_far_ones() {
        let mut game = Game::new(5);
        assert_eq!(game.sector(), SectorId::ORIGIN);
        game.player_invulnerability = 1e9;
        set_player(
            &mut game,
            Vec2::new(world::SECTOR_SIZE / 2.0 - 100.0, 0.0),
            Vec2::new(PLAYER_SPEED, 0.0),
        );
        game.step(DT, Input::default());
        assert!(game.loaded.contains(&SectorId { x: 1, y: 0 }));
        for _ in 0..60 * 30 {
            game.step(DT, Input::default());
            game.player_invulnerability = 1e9;
            let position = game.player().unwrap().position;
            set_player(&mut game, position, Vec2::new(PLAYER_SPEED, 0.0));
        }
        assert!(game.sector().x >= 3);
        // Sector 0 is more than UNLOAD_DISTANCE behind the player and has been dropped.
        assert!(!game.loaded.contains(&SectorId::ORIGIN));
        let home = game.sector();
        assert!(game.bodies.iter().all(|b| {
            SectorId::containing(b.position).chebyshev_distance(home) <= UNLOAD_DISTANCE
        }));
    }

    #[test]
    fn revisiting_a_sector_regenerates_the_same_population() {
        let mut game = Game::new(8);
        let count = |game: &Game| {
            game.bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Asteroid)
                .count()
        };
        let at_start = world::generate(8, SectorId::ORIGIN)
            .iter()
            .filter(|s| s.kind == BodyKind::Asteroid)
            .count();
        assert!(count(&game) >= at_start);
        set_player(
            &mut game,
            Vec2::new(5.0 * world::SECTOR_SIZE, 0.0),
            Vec2::ZERO,
        );
        game.step(DT, Input::default());
        assert!(!game.loaded.contains(&SectorId::ORIGIN));
        set_player(&mut game, Vec2::ZERO, Vec2::ZERO);
        game.step(DT, Input::default());
        assert!(game.loaded.contains(&SectorId::ORIGIN));
        assert!(count(&game) >= at_start);
    }

    #[test]
    fn bodies_outside_the_active_region_are_frozen() {
        let mut game = empty_game();
        let far = spawn(
            &mut game,
            &Species::lunatic(),
            Vec2::new(2.0 * world::SECTOR_SIZE, 0.0),
        );
        game.bodies
            .iter_mut()
            .find(|b| b.id == far)
            .unwrap()
            .velocity = Vec2::X * 100.0;
        let before = body(&game, far).position;
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert_eq!(body(&game, far).position, before);
        assert!(!body(&game, far).active);
    }

    #[test]
    fn wanderers_turn_back_at_the_active_edge_instead_of_freezing() {
        let mut game = empty_game();
        let edge = world::SECTOR_SIZE * 0.5;
        let drifter = spawn(&mut game, &Species::bogey(), Vec2::new(edge - 500.0, 0.0));
        {
            let b = game.bodies.iter_mut().find(|b| b.id == drifter).unwrap();
            b.velocity = Vec2::X * 200.0;
            b.wander = 0.0;
        }
        for _ in 0..600 {
            game.step(DT, Input::default());
        }
        let b = body(&game, drifter);
        assert!(b.active && b.position.x < edge);
        assert!(b.velocity.x < 0.0 || b.position.x < edge - 100.0);
    }

    #[test]
    fn a_chaser_follows_the_player_across_a_sector_border() {
        let mut game = empty_game();
        let edge = world::SECTOR_SIZE / 2.0;
        let chaser = spawn(&mut game, &Species::lunatic(), Vec2::new(edge - 400.0, 0.0));
        let anchor = Vec2::new(edge + 300.0, 0.0);
        game.player_invulnerability = 1e9;
        for _ in 0..600 {
            // Hold the ship still so the outcome does not depend on where it is flung.
            set_player(&mut game, anchor, Vec2::ZERO);
            game.step(DT, Input::default());
        }
        let chaser = body(&game, chaser);
        assert!(chaser.alert && chaser.active);
        assert!(chaser.position.x > edge, "chaser stayed in its home sector");
    }

    fn bogeys(game: &mut Game, center: Vec2, count: usize) -> Vec<u64> {
        // These tests are about flocking alone: hungry bogeys would otherwise call up
        // plankton and wander toward it, so growth is switched off.
        game.food_clock = f32::MAX;
        (0..count)
            .map(|i| {
                let angle = i as f32 * 2.4;
                let spot = center + Vec2::from_angle(angle) * (30.0 + 12.0 * i as f32);
                spawn(game, &Species::bogey(), spot)
            })
            .collect()
    }

    fn positions(game: &Game, ids: &[u64]) -> Vec<Vec2> {
        ids.iter().map(|&id| body(game, id).position).collect()
    }

    pub(super) fn mean(points: &[Vec2]) -> Vec2 {
        points.iter().sum::<Vec2>() / points.len() as f32
    }

    #[test]
    fn a_crowd_travels_loosely_instead_of_as_a_tight_unit() {
        let mut game = empty_game();
        let ids = bogeys(&mut game, Vec2::new(0.0, 2000.0), 8);
        let start = mean(&positions(&game, &ids));
        for _ in 0..1800 {
            game.step(DT, Input::default());
        }
        let spots = positions(&game, &ids);
        assert!(
            mean(&spots).distance(start) > 300.0,
            "crowd never travelled"
        );
        let nearest = |i: usize| {
            (0..spots.len())
                .filter(|&j| j != i)
                .map(|j| spots[i].distance(spots[j]))
                .fold(f32::INFINITY, f32::min)
        };
        let spacing = (0..spots.len()).map(nearest).sum::<f32>() / spots.len() as f32;
        assert!(spacing > 50.0, "crowd is packed too tightly: {spacing}");
        assert!(
            spots.iter().all(|p| p.distance(mean(&spots)) < 1200.0),
            "crowd scattered"
        );
    }

    #[test]
    fn nearby_bands_merge_while_distant_ones_stay_independent() {
        let mut game = empty_game();
        let left = bogeys(&mut game, Vec2::new(-200.0, 2200.0), 5);
        let right = bogeys(&mut game, Vec2::new(200.0, 2200.0), 5);
        let remote = bogeys(&mut game, Vec2::new(0.0, -2900.0), 5);
        for _ in 0..900 {
            game.step(DT, Input::default());
        }
        let heading = |game: &Game, ids: &[u64]| {
            ids.iter()
                .map(|&id| body(game, id).velocity.normalize_or_zero())
                .sum::<Vec2>()
                / ids.len() as f32
        };
        let near: Vec<u64> = left.iter().chain(&right).copied().collect();
        // Neighboring bands behave as one: strongly aligned and still mixed together.
        assert!(
            heading(&game, &near).length() > 0.8,
            "nearby bands did not align"
        );
        let a = mean(&positions(&game, &left));
        let b = mean(&positions(&game, &right));
        assert!(a.distance(b) < 600.0, "nearby bands drifted apart");
        // Nothing ties the remote band to them, and the sim stayed finite.
        assert!(mean(&positions(&game, &remote)).distance(mean(&positions(&game, &near))) > 2000.0);
    }

    #[test]
    fn touching_a_lunatic_flings_the_victim_chaotically_and_it_recoils() {
        let mut flings = Vec::new();
        for seed in 0..12 {
            let mut game = empty_game();
            game.rng = Rng::new(seed);
            game.player_invulnerability = 1e9;
            let lunatic = spawn(&mut game, &Species::lunatic(), Vec2::new(30.0, 0.0));
            game.step(DT, Input::default());
            let player = game.player().unwrap();
            assert!(
                player.velocity.length() >= FLING_SPEED * 0.95,
                "not flung hard"
            );
            assert!(
                body(&game, lunatic).velocity.dot(player.velocity) < 0.0,
                "no recoil"
            );
            flings.push(player.velocity.normalize());
        }
        // Directions vary around the contact normal rather than being identical.
        let spread = flings
            .iter()
            .map(|d| d.dot(Vec2::NEG_X))
            .fold((1.0_f32, -1.0_f32), |(lo, hi), x| (lo.min(x), hi.max(x)));
        assert!(spread.1 - spread.0 > 0.1);
        assert!(flings.iter().all(|d| d.dot(Vec2::NEG_X) > 0.3));
    }

    #[test]
    fn a_fling_is_not_snapped_back_to_top_speed_and_does_not_repeat_every_tick() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        spawn(&mut game, &Species::lunatic(), Vec2::new(30.0, 0.0));
        game.step(DT, Input::default());
        let flung = game.player().unwrap().velocity.length();
        assert!(flung > PLAYER_SPEED);
        game.step(DT, Input::default());
        let later = game.player().unwrap().velocity.length();
        assert!(later < flung && later > PLAYER_SPEED);
        for _ in 0..180 {
            game.step(DT, Input::default());
        }
        assert!(game.player().unwrap().velocity.length() <= PLAYER_SPEED * 1.01);
    }

    #[test]
    fn phenotype_weights_change_behavior_and_neutral_genes_do_not() {
        // A keener sensor agitates a bogey at a distance the default would ignore.
        let agitated_at = |sensor: f32| {
            let mut game = empty_game();
            let id = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 450.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .genes
                .sensor_acuity = sensor;
            game.player_invulnerability = 1e9;
            game.step(DT, Input::default());
            body(&game, id).alert
        };
        assert!(!agitated_at(1.0));
        assert!(agitated_at(1.6));
        // Mass affinity draws a creature toward a rock, or pushes it away.
        let drift = |affinity: f32| {
            let mut game = empty_game();
            let id = spawn(&mut game, &Species::lunatic(), Vec2::new(0.0, 2000.0));
            add(&mut game, BodyKind::Asteroid, Vec2::new(400.0, 2000.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == id)
                .unwrap()
                .genes
                .mass_affinity = affinity;
            for b in &mut game.bodies {
                b.velocity = Vec2::ZERO;
                b.wander = 0.0;
            }
            for _ in 0..60 {
                game.step(DT, Input::default());
            }
            body(&game, id).position.distance(Vec2::new(400.0, 2000.0))
        };
        assert!(drift(1.0) < drift(0.0) && drift(0.0) < drift(-1.0));
    }

    #[test]
    fn calm_bogeys_ignore_a_passing_player_until_it_gets_close_or_hurts_them() {
        let mut game = empty_game();
        let id = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 500.0));
        game.player_invulnerability = 1e9;
        for _ in 0..300 {
            game.step(DT, Input::default());
        }
        assert!(!body(&game, id).alert);
        assert!(game.bullets.iter().all(|b| b.friendly));
        // Getting too near agitates it.
        let near = body(&game, id).position - Vec2::new(0.0, 250.0);
        set_player(&mut game, near, Vec2::ZERO);
        game.step(DT, Input::default());
        assert!(body(&game, id).alert && !body(&game, id).enraged);
        // Moving well clear lets it settle again.
        set_player(&mut game, near - Vec2::new(0.0, 1500.0), Vec2::ZERO);
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert!(!body(&game, id).alert);
        // A shot from afar agitates it, and a second leaves it near death and berserk.
        let target = body(&game, id).position;
        set_player(&mut game, target - Vec2::new(0.0, 700.0), Vec2::ZERO);
        let shot = |at: Vec2| Bullet::friendly(at, Vec2::ZERO, 1.0);
        game.bullets.push(shot(body(&game, id).position));
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(body(&game, id).alert && !body(&game, id).enraged);
        game.bullets.push(shot(body(&game, id).position));
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(body(&game, id).enraged);
    }

    #[test]
    fn hurt_bogeys_panic_their_close_schoolmates() {
        let mut game = empty_game();
        let hurt = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 1500.0));
        let near = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 1650.0));
        let far = spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 1900.0));
        game.player_invulnerability = 1e9;
        game.bodies
            .iter_mut()
            .find(|b| b.id == hurt)
            .unwrap()
            .shield = 0.0;
        game.bodies
            .iter_mut()
            .find(|b| b.id == hurt)
            .unwrap()
            .since_hit = 0.0;
        game.step(DT, Input::default());
        assert!(body(&game, hurt).alert && body(&game, near).alert);
        assert!(!body(&game, far).alert);
    }

    #[test]
    fn alarms_spread_to_neighbors_in_earshot_but_not_beyond() {
        let mut game = empty_game();
        let watcher = spawn(&mut game, &Species::lunatic(), Vec2::new(0.0, 900.0));
        let neighbor = spawn(&mut game, &Species::lunatic(), Vec2::new(0.0, 1250.0));
        let distant = spawn(&mut game, &Species::lunatic(), Vec2::new(0.0, 1900.0));
        game.player_invulnerability = 1e9;
        game.step(DT, Input::default());
        assert!(body(&game, watcher).alert);
        assert!(body(&game, neighbor).alert);
        assert!(!body(&game, distant).alert);
    }

    #[test]
    fn shots_hit_along_their_path_and_drain_shields_first() {
        let mut game = empty_game();
        let id = spawn(&mut game, &Species::bogey(), Vec2::new(100.0, 0.0));
        game.bullets.push(Bullet::friendly(
            Vec2::new(50.0, 0.0),
            Vec2::new(4000.0, 0.0),
            1.0,
        ));
        game.step(0.05, Input::default());
        let target = body(&game, id);
        assert_eq!(target.shield, 0.0);
        assert_eq!(target.health, 27.0);
        assert!(game.bullets.iter().all(|b| !b.friendly));
    }

    #[test]
    fn shield_regeneration_requires_a_quiet_interval() {
        let mut game = empty_game();
        damage(&mut game.bodies[0], 30.0, 0.0);
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.player().unwrap().shield, 30.0);
        for _ in 0..90 {
            game.step(DT, Input::default());
        }
        assert!(game.player().unwrap().shield > 30.0);
    }

    #[test]
    fn death_consumes_lives_and_restart_restores_seed() {
        let mut game = empty_game();
        for expected_lives in [2, 1, 0] {
            game.bodies
                .iter_mut()
                .find(|b| b.kind == BodyKind::Player)
                .unwrap()
                .health = 0.0;
            game.step(DT, Input::default());
            assert_eq!(game.lives, expected_lives);
            assert_eq!(game.game_over, expected_lives == 0);
            if expected_lives > 0 {
                assert!(game.player_invulnerability > 2.0);
            }
        }
        assert!(game.player().is_none());
        game.reset();
        let fresh = Game::new(42);
        assert_eq!(game.lives, 3);
        assert_eq!(game.bodies.len(), fresh.bodies.len());
        assert_eq!(game.bodies[1].position, fresh.bodies[1].position);
    }

    #[test]
    fn kills_award_score() {
        let mut game = empty_game();
        spawn(&mut game, &Species::bogey(), Vec2::new(0.0, 2000.0));
        spawn(&mut game, &Species::lunatic(), Vec2::new(0.0, -2000.0));
        for body in &mut game.bodies {
            if body.kind == BodyKind::Creature {
                body.health = 0.0;
            }
        }
        game.step(DT, Input::default());
        assert_eq!(game.score, 225);
    }

    #[test]
    fn black_holes_pull_bodies_and_ignore_damage() {
        let mut game = empty_game();
        let hole = add(&mut game, BodyKind::BlackHole, Vec2::new(200.0, 0.0));
        game.step(DT, Input::default());
        assert!(game.player().unwrap().velocity.x > 0.0);
        let hole = game.bodies.iter_mut().find(|b| b.id == hole).unwrap();
        damage(hole, 1_000_000.0, 0.0);
        assert_eq!(hole.position, Vec2::new(200.0, 0.0));
        assert!(hole.health.is_infinite());
    }

    #[test]
    fn coincident_contacts_separate_without_nan() {
        let mut game = empty_game();
        let asteroid = add(&mut game, BodyKind::Asteroid, Vec2::ZERO);
        game.step(DT, Input::default());
        let player = game.player().unwrap();
        assert!(
            player.position.distance(body(&game, asteroid).position)
                >= player.radius + body(&game, asteroid).radius
        );
        assert_eq!(player.shield, 48.0);
        assert!(
            game.bodies
                .iter()
                .all(|b| b.position.is_finite() && b.velocity.is_finite())
        );
    }

    #[test]
    fn long_flights_with_continuous_fire_stay_bounded_and_deterministic() {
        let run = || {
            let mut game = Game::new(123);
            for tick in 0..12_000 {
                game.player_invulnerability = 10.0;
                game.step(
                    DT,
                    Input {
                        fire: true,
                        turn: if tick % 900 < 450 { 0.4 } else { -0.2 },
                        thrust: 1.0,
                        ..Default::default()
                    },
                );
                assert!(game.bullets.len() <= MAX_BULLETS);
                assert!(game.effects.len() <= MAX_EFFECTS);
                assert!(
                    game.bodies
                        .iter()
                        .all(|b| b.position.is_finite() && b.velocity.is_finite())
                );
            }
            game
        };
        let a = run();
        let b = run();
        assert!(a.loaded.len() <= 9 + 16);
        assert!(a.bodies.len() < 1500);
        assert_eq!(a.score, b.score);
        assert_eq!(a.bodies.len(), b.bodies.len());
        for (left, right) in a.bodies.iter().zip(&b.bodies) {
            assert_eq!(left.position, right.position);
            assert_eq!(left.health, right.health);
        }
    }

    #[test]
    fn move_direction_thrusts_independently_of_heading() {
        let mut game = Game::new(1);
        game.bodies[0].angle = 0.0;
        game.bodies[0].velocity = Vec2::ZERO;
        game.step(
            DT,
            Input {
                move_direction: Some(Vec2::Y),
                aim_direction: Some(Vec2::X),
                ..Default::default()
            },
        );
        let player = game.player().unwrap();
        assert!(player.velocity.y > 0.0);
        assert!(player.velocity.y > player.velocity.x.abs() * 10.0);
    }

    #[test]
    fn invalid_time_and_analog_inputs_cannot_poison_state() {
        let mut game = Game::new(1);
        game.step(f32::NAN, Input::default());
        game.step(-1.0, Input::default());
        assert_eq!(game.time, 0.0);
        game.step(
            DT,
            Input {
                thrust: f32::NAN,
                turn: f32::INFINITY,
                aim_direction: Some(Vec2::splat(f32::NAN)),
                ..Default::default()
            },
        );
        assert!(game.player().unwrap().position.is_finite());
        assert!(game.player().unwrap().angle.is_finite());
    }

    /// First sector (scanning outward) whose generated population satisfies `wanted`.
    pub(super) fn find_sector(seed: u64, wanted: impl Fn(&[world::Spawn]) -> bool) -> SectorId {
        for ring in 0..=20_i32 {
            for x in -ring..=ring {
                for y in -ring..=ring {
                    let id = SectorId { x, y };
                    if x.abs().max(y.abs()) == ring && wanted(&world::generate(seed, id)) {
                        return id;
                    }
                }
            }
        }
        panic!("no sector matched");
    }

    #[test]
    fn destroyed_spawns_stay_destroyed_across_unload_and_reload() {
        let mut game = Game::new(7);
        let victim = game
            .bodies
            .iter()
            .find(|b| b.kind == BodyKind::Creature && b.origin.is_some())
            .unwrap();
        let (victim_id, origin) = (victim.id, victim.origin.unwrap());
        let before = game.bodies.iter().filter(|b| b.origin.is_some()).count();
        game.bodies
            .iter_mut()
            .find(|b| b.id == victim_id)
            .unwrap()
            .health = 0.0;
        game.player_invulnerability = 1e9;
        game.step(DT, Input::default());
        // Leave far enough for the sector to unload, then come back.
        game.teleport(Vec2::new(6.0 * world::SECTOR_SIZE, 0.0));
        game.step(DT, Input::default());
        assert!(
            game.bodies
                .iter()
                .all(|b| b.origin.is_none_or(|(q, _)| q != SectorId::ORIGIN))
        );
        game.teleport(Vec2::ZERO);
        game.step(DT, Input::default());
        assert!(game.bodies.iter().all(|b| b.origin != Some(origin)));
        let after = game
            .bodies
            .iter()
            .filter(|b| b.origin.is_some_and(|(q, _)| q == SectorId::ORIGIN))
            .count();
        assert!(
            after >= before - 1 - 8,
            "the rest of the sector should return"
        );
        assert!(after > 0);
    }

    #[test]
    fn a_wandering_creature_is_not_duplicated_when_its_home_reloads() {
        let mut game = Game::new(7);
        let count = |game: &Game| game.bodies.len();
        let before = count(&game);
        game.loaded.remove(&SectorId::ORIGIN);
        game.populate(SectorId::ORIGIN);
        assert_eq!(count(&game), before);
    }

    #[test]
    fn destroyed_bases_and_chains_persist() {
        let seed = 11;
        let sector = find_sector(seed, |spawns| {
            spawns.iter().any(|s| s.kind == BodyKind::Base)
                && spawns
                    .iter()
                    .any(|s| s.species.is_some_and(|sp| sp.genome.is_jointed()))
        });
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(sector.center());
        game.step(DT, Input::default());
        let base = game.bodies.iter().find(|b| b.base.is_some()).unwrap();
        let base_origin = base.origin;
        let base_id = base.id;
        game.bodies
            .iter_mut()
            .find(|b| b.id == base_id)
            .unwrap()
            .health = 0.0;
        // Destroy a whole chain, one segment at a time.
        for body in game.bodies.iter_mut().filter(|b| b.chain.is_some()) {
            body.health = 0.0;
        }
        game.step(DT, Input::default());
        assert!(
            game.chains.is_empty()
                || game
                    .bodies
                    .iter()
                    .all(|b| b.chain.is_none() || b.health > 0.0)
        );
        game.teleport(sector.center() + Vec2::new(7.0 * world::SECTOR_SIZE, 0.0));
        game.step(DT, Input::default());
        game.teleport(sector.center());
        game.step(DT, Input::default());
        assert!(game.bodies.iter().all(|b| b.origin != base_origin));
        assert!(game.bodies.iter().any(|b| b.kind == BodyKind::Asteroid));
    }

    #[test]
    fn a_destroyed_rock_shatters_into_smaller_pieces_and_tiny_ones_vanish() {
        let mut game = empty_game();
        let big = add(&mut game, BodyKind::Asteroid, Vec2::new(0.0, 1500.0));
        {
            let rock = game.bodies.iter_mut().find(|b| b.id == big).unwrap();
            rock.radius = 50.0;
            rock.mass = 30.0;
            rock.health = 0.0;
        }
        game.step(DT, Input::default());
        let shards: Vec<&Body> = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Asteroid)
            .collect();
        assert_eq!(shards.len(), 3);
        assert!(
            shards
                .iter()
                .all(|s| (s.radius - 31.0).abs() < 0.01 && !s.pinned && s.origin.is_none())
        );
        assert!(
            shards
                .iter()
                .all(|s| s.health > 0.0 && s.velocity.length() > 60.0)
        );
        // A pebble leaves nothing behind.
        let mut game = empty_game();
        let small = add(&mut game, BodyKind::Asteroid, Vec2::new(0.0, 1500.0));
        {
            let rock = game.bodies.iter_mut().find(|b| b.id == small).unwrap();
            rock.radius = 15.0;
            rock.health = 0.0;
        }
        game.step(DT, Input::default());
        assert!(game.bodies.iter().all(|b| b.kind != BodyKind::Asteroid));
    }

    #[test]
    fn shattering_always_terminates_and_stays_bounded() {
        let mut game = empty_game();
        for i in 0..6 {
            let id = add(
                &mut game,
                BodyKind::Asteroid,
                Vec2::new(i as f32 * 150.0, 1800.0),
            );
            let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            rock.radius = 60.0;
            rock.mass = 36.0;
        }
        game.player_invulnerability = 1e9;
        for _ in 0..600 {
            for body in game
                .bodies
                .iter_mut()
                .filter(|b| b.kind == BodyKind::Asteroid)
            {
                body.health = 0.0;
            }
            game.step(DT, Input::default());
            assert!(game.bodies.len() < MAX_BODIES);
        }
        assert!(game.bodies.iter().all(|b| b.kind != BodyKind::Asteroid));
    }

    #[test]
    fn a_flung_rock_takes_damage_and_fast_rock_impacts_hurt_both() {
        let mut game = empty_game();
        let lunatic = spawn(&mut game, &Species::lunatic(), Vec2::new(0.0, 1500.0));
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(40.0, 1500.0));
        {
            let rock = game.bodies.iter_mut().find(|b| b.id == rock).unwrap();
            rock.radius = 30.0;
            rock.mass = 18.0;
            rock.health = 38.0;
        }
        let _ = lunatic;
        game.step(DT, Input::default());
        // 40 damage from the fling destroys it, and it shatters.
        assert!(game.bodies.iter().all(|b| b.id != rock));
        assert!(
            game.bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Asteroid)
                .count()
                >= 2
        );
        // Two rocks meeting hard damage each other; a gentle touch does not.
        for (speed, hurt) in [(800.0, true), (100.0, false)] {
            let mut game = empty_game();
            let a = add(&mut game, BodyKind::Asteroid, Vec2::new(0.0, 1500.0));
            let b = add(&mut game, BodyKind::Asteroid, Vec2::new(70.0, 1500.0));
            for (id, v) in [(a, speed), (b, 0.0)] {
                let rock = game.bodies.iter_mut().find(|r| r.id == id).unwrap();
                rock.velocity = Vec2::new(v, 0.0);
                rock.health = 200.0;
                rock.max_health = 200.0;
            }
            game.step(DT, Input::default());
            assert_eq!(body(&game, a).health < 200.0, hurt);
            assert_eq!(body(&game, b).health < 200.0, hurt);
        }
    }

    /// A ring of pinned stones matching the generator's nests, with the opening facing -y.
    fn build_nest(game: &mut Game, heart: Vec2) -> Vec<u64> {
        (0..9)
            .filter(|n| *n != 0)
            .map(|n| {
                let angle = -FRAC_PI_2 + n as f32 * TAU / 9.0;
                let id = add(
                    game,
                    BodyKind::Asteroid,
                    heart + Vec2::from_angle(angle) * 130.0,
                );
                let stone = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                stone.radius = 40.0;
                stone.pinned = true;
                stone.health = 1e6;
                id
            })
            .collect()
    }

    #[test]
    fn nest_stones_do_not_move_and_the_ship_enters_only_through_the_opening() {
        let heart = Vec2::new(0.0, 1500.0);
        // Through the opening (facing -y): fly straight up into the hollow.
        let mut game = empty_game();
        let stones = build_nest(&mut game, heart);
        let anchors: Vec<Vec2> = stones.iter().map(|&id| body(&game, id).position).collect();
        game.player_invulnerability = 1e9;
        set_player(&mut game, heart + Vec2::new(0.0, -420.0), Vec2::ZERO);
        for _ in 0..240 {
            game.step(
                DT,
                Input {
                    thrust: 1.0,
                    ..Default::default()
                },
            );
        }
        assert!(
            game.player().unwrap().position.distance(heart) < 100.0,
            "ship never got inside"
        );
        for (&id, &anchor) in stones.iter().zip(&anchors) {
            assert_eq!(body(&game, id).position, anchor);
        }
        // Against the far wall (opposite the opening): it bounces off and stays outside.
        let mut game = empty_game();
        build_nest(&mut game, heart);
        game.player_invulnerability = 1e9;
        set_player(&mut game, heart + Vec2::new(0.0, 420.0), Vec2::ZERO);
        game.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .angle = -FRAC_PI_2;
        for _ in 0..240 {
            game.step(
                DT,
                Input {
                    thrust: 1.0,
                    ..Default::default()
                },
            );
            assert!(game.player().unwrap().position.distance(heart) > 60.0);
        }
    }

    #[test]
    fn shooting_a_nest_stone_opens_the_wall() {
        let heart = Vec2::new(0.0, 1500.0);
        let mut game = empty_game();
        let stones = build_nest(&mut game, heart);
        let target = stones[3];
        {
            let stone = game.bodies.iter_mut().find(|b| b.id == target).unwrap();
            stone.health = 20.0;
        }
        let spot = body(&game, target).position;
        game.bullets.push(Bullet::friendly(
            spot - Vec2::new(0.0, 1.0),
            Vec2::ZERO,
            1.0,
        ));
        game.step(DT, Input::default());
        assert!(game.bodies.iter().all(|b| b.id != target));
        assert!(
            game.bodies
                .iter()
                .any(|b| b.kind == BodyKind::Asteroid && !b.pinned)
        );
    }

    #[test]
    fn explored_space_stays_finite_with_every_new_element_present() {
        let seed = 5;
        let sector = find_sector(seed, |s| {
            s.iter().any(|x| x.pinned)
                && s.iter().any(|x| x.kind == BodyKind::Base)
                && s.iter()
                    .any(|x| x.link.is_some() || x.species.is_some_and(|sp| sp.genome.is_jointed()))
        });
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(sector.center());
        for tick in 0..3600 {
            game.step(
                DT,
                Input {
                    thrust: 0.5,
                    turn: if tick % 600 < 300 { 0.4 } else { -0.4 },
                    fire: true,
                    ..Default::default()
                },
            );
            assert!(game.bodies.len() <= MAX_BODIES);
            assert!(game.tethers.len() <= tether::MAX_TETHERS);
            assert!(game.bullets.len() <= MAX_BULLETS);
            assert!(
                game.bodies
                    .iter()
                    .all(|b| b.position.is_finite() && b.velocity.is_finite())
            );
        }
    }

    #[test]
    fn home_species_keep_their_original_stats() {
        let mut game = empty_game();
        let mut stats = |species: Species| {
            let b = game.make_creature(&species, Vec2::ZERO);
            (b.radius, b.max_health, b.max_shield, b.mass)
        };
        assert_eq!(stats(Species::bogey()), (15.0, 35.0, 18.0, 8.0));
        assert_eq!(stats(Species::lunatic()), (18.0, 45.0, 0.0, 6.0));
        assert_eq!(stats(Species::smarty()), (18.0, 70.0, 0.0, 12.0));
        assert_eq!(stats(Species::fatso()), (48.0, 180.0, 0.0, 200.0));
        assert_eq!(stats(Species::leech()), (16.0, 40.0, 20.0, 7.0));
        // And they hunt at the original paces: smarties outrun fatsos.
        let pace = |species: Species| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            let id = spawn(&mut game, &species, Vec2::new(0.0, 900.0));
            for _ in 0..90 {
                game.step(DT, Input::default());
            }
            body(&game, id).velocity.length()
        };
        assert!(pace(Species::smarty()) > pace(Species::fatso()) * 2.0);
    }

    #[test]
    fn fling_is_a_continuous_trait_and_negative_mass_flings_by_itself() {
        let thrown = |genome: Genome| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            spawn(&mut game, &Species::of(genome), Vec2::new(30.0, 0.0));
            game.step(DT, Input::default());
            game.player().unwrap().velocity.length()
        };
        let calm = Genome {
            fling: 0.0,
            ..Genome::lunatic()
        };
        let weak = Genome { fling: 0.5, ..calm };
        let strong = Genome { fling: 1.5, ..calm };
        let negative = Genome {
            mass: -20.0,
            ..calm
        };
        assert!(thrown(calm) < FLING_SPEED * 0.5, "no fling gene, no fling");
        assert!(thrown(weak) < thrown(Genome::lunatic()));
        assert!(thrown(Genome::lunatic()) < thrown(strong));
        assert!(thrown(negative) > FLING_SPEED * 0.5, "negative mass flings");
        assert!(thrown(strong) <= FLING_HARD_CAP + 1.0);
    }

    #[test]
    fn negative_mass_is_repelled_by_gravity_wells() {
        let along = |mass: f32| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            add(&mut game, BodyKind::BlackHole, Vec2::new(300.0, 2000.0));
            let id = spawn(
                &mut game,
                &Species::of(Genome {
                    mass,
                    fling: 0.0,
                    speed: 30.0,
                    cruise: 10.0,
                    ..Genome::smarty()
                }),
                Vec2::new(0.0, 2000.0),
            );
            for _ in 0..30 {
                game.step(DT, Input::default());
            }
            body(&game, id).velocity.x
        };
        assert!(along(10.0) > 20.0);
        assert!(along(-10.0) < along(10.0) - 40.0);
    }

    #[test]
    fn any_species_can_carry_any_weapon() {
        // A smarty given a gun shoots; given a cord launcher it latches on.
        let mut game = empty_game();
        let armed = Genome {
            weapon: crate::genome::Weapon::Projectile,
            ..Genome::smarty()
        };
        spawn(&mut game, &Species::of(armed), Vec2::new(0.0, 500.0));
        game.step(DT, Input::default());
        assert!(game.bullets.iter().any(|b| !b.friendly));
        let mut game = empty_game();
        let hook = Genome {
            weapon: crate::genome::Weapon::Tether,
            ..Genome::fatso()
        };
        spawn(&mut game, &Species::of(hook), Vec2::new(0.0, 400.0));
        game.step(DT, Input::default());
        assert_eq!(game.tethers.len(), 1);
        // Without a siphoning diet the cord drags but does not feed.
        for _ in 0..200 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.player().unwrap().shield, 60.0);
    }

    #[test]
    fn fear_diet_and_brood_are_genes_too() {
        // Fear: a frightened creature backs away from the player instead of closing.
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let coward = Genome {
            fear: crate::genome::Fear::Player,
            ..Genome::smarty()
        };
        let id = spawn(&mut game, &Species::of(coward), Vec2::new(0.0, 600.0));
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert!(body(&game, id).position.y > 650.0);
        // Diet: a grazer eats a small rock it touches and ignores a big one.
        let mut game = empty_game();
        let grazer = Genome {
            diet: Diet::Rocks,
            ..Genome::fatso()
        };
        let eater = spawn(&mut game, &Species::of(grazer), Vec2::new(0.0, 2000.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == eater)
            .unwrap()
            .health = 50.0;
        let small = add(&mut game, BodyKind::Asteroid, Vec2::new(70.0, 2000.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == small)
            .unwrap()
            .radius = 20.0;
        let big = add(&mut game, BodyKind::Asteroid, Vec2::new(-90.0, 2000.0));
        game.bodies.iter_mut().find(|b| b.id == big).unwrap().radius = 45.0;
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert!(game.bodies.iter().all(|b| b.id != small));
        assert!(game.bodies.iter().any(|b| b.id == big));
        assert!(body(&game, eater).health > 50.0);
        // Brood: a tending parent bears a bounded number of small relatives that stay close.
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let parent = Genome {
            social: crate::genome::Social::Brood,
            ..Genome::smarty()
        };
        let species = Species::of(parent);
        let id = spawn(&mut game, &species, Vec2::new(0.0, 2400.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == id)
            .unwrap()
            .brood_timer = 0.1;
        for _ in 0..60 * 120 {
            game.step(DT, Input::default());
            let kids = game.bodies.iter().filter(|b| b.parent == Some(id)).count();
            assert!(kids <= 3);
        }
        let kids: Vec<&Body> = game
            .bodies
            .iter()
            .filter(|b| b.parent == Some(id))
            .collect();
        assert!(!kids.is_empty(), "nothing was born");
        assert!(kids.iter().all(|k| k.radius < body(&game, id).radius));
    }

    #[test]
    fn creatures_stay_finite_and_bounded_at_gene_extremes() {
        use crate::genome::Weapon;
        for stiffness in [30.0, 900.0] {
            for mass in [-60.0, 2.0, 300.0] {
                for speed in [30.0, 450.0] {
                    for (segments, limbs) in [(1, 0), (16, 0), (8, 6)] {
                        let mut game = empty_game();
                        game.player_invulnerability = 1e9;
                        add(&mut game, BodyKind::BlackHole, Vec2::new(500.0, 1500.0));
                        let genome = Genome {
                            stiffness,
                            mass,
                            speed,
                            cruise: speed * 0.4,
                            segments,
                            limbs,
                            limb_len: 3,
                            wave: 2.5,
                            rhythm: 7.0,
                            fling: 2.0,
                            weapon: Weapon::Projectile,
                            hardpoint_every: 2,
                            ..Genome::smarty()
                        }
                        .limited();
                        for i in 0..3 {
                            spawn(
                                &mut game,
                                &Species::of(genome),
                                Vec2::new(i as f32 * 90.0, 700.0),
                            );
                        }
                        for tick in 0..600 {
                            game.step(
                                DT,
                                Input {
                                    fire: true,
                                    thrust: 0.5,
                                    turn: 0.2,
                                    ..Default::default()
                                },
                            );
                            assert!(game.bodies.len() <= MAX_BODIES);
                            for b in &game.bodies {
                                assert!(
                                    b.position.is_finite()
                                        && b.velocity.is_finite()
                                        && (b.health.is_finite() || b.kind == BodyKind::BlackHole)
                                        && b.position.length() < 60_000.0
                                        && b.velocity.length() < 5_000.0,
                                    "stiffness {stiffness} mass {mass} speed {speed} \
                                     {segments}/{limbs} tick {tick}: {:?} {:?}",
                                    b.position,
                                    b.velocity
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn far_sectors_hold_novel_species_that_survive_a_long_flight() {
        let seed = 0x535343;
        let mut names = std::collections::HashSet::new();
        for (x, y) in [(14, 14), (-12, -9), (0, 9), (20, -5)] {
            let id = SectorId { x, y };
            let mut game = Game::new(seed);
            game.player_invulnerability = 1e9;
            game.teleport(id.center());
            for tick in 0..900 {
                game.step(
                    DT,
                    Input {
                        thrust: 0.6,
                        turn: if tick % 300 < 150 { 0.5 } else { -0.5 },
                        fire: true,
                        ..Default::default()
                    },
                );
                assert!(game.bodies.len() <= MAX_BODIES);
                assert!(
                    game.bodies
                        .iter()
                        .all(|b| b.position.is_finite() && b.velocity.is_finite())
                );
            }
            for b in game.bodies.iter().filter(|b| b.kind == BodyKind::Creature) {
                names.insert(b.genome.name());
            }
        }
        // Nothing here is a classic, and the casts are varied.
        assert!(names.len() > 8, "{names:?}");
        assert!(!names.contains("Bogey"));
    }
}
