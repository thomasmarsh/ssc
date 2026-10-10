//! Drops and the ship's loadout in play. Creatures shed items according to what they are
//! (see `upgrades::Source`), items drift until the ship flies close enough to be pulled in,
//! and what is collected is folded into the ship's `Stats`. A kill's drop is a pure function
//! of the world seed and the spawn it came from, so a route through the universe always
//! pays the same; creatures with no spawn (bred, shards) draw from the loot stream.

use super::upgrades::{self, Install, Slot, Source};
use super::*;
use crate::territory::CivRole;
use crate::world::BaseKind;
use crate::world::hash2;

/// Separates loot from every other stream.
pub(super) const LOOT_SALT: u64 = 0x100D_5A1D_0000_0007;
const MAX_PICKUPS: usize = 96;
/// A pickup is collected when the ship's edge is this close.
const PICKUP_RADIUS: f32 = 12.0;
const MAX_LIVES: u32 = 6;
const MAX_NOTICES: usize = 5;

#[derive(Clone, Debug)]
pub struct Pickup {
    /// Sealed relic provenance, independent of magnet motion.
    pub relic: Option<SectorId>,
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
        Item::Specimen(_) => 120.0,
        Item::Seed(_) => 90.0,
        Item::Material(..) => 60.0,
        _ => 30.0,
    }
}

/// How hard a creature is to put down, as a multiplier on its drop chance (see
/// `tuning::HARD_FLOOR`).
fn hardness(genome: &crate::genome::Genome) -> f32 {
    (tuning::HARD_FLOOR + tuning::HARD_SLOPE * (genome.hull + genome.shield) / tuning::HARD_REF)
        .min(tuning::HARD_CAP)
}

/// The chance that a creature of `genome` drops something when it falls (`bred` is 1 for a
/// generated creature and less for one raised in play).
fn creature_drop_chance(genome: &crate::genome::Genome, bred: f32) -> f32 {
    ((0.08 + 0.4 * genome.bounty / 400.0) / genome.parts() as f32 * bred * hardness(genome))
        .min(tuning::DROP_CHANCE_CAP)
}

/// What a fallen civilization seat pays: (guaranteed parts, extra rolls, a floor on the first
/// part's rarity). `tier` is the fortress tier of its territory (0 to 3), the measure of how
/// well it was defended; `capital` tells the heart from an outpost seat.
fn seat_loot(kind: BaseKind, capital: bool, tier: u8) -> (u32, u32, upgrades::Rarity) {
    let (parts, rolls) = match kind {
        BaseKind::Hive => (0, 2),
        BaseKind::Foundry | BaseKind::Depot => (0, 3),
        BaseKind::Bastion => (1, 2),
        BaseKind::Turret => (0, 0),
    };
    if capital {
        let floor = if tier >= tuning::CAPITAL_EPIC_TIER {
            upgrades::Rarity::Epic
        } else if tier >= 1 {
            upgrades::Rarity::Rare
        } else {
            upgrades::Rarity::Common
        };
        (
            tuning::CAPITAL_PARTS + parts + u32::from(tier),
            rolls + u32::from(tier),
            floor,
        )
    } else {
        let bonus = u32::from(tier >= tuning::OUTPOST_BONUS_TIER);
        (
            1 + parts.min(1) + bonus,
            tuning::OUTPOST_ROLLS + bonus,
            upgrades::Rarity::Common,
        )
    }
}

impl Game {
    /// The territory and role of a civilization's creature or station.
    fn civ_membership(&self, body: &Body) -> Option<(u64, CivRole)> {
        match body.kind {
            BodyKind::Base => body
                .origin
                .and_then(|key| self.civ_bases.get(&key))
                .copied(),
            _ => self.civ_of(body),
        }
    }

    /// The fortress tier (0 to 3) of the territory a body belongs to: how well defended its
    /// civilization is. Zero for anything not civil.
    fn civ_tier_of(&self, body: &Body) -> u8 {
        self.civ_membership(body)
            .and_then(|(tid, _)| self.civ_territories.get(&tid))
            .map_or(0, |t| t.fort_tier())
    }

    /// The threat of the sector the ship is in.
    pub fn threat(&self) -> f32 {
        world::threat(self.params().depth)
    }

    /// How much ship the player has (the bare ship is 1).
    pub fn power(&self) -> f32 {
        self.loadout.power()
    }

    /// Recomputes the stats from the loadout and fits the ship to them: a bigger hull or
    /// shield also arrives filled by the difference, a smaller one is trimmed.
    pub(super) fn refresh_stats(&mut self) {
        self.guide_fitted_unlocks();
        let stats = self.loadout.stats();
        self.stats = stats;
        self.cargo.extra = self.loadout.skills.cargo_bonus(&self.tune);
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

    /// Runs the boosts on their fuel and ages notices.
    pub(super) fn update_loadout(&mut self, dt: f32, input: &Input) {
        self.age_bench_feedback(dt);
        if self.pad.contact.is_some() && !self.bench_open() {
            self.pad.bench = None;
            self.pad.contact = None;
        }
        self.update_boosts(dt, input);
        for notice in &mut self.notices {
            notice.remaining -= dt;
        }
        self.notices.retain(|n| n.remaining > 0.0);
    }

    pub(super) fn notify(&mut self, text: String, rarity: upgrades::Rarity) {
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
            Item::Material(kind, amount) => {
                let taken = self.cargo.add(kind, amount);
                self.score = self.score.saturating_add(amount as u64);
                let full = if taken < amount - 0.5 {
                    " (HOLD FULL)"
                } else {
                    ""
                };
                self.notify(format!("{} +{taken:.0}{full}", kind.label()), rarity);
            }
            Item::Part(part) => {
                self.run.parts += 1;
                let name = part.name.to_uppercase();
                let summary = part.summary();
                let acquired = self.loadout.acquire(part);
                for (profile, gain) in acquired.profiles {
                    let fuel = match gain {
                        upgrades::Gain::New(_) => super::arms::UNLOCK_FUEL,
                        _ => super::arms::UPGRADE_FUEL,
                    };
                    self.grant_profile(profile, gain, fuel, rarity);
                }
                match acquired.installed {
                    None => {}
                    Some(Install::Added) => {
                        self.notify(format!("INSTALLED  {name}  {summary}"), rarity)
                    }
                    Some(Install::Replaced(old)) => self.notify(
                        format!(
                            "INSTALLED  {name}  {summary}  (replaces {})",
                            old.name.to_uppercase()
                        ),
                        rarity,
                    ),
                    Some(Install::Scrapped(part)) => {
                        let value = (part.rating() * 200.0) as u64;
                        self.score = self.score.saturating_add(value);
                        let metal = (part.rating() * 10.0).round();
                        let taken = self.cargo.add(Material::Metal, metal);
                        self.notify(
                            format!("SCRAPPED  {name}  +{value}  METAL +{taken:.0}"),
                            upgrades::Rarity::Common,
                        );
                    }
                }
                self.refresh_stats();
            }
            Item::Surge(surge) => self.charge(surge),
            Item::Specimen(strain) => {
                self.take_strain(strain, "SPECIMEN");
            }
            Item::Seed(kind) => self.gain_seed(kind),
        }
    }

    /// The stream a body's drop is rolled from: fixed by the world for generated spawns.
    fn loot_rng(&mut self, body: &Body) -> Rng {
        match body.origin {
            Some((sector, index)) => {
                let key = (u64::from(index) << 8 | u64::from(body.part))
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15);
                Rng::new(hash2(self.seed ^ LOOT_SALT ^ key, sector.x, sector.y))
            }
            None => Rng::new(self.loot.next_u64()),
        }
    }

    fn creature_reward(&self, body: &Body, rng: &mut Rng, source: &Source) -> Item {
        if self.civ_membership(body).is_some() {
            return upgrades::roll_item(rng, source);
        }
        let material = match rng.next_u64() % 4 {
            0 => Material::Metal,
            1 => Material::Volatiles,
            2 => Material::Crystal,
            _ => Material::Biomass,
        };
        Item::Material(material, 3.0 + body.genes.threat.sqrt().min(9.0))
    }

    /// Rolls and scatters whatever a destroyed body leaves behind.
    pub(super) fn drop_loot(&mut self, body: &Body) {
        let mut rng = self.loot_rng(body);
        let params = world::latent(self.seed, SectorId::containing(body.position));
        let mut drops: Vec<Item> = Vec::new();
        match body.kind {
            BodyKind::Creature => {
                let genome = &body.genome;
                // Better bounties drop more; a jointed creature shares one drop's worth among
                // its parts, and creatures bred by bases are poor farming.
                if self.is_elder(body) && self.civ_membership(body).is_some() {
                    // A boss always pays: two parts (the first epic, the second at least
                    // rare) and two lucky rolls, graded a notch above the place, for any
                    // elder that was generated (raised ones are not farmable bosses).
                    let mut source = Source::of_creature(genome, body.genes.threat * 1.25, params);
                    source.bias = 1.0;
                    source.min_rarity = upgrades::Rarity::Epic;
                    drops.push(Item::Part(upgrades::roll_part(&mut rng, &source)));
                    source.min_rarity = upgrades::Rarity::Rare;
                    drops.push(Item::Part(upgrades::roll_part(&mut rng, &source)));
                    // A well defended civilization's elder is worth more.
                    if self.civ_tier_of(body) >= tuning::ELDER_BONUS_TIER {
                        drops.push(Item::Part(upgrades::roll_part(&mut rng, &source)));
                    }
                    for _ in 0..2 {
                        drops.push(self.creature_reward(body, &mut rng, &source));
                    }
                }
                drops.extend(self.apex_loot(body, &mut rng, params));
                let bred = if body.origin.is_some() { 1.0 } else { 0.35 };
                if rng.chance(creature_drop_chance(genome, bred)) {
                    let source = Source::of_creature(genome, body.genes.threat, params);
                    drops.push(self.creature_reward(body, &mut rng, &source));
                }
                // A carrier of a rare power is worth a little more: one extra, slightly
                // luckier roll (drawn last, so every other drop of this body is unchanged).
                if self.carrier_bonus(body).is_some() && rng.chance(crate::power::EXTRA_DROP_CHANCE)
                {
                    let mut source = Source::of_creature(genome, body.genes.threat, params);
                    source.bias = crate::power::EXTRA_DROP_LUCK;
                    drops.push(self.creature_reward(body, &mut rng, &source));
                }
                // A special carrier's first kill may leave a specimen of its organ (drawn after
                // everything else, so no other drop of this body moves).
                if body.origin.is_some()
                    && let Some(organ) = genome
                        .live_powers()
                        .filter_map(|c| {
                            organs::Organ::from_power(c.power).map(|organ| (c.strength, organ))
                        })
                        .max_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, organ)| organ)
                    && rng.chance(tuning::HARVEST_CHANCE)
                {
                    drops.push(Item::Specimen(organs::Strain::from_donor(organ, genome)));
                }
                // A grazer's gut may hold seeds of what it eats (drawn after everything else,
                // so no other drop of this body moves).
                drops.extend(self.gut_seed(body, &mut rng));
            }
            // Geological rocks pay only their finite remaining lode through shattering/mining.
            // They never contain technological charges or bonus material beyond that budget.
            BodyKind::Asteroid => {}
            BodyKind::Base => {
                // A civilization's seat pays out by how well it was defended (`seat_loot`),
                // shaped by what the station was. A wall turret leaves scrap metal and now and
                // then a lucky find, no part.
                let mut source = Source::plain(body.genes.threat, params);
                source.bias = 1.0;
                let kind = body.base.as_ref().map_or(BaseKind::Hive, |b| b.kind);
                match kind {
                    BaseKind::Hive => source.affinity[Slot::Core.index()] += 2.0,
                    BaseKind::Foundry => {
                        source.affinity[Slot::Plating.index()] += 2.0;
                        source.affinity[Slot::Engine.index()] += 2.0;
                    }
                    BaseKind::Bastion => source.affinity[Slot::Cannon.index()] += 3.0,
                    BaseKind::Depot => source.affinity[Slot::Aux.index()] += 3.0,
                    BaseKind::Turret => {
                        let scrap = (12.0 * body.genes.threat).round().max(5.0);
                        drops.push(Item::Material(Material::Metal, scrap));
                    }
                }
                if let Some((weapon, _)) = body.base.as_ref().and_then(|b| b.arms) {
                    source.weapon = weapon;
                }
                if kind == BaseKind::Turret {
                    if rng.chance(0.3) {
                        drops.push(upgrades::roll_item(&mut rng, &source));
                    }
                } else {
                    let capital = matches!(self.civ_membership(body), Some((_, CivRole::Capital)));
                    let (parts, rolls, floor) = seat_loot(kind, capital, self.civ_tier_of(body));
                    for k in 0..parts {
                        // The heart's first part is the prize: at least `floor` rare.
                        source.min_rarity = if k == 0 {
                            floor
                        } else {
                            upgrades::Rarity::Common
                        };
                        drops.push(Item::Part(upgrades::roll_part(&mut rng, &source)));
                    }
                    source.min_rarity = upgrades::Rarity::Common;
                    for _ in 0..rolls {
                        drops.push(upgrades::roll_item(&mut rng, &source));
                    }
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
            relic: None,
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

    /// A wrecked ship loses part of its hold and its best part, which is left floating where
    /// it died for the respawned ship to recover. The arsenal stays: owned weapon profiles
    /// (and their levels) and owned boosts are never lost, only the fuel in the hold is at
    /// risk (see `shed_cargo`). Boosts stop; nothing runs through the wreck.
    pub(super) fn shed_on_death(&mut self, position: Vec2, insured: bool) {
        self.loadout.arsenal.stop_boosts();
        self.shed_cargo(position);
        if insured && let Some(best) = self.loadout.best_part() {
            let name = self.loadout.parts[best].name.to_uppercase();
            let rarity = self.loadout.parts[best].rarity;
            self.notify(format!("INSURED  {name} kept"), rarity);
        } else if let Some(best) = self.loadout.best_part() {
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
        let magnet = self.stats.magnet + self.loadout.skills.magnet_bonus(&self.tune);
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
                    let toward = offset / distance.max(1.0);
                    // A sideways swirl that dies out near the ship, so salvage arcs in
                    // instead of sliding straight. It keeps the side it is already moving on.
                    let side = if toward.perp_dot(pickup.velocity) < 0.0 {
                        -1.0
                    } else {
                        1.0
                    };
                    let swirl = side * 0.5 * pull * (distance / reach).powi(2);
                    pickup.velocity += (toward + toward.perp() * swirl / pull) * pull * dt;
                    pickup.velocity = pickup.velocity.clamp_length_max(1100.0);
                }
            }
            pickup.position += pickup.velocity * dt;
        }
        let mut items = Vec::new();
        for &index in taken.iter().rev() {
            let pickup = self.pickups.remove(index);
            if let Some(id) = pickup.relic {
                self.relics_taken.insert(id);
            }
            items.push(pickup.item);
        }
        self.pickups.retain(|p| p.remaining > 0.0);
        for item in items.into_iter().rev() {
            self.collect(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::arsenal::{Need, Profile};
    use super::*;
    use crate::genome::Species;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    use upgrades::{Effect, Part as ShipPart, Rarity, Slot, Stat, Surge, Trait, test_surge};

    fn plating(bonus: f32) -> ShipPart {
        ShipPart {
            name: "Test Plate".into(),
            slot: Slot::Plating,
            rarity: Rarity::Rare,
            grade: 1.0,
            stem: String::new(),
            core: usize::MAX,
            effects: vec![Effect::Stat(Stat::Hull, bonus)],
        }
    }

    /// A pickup carrying one effect, with 100 fuel (see `upgrades::test_surge`).
    fn surge(effect: Effect) -> Surge {
        test_surge(effect)
    }

    fn boost(effect: Effect, need: Need) -> Item {
        Item::Surge(Surge {
            need,
            ..surge(effect)
        })
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
        assert_eq!(game.notices.len(), 2);
        assert!(game.notices[0].text.starts_with("INSTALLED"));
        assert!(
            game.notices[1]
                .text
                .starts_with("PARRY: purchase available at the bench")
        );
        assert!(!game.parry_unlocked());
    }

    #[test]
    fn a_boost_runs_on_fuel_only_while_needed_and_stops_when_dry() {
        let mut game = empty_game();
        let overdrive = Surge {
            fuel: 3.0,
            drain: 2.0,
            ..surge(Effect::Stat(Stat::FireRate, 1.0))
        };
        game.collect(Item::Surge(overdrive));
        assert_eq!(game.cargo.fuel, 3.0);
        // Idle: nothing runs, nothing burns.
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.stats.fire_period, Stats::BASE.fire_period);
        assert_eq!(game.cargo.fuel, 3.0);
        // Firing wakes it and it burns 2 a second.
        for _ in 0..30 {
            game.step(DT, fire());
        }
        assert!(game.stats.fire_period < 0.09);
        assert!((game.cargo.fuel - 2.0).abs() < 0.05, "{}", game.cargo.fuel);
        // The fuel runs out about a second later, and the ship is back to base.
        for _ in 0..90 {
            game.step(DT, fire());
        }
        assert_eq!(game.stats.fire_period, Stats::BASE.fire_period);
        assert!(game.cargo.fuel < 0.05);
        assert!(game.loadout.arsenal.boosts[0].dry);
        assert!(game.notices.iter().any(|n| n.text.contains("OUT OF FUEL")));
        // It stays owned: fuel makes it run again, and the master switch stops it.
        game.collect(Item::Material(Material::Fuel, 20.0));
        game.step(DT, fire());
        assert!(game.stats.fire_period < 0.09);
        game.toggle_boosts();
        game.step(DT, fire());
        assert_eq!(game.stats.fire_period, Stats::BASE.fire_period);
        game.toggle_boosts();
        game.step(DT, fire());
        assert!(game.stats.fire_period < 0.09);
    }

    #[test]
    fn spread_fires_a_fan_and_each_shot_is_weaker() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Spread, 2))));
        game.step(DT, fire());
        assert_eq!(game.bullets.len(), 5);
        let center = game.bullets.iter().map(|b| b.damage).fold(0.0, f32::max);
        assert_eq!(center, Stats::BASE.damage);
        assert!(game.bullets.iter().any(|b| b.damage < center));
    }

    #[test]
    fn broadside_and_tail_guns_fire_sideways_and_back() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Broadside, 1))));
        game.step(DT, fire());
        // Broadside: the nose gun and one gun off each flank.
        assert_eq!(game.bullets.len(), 3);
        let forward = Vec2::from_angle(game.player().unwrap().angle);
        let dots = |game: &Game| -> Vec<f32> {
            game.bullets
                .iter()
                .map(|b| b.velocity.normalize().dot(forward))
                .collect()
        };
        assert_eq!(dots(&game).iter().filter(|d| d.abs() < 0.2).count(), 2);
        // The tail gun is another profile: finding it switches over to it.
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Tailgun, 1))));
        game.step(DT, fire());
        assert_eq!(game.bullets.len(), 2);
        let d = dots(&game);
        assert!(d.iter().any(|d| *d > 0.9) && d.iter().any(|d| *d < -0.9));
    }

    #[test]
    fn damage_and_fire_rate_follow_the_stats() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Stat(Stat::Damage, 1.0))));
        game.step(DT, fire());
        assert_eq!(game.bullets[0].damage, 52.0);
        assert!(game.cargo.fuel < 100.0);
    }

    #[test]
    fn piercing_shots_pass_through_a_line_of_targets_once_each() {
        let mut game = empty_game();
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Pierce, 2))));
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
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Blast, 1))));
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
                game.collect(Item::Surge(surge(Effect::Trait(Trait::Homing, level))));
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
        armored.collect(boost(Effect::Stat(Stat::Armor, 1.0), Need::Danger));
        for game in [&mut plain, &mut armored] {
            // One quiet step wakes the armor (a hit this recent counts as danger).
            game.bodies[0].since_hit = 0.0;
            game.step(DT, Input::default());
            game.player_invulnerability = 0.0;
            let ship = &mut game.bodies[0];
            damage(ship, 40.0, 0.0, &DEFAULT_TUNING);
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
            damage(creature, 30.0, 0.0, &DEFAULT_TUNING);
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
        game.collect(boost(Effect::Trait(Trait::Ballast, 1), Need::Wells));
        let hull = game.player().unwrap().health;
        for _ in 0..30 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.player().unwrap().health, hull);

        let mut game = empty_game();
        game.collect(boost(Effect::Trait(Trait::Shears, 1), Need::Cords));
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
        game.collect(boost(Effect::Trait(Trait::Aura, 2), Need::Firing));
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(30.0, 0.0));
        let hull = game.player().unwrap().health;
        game.step(DT, fire());
        assert!(body(&game, rock).velocity.length() > 200.0, "not flung");
        assert_eq!(game.player().unwrap().health, hull, "contact hurt the ship");

        let mut game = empty_game();
        game.collect(boost(Effect::Trait(Trait::Ram, 2), Need::Firing));
        let rock = add(&mut game, BodyKind::Asteroid, Vec2::new(30.0, 0.0));
        set_player(&mut game, Vec2::ZERO, Vec2::new(300.0, 0.0));
        let before = body(&game, rock).health;
        game.step(DT, fire());
        // Free rocks shrug off the ship's weapons, rams included (see `tuning`).
        assert!(body(&game, rock).health < before - 0.5);
    }

    #[test]
    fn pickups_are_drawn_in_and_taken_and_expire_when_ignored() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        game.drop_item(Vec2::new(100.0, 0.0), Vec2::ZERO, Item::Repair(10.0));
        game.drop_item(
            Vec2::new(3000.0, 0.0),
            Vec2::ZERO,
            Item::Material(Material::Metal, 50.0),
        );
        let score = game.score;
        for _ in 0..90 {
            game.step(DT, Input::default());
        }
        assert_eq!(game.pickups.len(), 1, "near one should be taken");
        assert_eq!(game.score, score);
        for _ in 0..60 * 61 {
            game.step(DT, Input::default());
        }
        assert!(game.pickups.is_empty(), "unclaimed salvage fades");
        // A bigger magnet reaches farther.
        let mut game = empty_game();
        game.collect(boost(Effect::Stat(Stat::Magnet, 3.0), Need::Loot));
        game.drop_item(
            Vec2::new(450.0, 0.0),
            Vec2::ZERO,
            Item::Material(Material::Metal, 50.0),
        );
        for _ in 0..90 {
            game.step(DT, Input::default());
        }
        assert!(game.pickups.is_empty());
    }

    #[test]
    fn pickups_arc_into_the_ship_instead_of_sliding_straight() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        let ship = game.player().unwrap().position;
        game.drop_item(
            ship + Vec2::new(120.0, 0.0),
            Vec2::ZERO,
            Item::Material(Material::Metal, 50.0),
        );
        let mut widest = 0.0_f32;
        for _ in 0..180 {
            if let Some(p) = game.pickups.first() {
                widest = widest.max((p.position.y - ship.y).abs());
            }
            game.step(DT, Input::default());
        }
        assert!(widest > 5.0, "path stayed straight: {widest}");
        assert!(game.pickups.is_empty(), "an arcing pickup is still taken");
    }

    #[test]
    fn extra_lives_are_capped_and_salvage_scores() {
        let mut game = empty_game();
        for _ in 0..10 {
            game.collect(Item::Life);
        }
        assert_eq!(game.lives, MAX_LIVES);
        game.collect(Item::Material(Material::Metal, 75.0));
        assert_eq!(game.score, 75);
        assert_eq!(game.cargo.metal, 75.0);
    }

    #[test]
    fn killing_creatures_yields_drops_that_are_the_same_every_time() {
        let kills = |seed: u64| {
            let mut game = Game::new(seed);
            game.player_invulnerability = 1e9;
            // HOME is empty: stand in a sector rich in creatures.
            let rich = crate::simulation::tests::find_sector(seed, |s| {
                s.iter().filter(|s| s.species.is_some()).count() > 20
            });
            game.teleport(rich.center());
            game.step(DT, Input::default());
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
        // (An unaffiliated seat pays like an undefended outpost: a part and a roll.)
        assert!(game.pickups.len() >= 2);
    }

    #[test]
    fn dying_costs_the_best_part_but_never_the_arsenal() {
        let mut game = empty_game();
        game.collect(Item::Part(plating(0.2)));
        game.collect(Item::Part(plating(0.6)));
        game.collect(Item::Surge(surge(Effect::Trait(Trait::Spread, 2))));
        game.collect(boost(Effect::Stat(Stat::Damage, 1.0), Need::Firing));
        assert!((game.stats.max_hull - 180.0).abs() < 1e-3);
        game.step(DT, fire());
        assert!(game.loadout.arsenal.boosts[0].running);
        game.bodies[0].health = 0.0;
        game.step(DT, Input::default());
        assert_eq!(game.lives, 2);
        // Owned profiles and boosts survive; only the running state is cut.
        let arsenal = &game.loadout.arsenal;
        assert_eq!(arsenal.level(Profile::Spread), 2);
        assert_eq!(arsenal.active, Profile::Spread);
        assert_eq!(arsenal.boosts.len(), 1);
        assert!(!arsenal.boosts[0].running);
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
        let at = game.player().unwrap().position;
        let wreck_at = game
            .pickups
            .iter()
            .position(|p| matches!(p.item, Item::Part(_)))
            .unwrap();
        game.pickups[wreck_at].position = at;
        game.step(DT, Input::default());
        assert!((game.stats.max_hull - 180.0).abs() < 1e-3);
    }

    #[test]
    fn far_pickups_unload_with_their_sector() {
        let mut game = empty_game();
        game.drop_item(
            Vec2::new(500.0, 0.0),
            Vec2::ZERO,
            Item::Material(Material::Metal, 10.0),
        );
        set_player(
            &mut game,
            Vec2::new(8.0 * world::SECTOR_SIZE, 0.0),
            Vec2::ZERO,
        );
        game.step(DT, Input::default());
        assert!(game.pickups.is_empty());
    }

    #[test]
    fn reaching_deeper_gives_better_loot_and_scales_enemies_up() {
        let near = world::threat(world::latent(5, SectorId { x: 1, y: 0 }).depth);
        let far = world::threat(world::latent(5, SectorId { x: 12, y: 0 }).depth);
        assert!(near < 1.5 && far > 3.5, "{near} {far}");
        for seed in 0..5 {
            for s in world::generate(seed, SectorId { x: 12, y: 3 }) {
                if s.kind == BodyKind::Creature {
                    assert!(s.phenotype.threat > 3.5);
                }
            }
        }
    }
}

#[cfg(test)]
mod supply_tests {
    use super::*;
    use crate::genome::Genome;
    use crate::world::*;

    #[test]
    fn hard_creatures_drop_more_and_frail_ones_less() {
        let frail = Genome {
            hull: 20.0,
            shield: 0.0,
            ..Genome::bogey()
        };
        let tough = Genome {
            hull: 200.0,
            shield: 60.0,
            ..Genome::bogey()
        };
        assert!(hardness(&frail) < 1.0 && hardness(&tough) > 2.0);
        assert!(creature_drop_chance(&tough, 1.0) > 2.0 * creature_drop_chance(&frail, 1.0));
        let huge = Genome {
            hull: 5000.0,
            ..Genome::bogey()
        };
        assert!(creature_drop_chance(&huge, 1.0) <= tuning::DROP_CHANCE_CAP);
        assert!(hardness(&huge) <= tuning::HARD_CAP);
    }

    #[test]
    fn a_seats_drops_scale_with_its_defense_and_the_heart_pays_most() {
        for kind in [
            BaseKind::Hive,
            BaseKind::Foundry,
            BaseKind::Bastion,
            BaseKind::Depot,
        ] {
            let mut last = (0, 0);
            for tier in 0..=3 {
                let (parts, rolls, _) = seat_loot(kind, true, tier);
                assert!(parts >= last.0 && rolls >= last.1);
                last = (parts, rolls);
                let (outpost_parts, outpost_rolls, _) = seat_loot(kind, false, tier);
                assert!(parts > outpost_parts && rolls >= outpost_rolls);
            }
            assert!(seat_loot(kind, true, 3).0 >= 5);
            assert_eq!(seat_loot(kind, true, 0).2, upgrades::Rarity::Common);
            assert_eq!(seat_loot(kind, true, 1).2, upgrades::Rarity::Rare);
            assert_eq!(seat_loot(kind, true, 3).2, upgrades::Rarity::Epic);
            // An undefended outpost pays about what one base used to, never a farm.
            assert!(seat_loot(kind, false, 0).0 <= 2);
        }
    }

    #[test]
    fn wild_carriers_and_geology_never_supply_technology() {
        let mut game = Game::new(42);
        for x in 1..=12 {
            game.teleport((SectorId { x, y: 4 }).center());
            game.step(1.0 / 60.0, Input::default());
            let bodies: Vec<_> = game
                .bodies
                .iter()
                .filter(|b| {
                    (b.kind == BodyKind::Creature && game.civ_membership(b).is_none())
                        || b.kind == BodyKind::Asteroid
                })
                .cloned()
                .collect();
            for body in bodies {
                game.pickups.clear();
                game.drop_loot(&body);
                assert!(game.pickups.iter().all(|p| matches!(
                    p.item,
                    Item::Material(
                        Material::Metal
                            | Material::Volatiles
                            | Material::Crystal
                            | Material::Biomass,
                        _
                    ) | Item::Specimen(_)
                        | Item::Seed(_)
                )));
            }
        }
    }
}
