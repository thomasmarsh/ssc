//! Drops and the ship's loadout in play. Creatures shed items according to what they are
//! (see `upgrades::Source`), items drift until the ship flies close enough to be pulled in,
//! and what is collected is folded into the ship's `Stats`. A kill's drop is a pure function
//! of the world seed and the spawn it came from, so a route through the universe always
//! pays the same; creatures with no spawn (bred, shards) draw from the loot stream.

use super::upgrades::{self, Install, Slot, Source};
use super::*;
use crate::world::hash2;
use crate::world::{BaseKind, RockKind};

/// Separates loot from every other stream.
pub(super) const LOOT_SALT: u64 = 0x100D_5A1D_0000_0007;
const MAX_PICKUPS: usize = 96;
/// A pickup is collected when the ship's edge is this close.
const PICKUP_RADIUS: f32 = 12.0;
const MAX_LIVES: u32 = 6;
const MAX_NOTICES: usize = 5;

#[derive(Clone, Debug)]
pub struct Pickup {
    pub position: Vec2,
    pub velocity: Vec2,
    pub item: Item,
    pub age: f32,
    /// Seconds until it fades away.
    pub remaining: f32,
}

#[derive(Clone, Debug)]
pub struct Notice {
    pub text: String,
    pub rarity: upgrades::Rarity,
    pub remaining: f32,
}

fn lifetime(item: &Item) -> f32 {
    match item {
        Item::Part(_) | Item::Life => 90.0,
        Item::Surge(_) => 50.0,
        _ => 30.0,
    }
}

impl Game {
    /// The threat of the quadrant the ship is in.
    pub fn threat(&self) -> f32 {
        world::threat(self.params().depth)
    }

    /// How much ship the player has (the bare ship is 1).
    pub fn power(&self) -> f32 {
        self.stats.power()
    }

    /// Recomputes the stats from the loadout and fits the ship to them: a bigger hull or
    /// shield also arrives filled by the difference, a smaller one is trimmed.
    pub(super) fn refresh_stats(&mut self) {
        let stats = self.loadout.stats();
        self.stats = stats;
        let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) else {
            return;
        };
        let (hull, shield) = (
            stats.max_hull - ship.max_health,
            stats.max_shield - ship.max_shield,
        );
        ship.max_health = stats.max_hull;
        ship.max_shield = stats.max_shield;
        ship.health = (ship.health + hull.max(0.0)).min(ship.max_health);
        ship.shield = (ship.shield + shield.max(0.0)).min(ship.max_shield);
        ship.rig = Rig {
            guard: stats.guard,
            ram: stats.ram,
            aura: stats.aura,
            ballast: stats.ballast,
        };
    }

    /// Runs surges down and ages notices.
    pub(super) fn update_loadout(&mut self, dt: f32) {
        if self.loadout.tick(dt) {
            self.refresh_stats();
        }
        for notice in &mut self.notices {
            notice.remaining -= dt;
        }
        self.notices.retain(|n| n.remaining > 0.0);
    }

    fn notify(&mut self, text: String, rarity: upgrades::Rarity) {
        if self.notices.len() >= MAX_NOTICES {
            self.notices.remove(0);
        }
        self.notices.push(Notice {
            text,
            rarity,
            remaining: 4.0 + 2.0 * rarity as usize as f32,
        });
    }

    /// Takes an item aboard: restoratives act at once, parts are bolted on, surges start.
    pub fn collect(&mut self, item: Item) {
        let rarity = item.rarity();
        self.cue(Cue::Pickup { rarity });
        match item {
            Item::Repair(amount) => {
                if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                    ship.health = (ship.health + amount).min(ship.max_health);
                }
                self.notify(format!("HULL +{amount:.0}"), rarity);
            }
            Item::Recharge(amount) => {
                if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player) {
                    ship.shield = (ship.shield + amount).min(ship.max_shield);
                }
                self.notify(format!("SHIELD +{amount:.0}"), rarity);
            }
            Item::Life => {
                self.lives = (self.lives + 1).min(MAX_LIVES);
                self.notify("EXTRA LIFE".into(), rarity);
            }
            Item::Scrap(points) => {
                self.score = self.score.saturating_add(u64::from(points));
                self.notify(format!("SALVAGE +{points}"), rarity);
            }
            Item::Part(part) => {
                let name = part.name.to_uppercase();
                let summary = part.summary();
                match self.loadout.install(part) {
                    Install::Added => self.notify(format!("INSTALLED  {name}  {summary}"), rarity),
                    Install::Replaced(old) => self.notify(
                        format!(
                            "INSTALLED  {name}  {summary}  (replaces {})",
                            old.name.to_uppercase()
                        ),
                        rarity,
                    ),
                    Install::Scrapped(part) => {
                        let value = (part.rating() * 200.0) as u64;
                        self.score = self.score.saturating_add(value);
                        self.notify(
                            format!("SCRAPPED  {name}  +{value}"),
                            upgrades::Rarity::Common,
                        );
                    }
                }
                self.refresh_stats();
            }
            Item::Surge(surge) => {
                self.notify(
                    format!(
                        "{}  {:.0}s  {}",
                        surge.name.to_uppercase(),
                        surge.duration,
                        surge.summary()
                    ),
                    rarity,
                );
                self.loadout.start(surge);
                self.refresh_stats();
            }
        }
    }

    /// The stream a body's drop is rolled from: fixed by the world for generated spawns.
    fn loot_rng(&mut self, body: &Body) -> Rng {
        match body.origin {
            Some((quadrant, index)) => {
                let key = (u64::from(index) << 8 | u64::from(body.part))
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15);
                Rng::new(hash2(self.seed ^ LOOT_SALT ^ key, quadrant.x, quadrant.y))
            }
            None => Rng::new(self.loot.next_u64()),
        }
    }

    /// Rolls and scatters whatever a destroyed body leaves behind.
    pub(super) fn drop_loot(&mut self, body: &Body) {
        let mut rng = self.loot_rng(body);
        let params = world::latent(self.seed, QuadrantId::containing(body.position));
        let mut drops: Vec<Item> = Vec::new();
        match body.kind {
            BodyKind::Creature => {
                let genome = &body.genome;
                // Better bounties drop more; a jointed creature shares one drop's worth among
                // its parts, and creatures bred by bases are poor farming.
                let bred = if body.origin.is_some() { 1.0 } else { 0.35 };
                let chance = (0.08 + 0.4 * genome.bounty / 400.0) / genome.parts() as f32 * bred;
                if rng.chance(chance) {
                    let source = Source::of_creature(genome, body.genes.threat, params);
                    drops.push(upgrades::roll_item(&mut rng, &source));
                }
            }
            BodyKind::Asteroid if !body.pinned && body.radius >= 30.0 => {
                let grade = world::threat(params.depth);
                let mut source = Source::plain(grade, params);
                match body.rock {
                    RockKind::Plain | RockKind::Husk => {
                        if rng.chance(0.07) {
                            drops.push(upgrades::roll_salvage(&mut rng, &source));
                        }
                    }
                    // Ice melts into shield charge.
                    RockKind::Ice => {
                        if rng.chance(0.22) {
                            drops.push(Item::Recharge(25.0 + 15.0 * grade.sqrt()));
                        }
                    }
                    // Ore is mostly scrap, now and then a salvaged part of heavy gear.
                    RockKind::Ore => {
                        if rng.chance(0.2) {
                            if rng.chance(0.2) {
                                source.affinity[Slot::Plating.index()] += 3.0;
                                source.affinity[Slot::Engine.index()] += 2.0;
                                drops.push(Item::Part(upgrades::roll_part(&mut rng, &source)));
                            } else {
                                drops.push(Item::Scrap((45.0 * grade) as u32 / 5 * 5));
                            }
                        }
                    }
                    // Crystal holds charge: a surge is often found in the shards.
                    RockKind::Crystal => {
                        if rng.chance(0.45) {
                            drops.push(Item::Surge(upgrades::roll_surge(&mut rng, &source)));
                        }
                    }
                }
            }
            BodyKind::Base => {
                // A fallen base pays out: a guaranteed part and a couple of lucky rolls,
                // shaped by what the station was.
                let mut source = Source::plain(body.genes.threat, params);
                source.bias = 1.0;
                let kind = body.base.as_ref().map_or(BaseKind::Hive, |b| b.kind);
                let (extra_parts, extra_rolls) = match kind {
                    BaseKind::Hive => {
                        source.affinity[Slot::Core.index()] += 2.0;
                        (0, 2)
                    }
                    BaseKind::Foundry => {
                        source.affinity[Slot::Plating.index()] += 2.0;
                        source.affinity[Slot::Engine.index()] += 2.0;
                        (0, 3)
                    }
                    BaseKind::Bastion => {
                        source.affinity[Slot::Cannon.index()] += 3.0;
                        (1, 2)
                    }
                    BaseKind::Depot => {
                        source.affinity[Slot::Aux.index()] += 3.0;
                        (0, 3)
                    }
                };
                if let Some((weapon, _)) = body.base.as_ref().and_then(|b| b.arms) {
                    source.weapon = weapon;
                }
                for _ in 0..=extra_parts {
                    drops.push(Item::Part(upgrades::roll_part(&mut rng, &source)));
                }
                for _ in 0..extra_rolls {
                    drops.push(upgrades::roll_item(&mut rng, &source));
                }
            }
            _ => {}
        }
        for item in drops {
            let velocity = rng.direction() * rng.range(30.0, 110.0);
            self.drop_item(body.position, velocity, item);
        }
    }

    /// Leaves an item floating at `position`.
    pub fn drop_item(&mut self, position: Vec2, velocity: Vec2, item: Item) {
        if self.pickups.len() >= MAX_PICKUPS {
            self.pickups.remove(0);
        }
        self.pickups.push(Pickup {
            position,
            velocity,
            remaining: lifetime(&item),
            item,
            age: 0.0,
        });
    }

    /// Siphon: kills near the ship mend its hull.
    pub(super) fn siphon(&mut self, fallen: &Body) {
        let level = self.stats.siphon;
        if level == 0 || !matches!(fallen.kind, BodyKind::Creature | BodyKind::Base) {
            return;
        }
        if let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player)
            && ship.position.distance(fallen.position) < 1500.0
        {
            ship.health = (ship.health + 3.0 * f32::from(level)).min(ship.max_health);
        }
    }

    /// A wrecked ship loses its surges and its best part, which is left floating where it
    /// died for the respawned ship to recover.
    pub(super) fn shed_on_death(&mut self, position: Vec2) {
        self.loadout.surges.clear();
        if let Some(best) = self.loadout.best_part() {
            let part = self.loadout.parts.remove(best);
            self.notify(
                format!("LOST  {}  - recover it", part.name.to_uppercase()),
                part.rarity,
            );
            let drift = Vec2::from_angle(self.loot.f32() * TAU) * 40.0;
            self.drop_item(position, drift, Item::Part(part));
        }
        self.refresh_stats();
    }

    /// Pickups drift and slow, are drawn in by the ship's magnet, and are taken on contact.
    pub(super) fn update_pickups(&mut self, dt: f32) {
        let ship = self.player().map(|p| (p.position, p.radius));
        let magnet = self.stats.magnet;
        let mut taken = Vec::new();
        for (index, pickup) in self.pickups.iter_mut().enumerate() {
            pickup.age += dt;
            pickup.remaining -= dt;
            pickup.velocity *= (-1.6 * dt).exp();
            if let Some((position, radius)) = ship {
                let offset = position - pickup.position;
                let distance = offset.length();
                if distance < radius + PICKUP_RADIUS {
                    taken.push(index);
                    continue;
                }
                let reach = magnet + radius;
                if distance < reach {
                    let pull = 300.0 + 1500.0 * (1.0 - distance / reach);
                    pickup.velocity += offset / distance.max(1.0) * pull * dt;
                    pickup.velocity = pickup.velocity.clamp_length_max(1100.0);
                }
            }
            pickup.position += pickup.velocity * dt;
        }
        let mut items = Vec::new();
        for &index in taken.iter().rev() {
            items.push(self.pickups.remove(index).item);
        }
        self.pickups.retain(|p| p.remaining > 0.0);
        for item in items.into_iter().rev() {
            self.collect(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Species;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    use upgrades::{Effect, Part as ShipPart, Rarity, Slot, Stat, Surge, Trait};

    fn plating(bonus: f32) -> ShipPart {
        ShipPart {
            name: "Test Plate".into(),
            slot: Slot::Plating,
            rarity: Rarity::Rare,
            grade: 1.0,
            effects: vec![Effect::Stat(Stat::Hull, bonus)],
        }
    }

    fn surge(effect: Effect, seconds: f32) -> Surge {
        Surge {
            name: "Test Surge".into(),
            slot: Slot::Cannon,
            rarity: Rarity::Common,
            effects: vec![effect],
            duration: seconds,
            remaining: seconds,
        }
    }

    fn fire() -> Input {
        Input {
            fire: true,
            ..Default::default()
        }
    }

    #[test]
    fn installing_a_part_changes_the_ship_and_fills_the_new_hull() {
        let mut game = empty_game();
        let ship = game.player().unwrap();
        assert_eq!((ship.max_health, ship.health), (100.0, 100.0));
        game.collect(Item::Part(plating(0.5)));
        let ship = game.player().unwrap();
        assert_eq!((ship.max_health, ship.health), (150.0, 150.0));
        assert_eq!(game.stats.max_hull, 150.0);
        assert!(game.power() > 1.0);
        assert_eq!(game.notices.len(), 1);
    }

    #[test]
    fn a_surge_boosts_then_ends() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Stat(Stat::FireRate, 1.0), 2.0)));
        assert!(game.stats.fire_period < 0.09);
        for _ in 0..150 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.stats.fire_period, Stats::BASE.fire_period);
        assert!(game.loadout.surges.is_empty());
    }

    #[test]
    fn spread_fires_a_fan_and_each_shot_is_weaker() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Spread, 2), 30.0)));
        game.step(DT, fire());
        assert_eq!(game.bullets.len(), 5);
        let center = game.bullets.iter().map(|b| b.damage).fold(0.0, f32::max);
        assert_eq!(center, Stats::BASE.damage);
        assert!(game.bullets.iter().any(|b| b.damage < center));
    }

    #[test]
    fn broadside_and_tail_guns_fire_sideways_and_back() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Broadside, 1), 30.0)));
        game.collect(Item::Surge(Surge {
            name: "Tail".into(),
            ..surge(Effect::Trait(Trait::Tailgun, 1), 30.0)
        }));
        game.step(DT, fire());
        assert_eq!(game.bullets.len(), 4);
        let forward = Vec2::from_angle(game.player().unwrap().angle);
        let dots: Vec<f32> = game
            .bullets
            .iter()
            .map(|b| b.velocity.normalize().dot(forward))
            .collect();
        assert!(dots.iter().any(|d| *d > 0.9));
        assert!(dots.iter().any(|d| *d < -0.9));
        assert_eq!(dots.iter().filter(|d| d.abs() < 0.2).count(), 2);
    }

    #[test]
    fn damage_and_fire_rate_follow_the_stats() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Stat(Stat::Damage, 1.0), 30.0)));
        game.step(DT, fire());
        assert_eq!(game.bullets[0].damage, 52.0);
    }

    #[test]
    fn piercing_shots_pass_through_a_line_of_targets_once_each() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Pierce, 2), 30.0)));
        let ids: Vec<u64> = [300.0, 360.0, 420.0, 480.0]
            .into_iter()
            .map(|x| add(&mut game, BodyKind::Asteroid, Vec2::new(x, 0.0)))
            .collect();
        for id in &ids {
            let rock = game.bodies.iter_mut().find(|b| b.id == *id).unwrap();
            rock.radius = 12.0;
            rock.pinned = true;
            rock.health = 1000.0;
        }
        game.player_invulnerability = 1e9;
        // Face down the line.
        game.bodies[0].angle = 0.0;
        game.step(DT, fire());
        for _ in 0..40 {
            game.step(
                DT,
                Input {
                    aim_direction: Some(Vec2::X),
                    ..Default::default()
                },
            );
        }
        let struck = ids
            .iter()
            .filter(|id| body(&game, **id).health < 1000.0)
            .count();
        // Pierce 2 passes two bodies and ends in the third, and nothing is struck twice.
        assert_eq!(struck, 3);
        for id in ids.iter().take(3) {
            assert_eq!(body(&game, *id).health, 1000.0 - Stats::BASE.damage);
        }
    }

    #[test]
    fn a_blast_hurts_neighbors_of_the_target() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Blast, 1), 30.0)));
        let near = add(&mut game, BodyKind::Asteroid, Vec2::new(300.0, 0.0));
        let beside = add(&mut game, BodyKind::Asteroid, Vec2::new(300.0, 80.0));
        let far = add(&mut game, BodyKind::Asteroid, Vec2::new(300.0, 400.0));
        for id in [near, beside, far] {
            let rock = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            rock.pinned = true;
            rock.radius = 20.0;
            rock.health = 500.0;
        }
        game.bodies[0].angle = 0.0;
        game.step(DT, fire());
        for _ in 0..40 {
            game.step(DT, Input::default());
        }
        assert!(body(&game, near).health < 500.0);
        assert!(body(&game, beside).health < 500.0);
        assert_eq!(body(&game, far).health, 500.0);
    }

    #[test]
    fn homing_shots_curve_toward_a_target_off_their_line() {
        let run = |level: u8| {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            if level > 0 {
                game.collect(Item::Surge(surge(
                    Effect::Trait(Trait::Homing, level),
                    30.0,
                )));
            }
            let target = spawn(&mut game, &Species::fatso(), Vec2::new(500.0, 160.0));
            game.bodies
                .iter_mut()
                .find(|b| b.id == target)
                .unwrap()
                .genome
                .speed = 0.0;
            game.bodies[0].angle = 0.0;
            game.step(DT, fire());
            for _ in 0..50 {
                game.step(
                    DT,
                    Input {
                        aim_direction: Some(Vec2::X),
                        ..Default::default()
                    },
                );
            }
            body(&game, target).health < body(&game, target).max_health
        };
        assert!(!run(0));
        assert!(run(2));
    }

    #[test]
    fn armor_softens_hits() {
        let mut plain = empty_game();
        let mut armored = empty_game();
        armored.collect(Item::Surge(surge(Effect::Stat(Stat::Armor, 1.0), 30.0)));
        for game in [&mut plain, &mut armored] {
            let ship = &mut game.bodies[0];
            damage(ship, 40.0, 0.0);
        }
        let lost = |game: &Game| {
            let ship = game.player().unwrap();
            ship.max_shield - ship.shield + ship.max_health - ship.health
        };
        assert_eq!(lost(&plain), 40.0);
        assert_eq!(lost(&armored), 20.0);
    }

    #[test]
    fn deep_fauna_is_tougher_and_deadlier() {
        let mut game = empty_game();
        let mut hurt = |threat: f32| {
            let id = spawn(&mut game, &Species::fatso(), Vec2::new(3000.0, 0.0));
            let creature = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
            creature.genes.threat = threat;
            let before = creature.health;
            damage(creature, 30.0, 0.0);
            let lost = before - creature.health;
            let sharp = creature.genes.sharpness();
            (lost, sharp)
        };
        let (shallow, sharp_shallow) = hurt(1.0);
        let (deep, sharp_deep) = hurt(4.0);
        assert_eq!(shallow, 30.0);
        assert!((deep - 7.5).abs() < 1e-4);
        assert_eq!(sharp_shallow, 1.0);
        assert!(sharp_deep > 2.5);
    }

    #[test]
    fn ballast_ignores_wells_and_shears_cut_cords() {
        let mut game = empty_game();
        add(&mut game, BodyKind::BlackHole, Vec2::new(40.0, 0.0));
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Ballast, 1), 60.0)));
        let hull = game.player().unwrap().health;
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.player().unwrap().health, hull);

        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Shears, 1), 60.0)));
        let leech = spawn(&mut game, &Species::leech(), Vec2::new(0.0, 400.0));
        game.tethers
            .push(Tether::latch(leech, Vec2::new(0.0, 400.0), -Vec2::Y, 55.0));
        game.step(DT, Input::default());
        game.step(DT, Input::default());
        assert!(!game.tethered());
    }

    #[test]
    fn a_lunatic_field_flings_what_touches_the_ship_and_ramming_hurts_it() {
        let mut game = empty_game();
        game.collect(Item::Surge(Surge {
            effects: vec![Effect::Trait(Trait::Aura, 2)],
            ..surge(Effect::Trait(Trait::Aura, 2), 30.0)
        }));
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(30.0, 0.0));
        let hull = game.player().unwrap().health;
        game.step(DT, Input::default());
        assert!(body(&game, rock).velocity.length() > 200.0, "not flung");
        assert_eq!(game.player().unwrap().health, hull, "contact hurt the ship");

        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Ram, 2), 30.0)));
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(30.0, 0.0));
        set_player(&mut game, Vec2::ZERO, Vec2::new(300.0, 0.0));
        let before = body(&game, rock).health;
        game.step(DT, Input::default());
        assert!(body(&game, rock).health < before - 10.0);
    }

    #[test]
    fn pickups_are_drawn_in_and_taken_and_expire_when_ignored() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        game.drop_item(Vec2::new(100.0, 0.0), Vec2::ZERO, Item::Repair(10.0));
        game.drop_item(Vec2::new(3000.0, 0.0), Vec2::ZERO, Item::Scrap(50));
        let score = game.score;
        for _ in 0..90 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.pickups.len(), 1, "near one should be taken");
        assert_eq!(game.score, score);
        for _ in 0..60 * 31 {
            game.step(DT, Input::default());
        }
        assert!(game.pickups.is_empty(), "unclaimed salvage fades");
        // A bigger magnet reaches farther.
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Stat(Stat::Magnet, 3.0), 60.0)));
        game.drop_item(Vec2::new(450.0, 0.0), Vec2::ZERO, Item::Scrap(50));
        for _ in 0..90 {
            game.step(DT, Input::default());
        }
        assert!(game.pickups.is_empty());
    }

    #[test]
    fn extra_lives_are_capped_and_scrap_scores() {
        let mut game = empty_game();
        for _ in 0..10 {
            game.collect(Item::Life);
        }
        assert_eq!(game.lives, MAX_LIVES);
        game.collect(Item::Scrap(75));
        assert_eq!(game.score, 75);
    }

    #[test]
    fn killing_creatures_yields_drops_that_are_the_same_every_time() {
        let kills = |seed: u64| {
            let mut game = Game::new(seed);
            game.player_invulnerability = 1e9;
            let ids: Vec<u64> = game
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Creature && b.origin.is_some())
                .map(|b| b.id)
                .collect();
            assert!(ids.len() > 20);
            for id in ids {
                game.bodies.iter_mut().find(|b| b.id == id).unwrap().health = 0.0;
            }
            game.step(DT, Input::default());
            game.pickups
                .iter()
                .map(|p| (p.item.name(), p.item.rarity() as u8))
                .collect::<Vec<_>>()
        };
        let first = kills(11);
        assert!(!first.is_empty(), "a whole population dropped nothing");
        assert_eq!(first, kills(11));
        assert_ne!(first, kills(12));
    }

    #[test]
    fn a_fallen_base_pays_out_a_part() {
        let mut game = empty_game();
        let base = add(&mut game, BodyKind::Base, Vec2::new(0.0, 2000.0));
        game.bodies
            .iter_mut()
            .find(|b| b.id == base)
            .unwrap()
            .health = 0.0;
        game.step(DT, Input::default());
        assert!(game.pickups.iter().any(|p| matches!(p.item, Item::Part(_))));
        assert!(game.pickups.len() >= 3);
    }

    #[test]
    fn dying_costs_surges_and_the_best_part_but_it_can_be_recovered() {
        let mut game = empty_game();
        game.collect(Item::Part(plating(0.2)));
        game.collect(Item::Part(plating(0.6)));
        game.collect(Item::Surge(surge(Effect::Stat(Stat::Damage, 1.0), 60.0)));
        assert!((game.stats.max_hull - 180.0).abs() < 1e-3);
        game.bodies[0].health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.lives, 2);
        assert!(game.loadout.surges.is_empty());
        assert_eq!(game.loadout.parts.len(), 1);
        assert!((game.stats.max_hull - 120.0).abs() < 1e-3);
        let ship = game.player().unwrap();
        assert!((ship.health - 120.0).abs() < 1e-3 && ship.shield == 60.0);
        let wreck = game
            .pickups
            .iter()
            .find_map(|p| match &p.item {
                Item::Part(part) => Some(part.clone()),
                _ => None,
            })
            .expect("the best part is left behind");
        assert_eq!(wreck.effects, vec![Effect::Stat(Stat::Hull, 0.6)]);
        // Flying back into it restores the ship.
        game.pickups[0].position = game.player().unwrap().position;
        game.step(DT, Input::default());
        assert!((game.stats.max_hull - 180.0).abs() < 1e-3);
    }

    #[test]
    fn far_pickups_unload_with_their_quadrant() {
        let mut game = empty_game();
        game.drop_item(Vec2::new(500.0, 0.0), Vec2::ZERO, Item::Scrap(10));
        set_player(
            &mut game,
            Vec2::new(8.0 * world::QUADRANT_SIZE, 0.0),
            Vec2::ZERO,
        );
        game.step(DT, Input::default());
        assert!(game.pickups.is_empty());
    }

    #[test]
    fn reaching_deeper_gives_better_loot_and_scales_enemies_up() {
        let near = world::threat(world::latent(5, QuadrantId { x: 1, y: 0 }).depth);
        let far = world::threat(world::latent(5, QuadrantId { x: 12, y: 0 }).depth);
        assert!(near < 1.5 && far > 3.5, "{near} {far}");
        for seed in 0..5 {
            for s in world::generate(seed, QuadrantId { x: 12, y: 3 }) {
                if s.kind == BodyKind::Creature {
                    assert!(s.phenotype.threat > 3.5);
                }
            }
        }
    }
}
