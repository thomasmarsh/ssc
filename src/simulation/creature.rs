//! The generic creature. Every living thing is steered, armed and provoked by its genome;
//! nothing here asks what species a creature is. Tuning constants that used to belong to
//! one kind of enemy are now genes (sight, standoff, rage, fire period, ...).

use super::civ as civil;
use super::wildlife::Mode;
use super::*;
use crate::genome::{Diet, Fear, Social, Trigger, Weapon};

/// How far a creature notices same-species neighbors, how close it tolerates them, and how
/// far it may stray from the crowd's center before drifting back.
const PERCEPTION: f32 = 380.0;
const PERSONAL_SPACE: f32 = 110.0;
const LOOSE_RADIUS: f32 = 220.0;
/// True schools (passive schoolers that wait to be provoked) hold together more: they pull
/// stragglers back from nearer, keep a steadier common pace and fidget less, so a band
/// stays a band instead of straggling into singletons.
const SCHOOL_LOOSE_RADIUS: f32 = 170.0;
const SCHOOL_PULL_SPAN: f32 = 160.0;
/// How far a creature with a mass affinity notices rocks and gravity wells.
const HEAVY_RANGE: f32 = 600.0;
/// An enraged creature pursues the player this far, whatever its sight.
const RAGE_PURSUIT_RANGE: f32 = 1500.0;
/// Rage-capable creatures hunt cautiously until they frenzy.
const CAUTIOUS_PACE: f32 = 0.85;
const FRENZY_PACE: f32 = 1.35;
/// Closest a cord launcher will fire.
const TETHER_MIN_RANGE: f32 = 140.0;
/// How far fearful and grazing creatures look for bullets, wells and rocks.
const DODGE_RANGE: f32 = 300.0;
const WELL_FEAR_RANGE: f32 = 700.0;
const GRAZE_RANGE: f32 = 700.0;

impl Game {
    /// A creature body built entirely from a species' genome.
    pub(super) fn make_creature(&mut self, species: &Species, position: Vec2) -> Body {
        let genome = species.genome;
        let mut body = self.make_body(BodyKind::Creature, position);
        body.radius = genome.radius;
        body.health = genome.hull;
        body.max_health = genome.hull;
        body.shield = genome.shield;
        body.max_shield = genome.shield;
        body.mass = genome.body_mass();
        body.genome = genome;
        body.species = species.lineage;
        body.brood_timer = 4.0 + (body.id % 5) as f32 * 2.0;
        body.max_energy = genome.energy_capacity();
        body.energy = body.max_energy * food::starting_energy(&genome, body.id);
        body.generation = species.generation;
        body.breed_clock = growth::first_clock(&genome, body.id);
        if genome.learner > 0.0 {
            body.brain = Some(Box::new(Brain::new(body.id)));
        }
        body
    }

    /// Adds a creature to the world: a lone body, or a whole jointed creature when its
    /// genome calls for a spine or limbs. Returns the head's id.
    pub(super) fn add_body(&mut self, body: Body) -> u64 {
        if body.genome.is_jointed() {
            self.spawn_chain(body)
        } else {
            let id = body.id;
            self.bodies.push(body);
            id
        }
    }

    /// Creatures have no group membership. Each reacts only to same-species neighbors
    /// within its perception, so nearby bands merge into one drifting crowd and split
    /// apart again as individual restlessness pulls them in different directions.
    pub(super) fn steer_creatures(&mut self, dt: f32) {
        struct Neighbor {
            id: u64,
            species: u64,
            position: Vec2,
            radius: f32,
            heading: Vec2,
            chain: Option<u32>,
            heavy: bool,
            /// A rock a rooting creature may cling to.
            rock: bool,
            well: bool,
            grazable: bool,
            /// Free for a predator to chase: a lone, unprotected creature, and its mass.
            huntable: bool,
            mass: f32,
            raising_alarm: bool,
            alarm_range: f32,
            /// Belongs to a civilization: wildlife gives it room.
            civil: bool,
        }
        let player = self.player().map(|p| (p.position, p.velocity));
        // A ship landed on a pad and unseen is noticed at a fraction of the distance, and
        // creatures lose it for good once it has stayed hidden long enough; creatures that
        // know of a pad go for it instead (see `pads`).
        let (hide, hidden_long) = (self.pad.sight_mult(), self.pad.lost_track());
        let hidden = hide > 1.0;
        // A quiet ship in a dim field is noticed at a fraction of the distance.
        let dim = self.dim_notice();
        let sanctuary = self.sanctuary
            && player.is_some_and(|(p, _)| {
                crate::world::SectorId::containing(p) == crate::world::SectorId::ORIGIN
            });
        let sieges = self.siege_targets();
        // A compact snapshot makes steering independent of body iteration order.
        let neighbors: Vec<_> =
            self.bodies
                .iter()
                .filter(|b| b.active && b.kind != BodyKind::Player)
                .map(|b| {
                    let g = &b.genome;
                    let creature = b.kind == BodyKind::Creature;
                    Neighbor {
                        id: b.id,
                        species: if creature { b.species } else { 0 },
                        position: b.position,
                        radius: b.radius,
                        heading: Vec2::from_angle(b.wander),
                        chain: b.chain,
                        heavy: matches!(b.kind, BodyKind::Asteroid | BodyKind::BlackHole),
                        rock: root::can_host(b),
                        well: b.kind == BodyKind::BlackHole,
                        grazable: ecology::edible(b),
                        huntable: food::huntable(b),
                        mass: b.mass,
                        raising_alarm: creature
                            && match g.trigger {
                                // Hunters raise the alarm on seeing the player (packs, on
                                // being alert themselves); touchy creatures only when hurt.
                                Trigger::Sight if g.social == Social::Pack => b.alert,
                                Trigger::Sight => player
                                    .is_some_and(|(p, _)| p.distance(b.position) * hide < g.sight),
                                _ => b.enraged || is_hurt(b),
                            },
                        alarm_range: g.alarm,
                        civil: creature && self.civ_lineages.contains_key(&b.species),
                    }
                })
                .collect();
        let shots: Vec<(Vec2, Vec2)> = self
            .bullets
            .iter()
            .filter(|b| b.friendly)
            .map(|b| (b.position, b.velocity))
            .collect();
        let plankton: Vec<Vec2> = if self
            .bodies
            .iter()
            .any(|b| b.active && b.kind == BodyKind::Creature && b.genome.diet == Diet::Graze)
        {
            self.food.iter().map(|f| f.position).collect()
        } else {
            Vec::new()
        };
        let seed = self.seed;
        let crops: Vec<(Vec2, u16)> = self
            .farm
            .live
            .iter()
            .filter(|l| l.growth >= farm::GRAZE_FLOOR + 0.05)
            .map(|l| (l.position, l.species))
            .collect();
        let farm = &self.farm;
        let civs = self.civ_snapshot();
        let pulls = self.fauna_pulls();
        let homes = self.builder_homes();
        for body in self.bodies.iter_mut().filter(|b| b.active) {
            if body.kind != BodyKind::Creature {
                continue;
            }
            // Trailing parts are dragged along by their joints.
            if body.follower {
                continue;
            }
            let g = body.genome;
            let phenotype = body.genes;
            // Individuals differ a little in pace and in how restless they are.
            let temperament = (body.id % 5) as f32 / 4.0;
            let schooling = g.social == Social::School && g.trigger != Trigger::Sight;
            // A tired forager is slower; a fed one (or one that needs no food) is unchanged.
            let vigor = body.vigor();
            let pace_spread = if schooling { 0.1 } else { 0.3 };
            let cruise = g.cruise
                * (1.0 - pace_spread / 2.0 + pace_spread * temperament)
                * vigor
                * body.genes.foe.speed;
            let lead = g.lead * phenotype.sensor_acuity;
            let flock = g.flocking * phenotype.flocking;
            let perception = if g.social == Social::Solitary {
                0.0
            } else {
                PERCEPTION * flock
            };
            // Aggression moves pace only a little; it mostly shows in rage and fire rate.
            let speed =
                g.speed * (1.0 + (phenotype.aggression - 1.0) * 0.4) * vigor * phenotype.foe.speed;

            let mut separation = Vec2::ZERO;
            let mut heading = Vec2::ZERO;
            let mut center = Vec2::ZERO;
            let mut crowd = 0.0;
            let mut warned = false;
            let mut crowding = Vec2::ZERO;
            let mut heavy: Option<(f32, Vec2)> = None;
            let mut well: Option<(f32, Vec2)> = None;
            let mut meal: Option<(f32, Vec2)> = None;
            let mut perch: Option<(f32, Vec2)> = None;
            let seeking = body.wants_host();
            // An Oozer eats rocks, so rocks do not push it away.
            let rock_eater = crate::power::Power::Engulf.active(&g);
            let mut prey: Option<(f32, Vec2)> = None;
            let civ = self.civ_lineages.get(&body.species).copied();
            let pull = pulls.get(&body.id).copied();
            let mut civil_near: Option<(f32, Vec2)> = None;
            let hunting = body.hunts_prey();
            let prey_sight = (g.sight * phenotype.sensor_acuity).clamp(250.0, 900.0);
            for other in neighbors
                .iter()
                .filter(|n| n.id != body.id && (body.chain.is_none() || n.chain != body.chain))
            {
                let offset = body.position - other.position;
                let distance_squared = offset.length_squared();
                let range = body.radius + other.radius + 35.0;
                if seeking
                    && other.rock
                    && distance_squared < root::SEEK_RANGE * root::SEEK_RANGE
                    && perch.is_none_or(|(best, _)| distance_squared < best)
                {
                    perch = Some((distance_squared, -offset));
                }
                // A creature looking for a rock to cling to is not repelled by one.
                // An Oozer is not repelled by solid rock: it flows into the gaps between them.
                let squeezes = other.heavy && rock_eater;
                if !((seeking || rock_eater) && other.rock)
                    && !squeezes
                    && distance_squared < range * range
                    && distance_squared > 0.1
                {
                    crowding += offset / distance_squared * 6500.0;
                }
                if other.heavy
                    && !squeezes
                    && distance_squared < HEAVY_RANGE * HEAVY_RANGE
                    && heavy.is_none_or(|(best, _)| distance_squared < best)
                {
                    heavy = Some((distance_squared, -offset));
                }
                if other.well
                    && distance_squared < WELL_FEAR_RANGE * WELL_FEAR_RANGE
                    && well.is_none_or(|(best, _)| distance_squared < best)
                {
                    well = Some((distance_squared, -offset));
                }
                if other.grazable
                    && distance_squared < GRAZE_RANGE * GRAZE_RANGE
                    && meal.is_none_or(|(best, _)| distance_squared < best)
                {
                    meal = Some((distance_squared, -offset));
                }
                if hunting
                    && other.huntable
                    && other.species != body.species
                    && other.mass < body.mass * food::PREY_MASS_RATIO
                    && distance_squared < prey_sight * prey_sight
                    && prey.is_none_or(|(best, _)| distance_squared < best)
                {
                    prey = Some((distance_squared, -offset));
                }
                if other.civil
                    && civ.is_none()
                    && pull.is_none()
                    && distance_squared < civil::SHOO_RANGE * civil::SHOO_RANGE
                    && civil_near.is_none_or(|(best, _)| distance_squared < best)
                {
                    civil_near = Some((distance_squared, -offset));
                }
                if other.species != body.species || distance_squared > perception * perception {
                    continue;
                }
                let distance = distance_squared.sqrt().max(0.1);
                if distance < PERSONAL_SPACE {
                    separation += offset / distance * (1.0 - distance / PERSONAL_SPACE);
                }
                heading += other.heading;
                center += other.position;
                crowd += 1.0;
                warned |= other.raising_alarm && distance < other.alarm_range;
            }

            // What the creature perceives: far, when the ship is hiding.
            let player_distance =
                player.map_or(f32::INFINITY, |(p, _)| p.distance(body.position)) * hide * dim;
            // A civilization's members see further inside their own territory and are
            // rallied by comrades and calls to arms (see `civ`).
            let posture = civs.posture(
                civ,
                body.id,
                body.position,
                player_distance,
                g.lose * phenotype.sensor_acuity,
            );
            let (sight, lose) = (
                g.sight * phenotype.sensor_acuity * posture.reach,
                g.lose * phenotype.sensor_acuity * posture.reach,
            );
            body.enraged =
                g.rage > 0.0 && body.health < body.max_health * g.rage * phenotype.aggression;
            // Sight and proximity creatures notice the player by distance, with hysteresis;
            // touchy ones (anything but sight) also fly up when hurt, and rage pursues.
            let by_distance = g.trigger != Trigger::Harm
                && !posture.calm
                && player_distance < if body.alert && !hidden { lose } else { sight };
            // An apex elder hurt from beyond its sight is no less angry for it: it answers a
            // sniper (see `apexes::closers`) however far the shots came from.
            let elder_hurt = is_hurt(body)
                && body
                    .origin
                    .is_some_and(|key| self.apexes.contains_key(&key));
            let provoked = (g.trigger != Trigger::Sight && is_hurt(body))
                || elder_hurt
                || body.provoked > 0.0
                || (body.enraged
                    && player_distance < RAGE_PURSUIT_RANGE
                    // A calm civilization's person hurt by wildlife is not enraged at the ship.
                    && (!posture.calm || self.civ_struck.contains_key(&body.id)));
            body.alert = by_distance || (warned && !posture.calm) || provoked || posture.rallied;
            if hidden_long {
                // Out of sight long enough: only a harm done to it keeps a creature on the hunt.
                body.alert = provoked;
            }
            // A creature that knows of a pad goes for it.
            let siege = sieges.get(&body.id).copied();
            if siege.is_some() {
                body.alert = true;
            }
            // HOME is a sanctuary: nothing there goes after the ship, whatever it saw or
            // heard, unless it has been hurt (a creature from next door that wanders in
            // calms down). Fire always has an answer.
            if sanctuary {
                body.alert = provoked || is_hurt(body);
            }
            if (civ.is_some() && posture.calm) || body.panic > 0.0 {
                body.alert = false;
            }
            // A rooted creature does not steer: it holds its place, turning to face a threat.
            if body.root.is_some() {
                if let (true, Some((p, _))) = (body.alert, player) {
                    body.angle = (p - body.position).to_angle();
                }
                continue;
            }
            let enraged = body.enraged;
            let speed = match (g.rage > 0.0, enraged) {
                (true, true) => speed * FRENZY_PACE,
                (true, false) => speed * CAUTIOUS_PACE,
                _ => speed,
            };
            // A learner watches the ship while it hunts it and aims at where its brain
            // expects the ship to be; everyone else leads by a plain gene.
            let learner = g.learner;
            let to_player = match (player, body.brain.as_mut()) {
                (Some((p, v)), Some(brain)) if learner > 0.0 => {
                    brain.observe(
                        dt,
                        body.alert,
                        p,
                        v,
                        p - body.position,
                        brain::step_size(g.learn_rate),
                    );
                    Some(p + brain.aim_offset(v, lead, learner) - body.position)
                }
                _ => player.map(|(p, v)| p + v * lead - body.position),
            };

            // The pad is the target when it is nearer than the ship (or the ship is hidden).
            let to_player = match siege {
                Some(pad) if (pad - body.position).length() <= player_distance => {
                    Some(pad - body.position)
                }
                _ => to_player,
            };

            // The wander heading doubles as the shared heading that neighbors align to.
            if crowd > 0.0 {
                let blend = (dt * 1.2 * flock).min(1.0);
                let own = Vec2::from_angle(body.wander);
                let merged = own + (heading / crowd - own) * blend;
                body.wander = merged.y.atan2(merged.x);
            }
            let restless = if schooling {
                0.8 + 0.8 * temperament
            } else {
                1.5 + 2.0 * temperament
            };
            body.wander += (self.rng.f32() - 0.5) * restless * dt;

            if let (Some(home), false) = (body.home, body.alert || body.panic > 0.0) {
                let back = home - body.position;
                if back.length_squared() > HOME_LEASH * HOME_LEASH {
                    let own = Vec2::from_angle(body.wander);
                    let merged = own + (back.normalize() - own) * (dt * 1.5).min(1.0);
                    body.wander = merged.y.atan2(merged.x);
                }
            }
            let mut desired = match (body.alert, to_player) {
                _ if body.panic > 0.0 => {
                    // Scatter from the ruined base, veering erratically as they go.
                    let away = (body.position - body.panic_from).normalize_or_zero();
                    let swerve = (self.time * 7.0 + body.id as f32).sin() * 1.1;
                    body.wander = away.y.atan2(away.x) + swerve;
                    Vec2::from_angle(body.wander) * speed
                }
                (true, Some(difference)) => {
                    let mut desired = difference.normalize_or_zero() * speed;
                    let distance = difference.length();
                    if g.fear == Fear::Player {
                        desired = -desired;
                    } else if g.standoff > 0.0 && !enraged && distance < g.standoff {
                        desired = -desired * 0.65;
                    }
                    if g.strafe > 0.0 && distance < 520.0 {
                        // Strafe, each creature committing to its own side.
                        let side = Vec2::new(-difference.y, difference.x).normalize_or_zero();
                        desired +=
                            side * speed * g.strafe * if body.id % 2 == 0 { 1.0 } else { -1.0 };
                    }
                    body.wander = body.velocity.y.atan2(body.velocity.x);
                    desired
                }
                _ => Vec2::from_angle(body.wander) * cruise,
            };
            if crowd > 0.0 {
                // Loose cohesion: only stragglers drift back, so the crowd stays spread out.
                let to_center = center / crowd - body.position;
                let gap = to_center.length();
                let (radius, span) = if schooling {
                    (SCHOOL_LOOSE_RADIUS, SCHOOL_PULL_SPAN)
                } else {
                    (LOOSE_RADIUS, 200.0)
                };
                if gap > radius {
                    desired += to_center / gap * cruise * ((gap - radius) / span).min(1.0) * flock;
                }
                desired += separation * speed * 1.4 * flock;
            }
            desired += crowding;
            if let Some((_, toward)) = heavy {
                // Some creatures hug rocks and wells; others give them a wide berth.
                desired += toward.normalize_or_zero()
                    * cruise
                    * (g.mass_affinity + phenotype.mass_affinity);
            }
            if let (true, Some((_, toward))) = (seeking && !body.alert, perch) {
                desired += toward.normalize_or_zero() * cruise * 1.8;
            }
            match g.fear {
                Fear::Wells => {
                    if let Some((_, toward)) = well {
                        desired -= toward.normalize_or_zero() * cruise * 3.0;
                    }
                }
                Fear::Bullets => {
                    // Sidestep the nearest incoming shot.
                    let incoming = shots
                        .iter()
                        .filter(|(p, v)| {
                            let offset = body.position - *p;
                            offset.length() < DODGE_RANGE && v.dot(offset) > 0.0
                        })
                        .min_by(|a, b| {
                            a.0.distance_squared(body.position)
                                .total_cmp(&b.0.distance_squared(body.position))
                        });
                    if let Some((p, v)) = incoming {
                        let side = Vec2::new(-v.y, v.x).normalize_or_zero();
                        let sign = if side.dot(body.position - *p) >= 0.0 {
                            1.0
                        } else {
                            -1.0
                        };
                        desired += side * sign * speed * 0.9;
                    }
                }
                _ => {}
            }
            // Wildlife gives a civilization's creatures room, and is edged off its ground.
            if let Some((_, toward)) = civil_near {
                desired -= toward.normalize_or_zero() * cruise * 1.6;
            }
            // Hostile wildlife sets upon what it hates, a civilization's idle people go after
            // it, and friendly wildlife drifts in to the settlement (see `wildlife`).
            let mut striking = false;
            if let Some(p) = pull {
                match p.mode {
                    Mode::Attack | Mode::Defend if !body.alert => {
                        let pace = if p.mode == Mode::Attack {
                            tuning::ATTACK_PACE
                        } else {
                            tuning::DEFEND_PACE
                        };
                        desired += p.toward.normalize_or_zero() * speed * pace;
                        striking = true;
                    }
                    Mode::Herd if p.distance > tuning::HERD_RING => {
                        desired += p.toward.normalize_or_zero() * cruise * tuning::HERD_PULL;
                    }
                    _ => {}
                }
            }
            if let (Diet::Rocks, Some((_, toward)), false) = (g.diet, meal, body.alert) {
                desired += toward.normalize_or_zero() * cruise * 1.2;
            }
            // Hunger beats hostility: a grazer weak with hunger breaks off its pursuit to eat.
            let desperate = body.energy_fraction() < food::DESPERATE_BELOW;
            if body.grazes_plankton()
                && (!body.alert || desperate)
                && let Some(toward) = plankton
                    .iter()
                    .map(|&p| p - body.position)
                    .filter(|d| d.length_squared() < food::FOOD_SIGHT * food::FOOD_SIGHT)
                    .min_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
            {
                let pull = if body.alert {
                    speed * 1.1
                } else {
                    cruise * 1.2
                };
                desired += toward.normalize_or_zero() * pull;
            }
            // Plants are food too, for a palate that likes them. They stand on a planetoid that
            // also pushes grazers away, so the pull to a plant is the stronger of the two. A school
            // keeps its shape (its members still graze whatever they drift past).
            if body.grazes_plankton()
                && (!body.alert || desperate)
                && !schooling
                && !crops.is_empty()
                && let Some(toward) = {
                    let palate = crate::flora::creature_palate(seed, body.species);
                    crops
                        .iter()
                        .filter(|(_, s)| farm.flora(*s).is_some_and(|f| palate.eats(&f.chemistry)))
                        .map(|(p, _)| *p - body.position)
                        .filter(|d| d.length_squared() < food::FOOD_SIGHT * food::FOOD_SIGHT)
                        .min_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
                }
            {
                desired += toward.normalize_or_zero() * cruise * 2.5;
            }
            // A builder in the middle of a structure stays within reach of it.
            if let (false, Some(home)) = (body.alert, homes.get(&body.id)) {
                desired += super::build::home_pull(body.position, *home, cruise);
            }
            let chasing = !body.alert && prey.is_some();
            if let (true, Some((_, toward))) = (chasing, prey) {
                desired += toward.normalize_or_zero() * speed * 0.7;
            }
            let top_speed = if body.alert {
                speed
            } else if chasing {
                (cruise * 1.4).max(speed * 0.7)
            } else if striking {
                speed
            } else {
                cruise * 1.4
            };
            // A jointed creature's head steers with the muscle to tow the body it drags.
            let tow = body
                .chain
                .and_then(|c| self.chains.get(&c))
                .map_or(1.0, |c| (c.len() as f32 * 0.6).max(1.0));
            body.velocity +=
                (desired.clamp_length_max(top_speed) - body.velocity) * (dt * 2.2 * tow).min(1.0);
            body.angle = match to_player {
                Some(difference) if body.alert => difference.y.atan2(difference.x),
                _ if body.velocity.length_squared() > 25.0 => {
                    body.velocity.y.atan2(body.velocity.x)
                }
                _ => body.angle,
            };
        }
    }

    /// Hardpoints fire. Any part of any creature can carry a gun or cord launcher, as its
    /// genome says; only hunting creatures shoot, and only at targets in range.
    pub(super) fn fire_weapons(&mut self) {
        let Some((target, ship_velocity)) = self.player().map(|p| (p.position, p.velocity)) else {
            return;
        };
        let walled = self.bodies.iter().any(|b| b.rock == RockKind::Wall);
        for index in 0..self.bodies.len() {
            let body = &self.bodies[index];
            let g = body.genome;
            if !(body.active
                && body.kind == BodyKind::Creature
                && body.alert
                && body.panic <= 0.0
                && body.fire_cooldown <= 0.0
                && !body.phased
                && self.armed_part(body))
            {
                continue;
            }
            let distance = body.position.distance(target);
            // A learner leads its shots with its brain's guess of where the ship will be
            // when they arrive; everyone else shoots at where it is.
            let aim_at = match body.brain.as_ref() {
                Some(brain) if g.learner > 0.0 => {
                    let speed = if g.weapon == Weapon::Tether {
                        tether::TIP_SPEED
                    } else {
                        g.shot_speed
                    };
                    let flight = distance / speed.max(1.0);
                    target + brain.shot_offset(ship_velocity, flight, g.learner)
                }
                _ => target,
            };
            let direction = (aim_at - body.position).normalize_or_zero();
            // A rooted defender covers only the open side of its rock, never through it.
            if let Some(root) = body.root
                && let Some(host) = self.body(root.host)
                && direction.dot((body.position - host.position).normalize_or_zero())
                    < root::FIRE_ARC
            {
                continue;
            }
            // No shooting at a fortress wall: without a clear line to the ship, hold fire.
            if walled && g.weapon != Weapon::Tether && !self.clear_shot(body.position, aim_at) {
                continue;
            }
            let pace = if g.rage > 0.0 {
                if body.enraged { 0.4 } else { 1.15 }
            } else {
                1.0
            };
            let period = (g.fire_period + (body.id % 7) as f32 * 0.13) * pace
                / body.genes.aggression.max(0.2);
            if g.weapon == Weapon::Mine && crate::power::Power::Rune.active(&g) {
                continue;
            }
            match g.weapon {
                Weapon::None => {}
                Weapon::Tether => {
                    // A weaver's launchers build its web instead of latching onto the ship.
                    if crate::power::Power::Weave.active(&g) {
                        continue;
                    }
                    let id = body.id;
                    if (TETHER_MIN_RANGE..g.weapon_range).contains(&distance)
                        && self.tethers.len() < tether::MAX_TETHERS
                        && !self
                            .tethers
                            .iter()
                            .any(|t| t.owner == id && t.kind == TetherKind::Latch)
                    {
                        let from = body.position;
                        let rooted = body.root.is_some();
                        let cord = Cord::from_genome(&g, body.genes.threat, rooted);
                        self.tethers.push(Tether::latch_with(
                            id, from, direction, g.reel, cord, rooted,
                        ));
                        self.bodies[index].fire_cooldown = g.fire_period;
                    }
                }
                weapon => {
                    let range = g.weapon_range * if body.enraged { 1.1875 } else { 1.0 };
                    if distance < range && self.bullets.len() < MAX_BULLETS {
                        let muzzle = weapons::Muzzle {
                            civilization: self.civ_of(body).map(|(id, _)| id),
                            origin: body.position + direction * (body.radius + 5.0),
                            aim: direction,
                            velocity: body.velocity,
                            reach: g.weapon_range,
                            shot_speed: if g.bypass_share() > 0.0 {
                                g.shot_speed.min(crate::power::BYPASS_SHOT_SPEED)
                            } else {
                                g.shot_speed
                            },
                            sharpness: body.genes.sharpness(),
                            pith: g.bypass_share(),
                        };
                        let spin = body.spin;
                        let spun = self.discharge(weapon, g.volley, &muzzle, spin);
                        self.bodies[index].spin = spun;
                        self.bodies[index].fire_cooldown = period * weapons::pace(weapon);
                    }
                }
            }
        }
    }
}
