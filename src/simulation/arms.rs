//! The arsenal in play: switching profiles, paying for shots out of the cargo hold, falling
//! back to the free stock gun when a profile runs dry, and the boosts that burn fuel while
//! they are in use. The data (profiles, levels, boosts) lives in `arsenal`; this is the
//! part that touches the ship, the hold and the clock.
//!
//! Model, in short:
//! - profiles are owned for the run and only ever gain levels; death and pickups never
//!   take one away (game over resets the whole run);
//! - only the active profile is in force, and it is billed in a material when it fires;
//! - a dry profile does not stop the guns: the first dry pull swaps back to the previous
//!   usable profile (or stock) with a cue, and the same shot still goes out.

use super::arsenal::{Billing, BoostGain, Gain, Need, Profile};
use super::upgrades::{Charged, Rarity, Surge};
use super::*;

/// Shortest gap between two switches.
const SWITCH_DEBOUNCE: f32 = 0.08;
/// Seconds the HUD announces a switch.
const FLASH: f32 = 1.6;
/// Fuel that comes with unlocking a profile from a part, and with a repeat find.
pub const UNLOCK_FUEL: f32 = 40.0;
pub const UPGRADE_FUEL: f32 = 25.0;
/// How close hostiles, loot and wells must be to wake the boosts that watch for them.
const DANGER_RANGE: f32 = 800.0;
const LOOT_RANGE: f32 = 900.0;
const WELL_RANGE: f32 = 800.0;
/// A hit this recent counts as danger.
const RECENT_HIT: f32 = 3.0;

impl Game {
    /// Whether a profile could fire right now (the stock gun always can).
    pub fn usable(&self, profile: Profile) -> bool {
        match profile.material() {
            None => true,
            Some(material) => self
                .cargo
                .can_afford(&[(material, profile.cost(self.loadout.arsenal.level(profile)))]),
        }
    }

    /// Next (`step` > 0) or previous owned profile, wrapping. Landing on an empty one is
    /// allowed; it shows as dry and falls back when fired.
    pub fn switch_weapon(&mut self, step: i32) -> bool {
        if self.switch_clock > 0.0 || self.game_over {
            return false;
        }
        match self.loadout.arsenal.cycle(step) {
            Some(profile) => {
                self.switched(profile);
                true
            }
            None => false,
        }
    }

    /// Selects the `n`th owned profile directly (0 is the stock gun).
    pub fn select_weapon(&mut self, n: usize) -> bool {
        if self.switch_clock > 0.0 || self.game_over {
            return false;
        }
        match self.loadout.arsenal.select(n) {
            Some(profile) => {
                self.switched(profile);
                true
            }
            None => false,
        }
    }

    /// Master switch for the boosts, to save fuel.
    pub fn toggle_boosts(&mut self) {
        let arsenal = &mut self.loadout.arsenal;
        arsenal.boosts_on = !arsenal.boosts_on;
        let on = arsenal.boosts_on;
        if !on {
            arsenal.stop_boosts();
        }
        self.notify(
            if on { "BOOSTS ON" } else { "BOOSTS OFF" }.into(),
            Rarity::Common,
        );
        self.arsenal_flash = FLASH;
        self.refresh_stats();
    }

    fn switched(&mut self, profile: Profile) {
        self.switch_clock = SWITCH_DEBOUNCE;
        self.arsenal_flash = FLASH;
        let dry = !self.usable(profile);
        self.cue(Cue::Switch { dry });
        self.refresh_stats();
    }

    /// Bills a trigger pull of the main gun to the active profile; if it cannot be paid,
    /// falls back first, so the shot that follows is always fired.
    pub(super) fn pay_volley(&mut self) {
        self.pay(Billing::Volley);
    }

    /// Bills a launch of the active profile's own launcher. False means it was dry, the
    /// profile has just been swapped and nothing should launch this step.
    pub(super) fn pay_launch(&mut self) -> bool {
        self.pay(Billing::Launch)
    }

    fn pay(&mut self, billing: Billing) -> bool {
        let arsenal = &self.loadout.arsenal;
        let profile = arsenal.active;
        let Some(material) = profile.material().filter(|_| profile.billing() == billing) else {
            return true;
        };
        let cost = profile.cost(arsenal.level(profile));
        if self.cargo.spend(&[(material, cost)]) {
            return true;
        }
        self.run_dry(profile, material);
        false
    }

    /// The active profile is out of fuel: back to the previous profile if it can fire,
    /// otherwise to stock.
    fn run_dry(&mut self, dried: Profile, material: Material) {
        let arsenal = &self.loadout.arsenal;
        let previous = arsenal.previous;
        let back = if previous != dried && arsenal.owns(previous) && self.usable(previous) {
            previous
        } else {
            Profile::Stock
        };
        self.loadout.arsenal.active = back;
        self.loadout.arsenal.previous = dried;
        self.arsenal_flash = FLASH;
        self.cue(Cue::Dry);
        self.notify(
            format!(
                "DRY  {} EMPTY  - back to {}",
                material.label().to_uppercase(),
                back.label()
            ),
            Rarity::Epic,
        );
        self.refresh_stats();
    }

    /// Records a weapon profile unlocked or leveled by a pickup: fuel comes aboard, a new
    /// profile is equipped at once, and the player is told.
    pub(super) fn grant_profile(
        &mut self,
        profile: Profile,
        gain: Gain,
        fuel: f32,
        rarity: Rarity,
    ) {
        let taken = profile
            .material()
            .map_or(0.0, |material| self.cargo.add(material, fuel));
        let fuel_note = match profile.material() {
            Some(material) if taken >= 0.5 => {
                format!("  +{taken:.0} {}", material.label().to_uppercase())
            }
            _ => String::new(),
        };
        match gain {
            Gain::New(level) => {
                self.run.weapons += 1;
                self.notify(
                    format!(
                        "ARMED  {} {level}  {}{fuel_note}   [ ] to switch",
                        profile.label(),
                        profile.summary()
                    ),
                    rarity,
                );
                if self.loadout.arsenal.set_active(profile) {
                    self.arsenal_flash = FLASH;
                    self.cue(Cue::Switch { dry: false });
                }
            }
            Gain::Upgraded { from, to } => self.notify(
                format!("UPGRADED  {} {from} -> {to}{fuel_note}", profile.label()),
                rarity,
            ),
            Gain::Maxed => self.notify(
                format!("{} AT MAX{fuel_note}", profile.label()),
                Rarity::Common,
            ),
        }
    }

    /// Takes a charged pickup aboard: levels up weapon profiles or gains a boost, and in
    /// either case fills the hold with the fuel it brought.
    pub(super) fn charge(&mut self, surge: Surge) {
        let rarity = surge.rarity;
        match self.loadout.charge(&surge) {
            Charged::Profiles(gains) => {
                for (profile, gain) in gains {
                    self.grant_profile(profile, gain, surge.fuel, rarity);
                }
            }
            Charged::Boost(gain) => {
                let taken = self.cargo.add(surge.material, surge.fuel);
                let what = match gain {
                    BoostGain::New => "BOOST",
                    BoostGain::Upgraded => "BOOST UPGRADED",
                    BoostGain::Same => "BOOST REFUELED",
                };
                self.notify(
                    format!(
                        "{what}  {}  {}  burns {} while {}  +{taken:.0} {}",
                        surge.name.to_uppercase(),
                        surge.summary(),
                        surge.material.label().to_uppercase(),
                        surge.need.label(),
                        surge.material.label().to_uppercase(),
                    ),
                    rarity,
                );
            }
        }
        self.refresh_stats();
    }

    /// Runs the boosts: each wakes while its condition holds and burns fuel by the second;
    /// with none left it stops and shows as dry until fuel comes aboard.
    pub(super) fn update_boosts(&mut self, dt: f32, input: &Input) {
        self.arsenal_flash = (self.arsenal_flash - dt).max(0.0);
        self.switch_clock = (self.switch_clock - dt).max(0.0);
        if self.loadout.arsenal.boosts.is_empty() {
            return;
        }
        let on = self.loadout.arsenal.boosts_on;
        let needs: Vec<Need> = self.loadout.arsenal.boosts.iter().map(|b| b.need).collect();
        let awake: Vec<bool> = needs
            .iter()
            .map(|&need| on && self.need_holds(need, input))
            .collect();
        let mut changed = false;
        let mut spent_out = Vec::new();
        for (index, wanted) in awake.into_iter().enumerate() {
            let (material, drain) = {
                let boost = &self.loadout.arsenal.boosts[index];
                (boost.material, boost.drain)
            };
            let (running, dry) = if !wanted {
                (false, false)
            } else if self.cargo.spend(&[(material, drain * dt)]) {
                (true, false)
            } else {
                (false, true)
            };
            let boost = &mut self.loadout.arsenal.boosts[index];
            changed |= boost.running != running;
            if dry && !boost.dry {
                spent_out.push(format!(
                    "{}  OUT OF {}",
                    boost.name.to_uppercase(),
                    material.label().to_uppercase()
                ));
            }
            boost.running = running;
            boost.dry = dry;
        }
        for text in spent_out {
            self.cue(Cue::Dry);
            self.notify(text, Rarity::Epic);
        }
        if changed {
            self.refresh_stats();
        }
    }

    fn need_holds(&self, need: Need, input: &Input) -> bool {
        let Some(ship) = self.player() else {
            return false;
        };
        let near = |range: f32, wanted: &dyn Fn(&Body) -> bool| {
            self.bodies
                .iter()
                .any(|b| b.active && wanted(b) && b.position.distance(ship.position) < range)
        };
        match need {
            Need::Firing => input.fire,
            Need::Thrusting => {
                input.thrust > 0.1 || input.move_direction.is_some_and(|m| m.length() > 0.1)
            }
            Need::Danger => {
                ship.since_hit < RECENT_HIT
                    || near(DANGER_RANGE, &|b| {
                        matches!(b.kind, BodyKind::Creature | BodyKind::Base)
                    })
            }
            Need::Loot => self
                .pickups
                .iter()
                .any(|p| p.position.distance(ship.position) < LOOT_RANGE),
            Need::Wells => near(WELL_RANGE, &|b| b.kind == BodyKind::BlackHole),
            Need::Cords => self
                .tethers
                .iter()
                .any(|t| t.kind == TetherKind::Latch && t.attached()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Species;
    use crate::simulation::tests::{DT, empty_game, spawn};
    use crate::simulation::upgrades::{
        self, Effect, Item, Part, Slot, Source, Stat, Trait, test_surge,
    };
    use crate::world::SectorParams;

    fn fire() -> Input {
        Input {
            fire: true,
            ..Default::default()
        }
    }

    fn part(effects: Vec<Effect>) -> Part {
        Part {
            name: "Test Part".into(),
            slot: Slot::Cannon,
            rarity: Rarity::Common,
            grade: 1.0,
            effects,
            stem: String::new(),
            core: usize::MAX,
        }
    }

    /// Lets the debounce lapse.
    fn settle(game: &mut Game) {
        for _ in 0..8 {
            game.step(DT, Input::default());
        }
    }

    #[test]
    fn a_weapon_part_unlocks_its_profile_for_good_and_repeats_level_it() {
        let mut game = empty_game();
        game.collect(Item::Part(part(vec![Effect::Trait(Trait::Missiles, 1)])));
        let arsenal = &game.loadout.arsenal;
        assert_eq!(arsenal.level(Profile::Missiles), 1);
        assert_eq!(
            arsenal.active,
            Profile::Missiles,
            "a new weapon is equipped"
        );
        assert!(
            game.loadout.parts.is_empty(),
            "weapon-only parts take no slot"
        );
        assert_eq!(game.cargo.volatiles, UNLOCK_FUEL);
        game.collect(Item::Part(part(vec![Effect::Trait(Trait::Missiles, 1)])));
        assert_eq!(game.loadout.arsenal.level(Profile::Missiles), 2);
        assert_eq!(game.cargo.volatiles, UNLOCK_FUEL + UPGRADE_FUEL);
        // A weaker find never lowers it, and a maxed one only brings fuel.
        for _ in 0..4 {
            game.collect(Item::Part(part(vec![Effect::Trait(Trait::Missiles, 1)])));
        }
        assert_eq!(game.loadout.arsenal.level(Profile::Missiles), 3);
        // Mixed parts: the weapon trait is harvested, the stat stays bolted on, and the
        // price that came with the trait (negative stats) is dropped with it.
        game.collect(Item::Part(part(vec![
            Effect::Trait(Trait::Blast, 1),
            Effect::Stat(Stat::Damage, -0.05),
            Effect::Stat(Stat::ShotSpeed, 0.1),
        ])));
        assert_eq!(game.loadout.arsenal.level(Profile::Blast), 1);
        assert_eq!(game.loadout.parts.len(), 1);
        assert_eq!(
            game.loadout.parts[0].effects,
            vec![Effect::Stat(Stat::ShotSpeed, 0.1)]
        );
        assert_eq!(game.stats.damage, Stats::BASE.damage);
    }

    #[test]
    fn nothing_found_ever_weakens_the_ship() {
        // Property over random pickups of every kind at every depth, with deaths between:
        // profile levels never fall, nothing owned is lost, stock firepower never drops.
        for seed in 0..6 {
            let mut rng = Rng::new(seed);
            let mut game = empty_game();
            game.lives = 100;
            let mut levels = [0_u8; Profile::ALL.len()];
            let mut firepower = 1.0_f32;
            let mut boosts = 0;
            for round in 0..240 {
                let source = Source::plain(1.0 + rng.f32() * 6.0, SectorParams::HOME);
                let item = match rng.int(0, 2) {
                    0 => Item::Surge(upgrades::roll_surge(&mut rng, &source)),
                    _ => Item::Part(upgrades::roll_part(&mut rng, &source)),
                };
                game.collect(item);
                if round % 40 == 39 {
                    game.player_invulnerability = 0.0;
                    game.bodies[0].health = 0.0;
                    game.step(DT, Input::default());
                }
                for profile in Profile::ALL {
                    let level = game.loadout.arsenal.level(profile);
                    assert!(level >= levels[profile.index()], "{profile:?} fell");
                    levels[profile.index()] = level;
                }
                assert!(game.loadout.arsenal.boosts.len() >= boosts);
                boosts = game.loadout.arsenal.boosts.len();
                // Dying sheds the best part, so firepower is compared only between pickups.
                let now = game.loadout.gear_stats().firepower();
                if round % 40 != 39 {
                    assert!(
                        now + 1e-4 >= firepower,
                        "firepower fell {firepower} -> {now}"
                    );
                }
                firepower = now;
                assert!(game.loadout.arsenal.owns(game.loadout.arsenal.active));
            }
            assert!(
                levels.iter().filter(|&&l| l > 0).count() > 4,
                "too few finds"
            );
        }
    }

    #[test]
    fn a_better_gun_part_never_replaces_firepower_with_less() {
        let mut loadout = upgrades::Loadout::default();
        let gun = |name: &str, effects: Vec<Effect>| Part {
            name: name.into(),
            ..part(effects)
        };
        loadout.acquire(gun("rapid", vec![Effect::Stat(Stat::FireRate, 0.5)]));
        loadout.acquire(gun("rapid2", vec![Effect::Stat(Stat::FireRate, 0.5)]));
        loadout.acquire(gun("rapid3", vec![Effect::Stat(Stat::FireRate, 0.5)]));
        let before = loadout.gear_stats().firepower();
        // Rated higher by its raw numbers but it would cost rate of fire overall.
        let outcome = loadout.acquire(gun(
            "odd",
            vec![
                Effect::Stat(Stat::Damage, 0.1),
                Effect::Stat(Stat::FireRate, -0.6),
            ],
        ));
        assert!(
            loadout.gear_stats().firepower() >= before - 1e-4,
            "{outcome:?}"
        );
    }

    #[test]
    fn switching_cycles_wraps_and_is_debounced() {
        let mut game = empty_game();
        assert!(
            !game.switch_weapon(1),
            "only the stock gun: nothing to switch to"
        );
        game.collect(Item::Surge(test_surge(Effect::Trait(Trait::Spread, 1))));
        game.collect(Item::Surge(test_surge(Effect::Trait(Trait::Needles, 1))));
        assert_eq!(game.loadout.arsenal.active, Profile::Needles);
        settle(&mut game);
        let active = |game: &Game| game.loadout.arsenal.active;
        assert!(game.switch_weapon(1));
        assert_eq!(active(&game), Profile::Stock, "wraps past the end");
        assert!(
            !game.switch_weapon(1),
            "a second press in the same instant is a bounce"
        );
        settle(&mut game);
        assert!(game.switch_weapon(-1));
        assert_eq!(active(&game), Profile::Needles, "wraps backward");
        settle(&mut game);
        assert!(game.select_weapon(1));
        assert_eq!(active(&game), Profile::Spread);
        // Switching takes effect at once on the guns and is announced.
        assert_eq!(game.stats.spread, 1);
        assert_eq!(game.stats.needles, 0);
        assert!(game.arsenal_flash > 0.0);
        assert!(
            game.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::Switch { dry: false }))
        );
    }

    #[test]
    fn firing_is_billed_and_a_dry_profile_falls_back_to_stock_with_a_cue() {
        let mut game = empty_game();
        game.collect(Item::Surge(test_surge(Effect::Trait(Trait::Spread, 1))));
        let fuel = |game: &Game| game.cargo.metal;
        assert_eq!(fuel(&game), 100.0);
        game.step(DT, fire());
        assert!((fuel(&game) - (100.0 - Profile::Spread.cost(1))).abs() < 1e-4);
        assert_eq!(game.bullets.len(), 3);
        // Run the hold down to a hair under one volley.
        game.cargo.metal = Profile::Spread.cost(1) * 0.5;
        game.drain_cues();
        game.notices.clear();
        for _ in 0..12 {
            game.step(DT, fire());
        }
        let arsenal = &game.loadout.arsenal;
        assert_eq!(arsenal.active, Profile::Stock);
        assert_eq!(arsenal.previous, Profile::Spread);
        assert_eq!(game.stats.spread, 0);
        assert!(game.drain_cues().iter().any(|c| matches!(c, Cue::Dry)));
        assert!(game.notices.iter().any(|n| n.text.starts_with("DRY")));
        // The trigger pull that found it dry still fired (the stock pellet).
        assert!(game.bullets.iter().any(|b| b.friendly));
        // Stock fire costs nothing, ever.
        let metal = game.cargo.metal;
        for _ in 0..30 {
            game.step(DT, fire());
        }
        assert_eq!(game.cargo.metal, metal);
        // Refilling makes the profile usable again.
        assert!(!game.usable(Profile::Spread));
        game.collect(Item::Material(Material::Metal, 50.0));
        assert!(game.usable(Profile::Spread));
        settle(&mut game);
        assert!(game.switch_weapon(1));
        assert_eq!(game.loadout.arsenal.active, Profile::Spread);
        game.step(DT, fire());
        assert!(game.cargo.metal < metal + 50.0);
    }

    #[test]
    fn a_dry_launcher_falls_back_to_the_previous_usable_profile() {
        let mut game = empty_game();
        game.player_invulnerability = 1e9;
        game.collect(Item::Surge(test_surge(Effect::Trait(Trait::Spread, 1))));
        game.collect(Item::Surge(test_surge(Effect::Trait(Trait::Missiles, 1))));
        assert_eq!(game.loadout.arsenal.previous, Profile::Spread);
        game.cargo.volatiles = 0.0;
        game.step(DT, fire());
        assert_eq!(game.loadout.arsenal.active, Profile::Spread);
        assert!(game.stats.spread == 1 && game.stats.missiles == 0);
        // Landing on an empty profile on purpose is allowed; it shows as dry.
        settle(&mut game);
        game.switch_weapon(1);
        assert_eq!(game.loadout.arsenal.active, Profile::Missiles);
        assert!(!game.usable(Profile::Missiles));
        assert!(
            game.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::Switch { dry: true }))
        );
    }

    #[test]
    fn a_costly_arsenal_gives_meaningful_but_not_endless_fire() {
        // A full hold of the fuel lasts a good while for the cheap guns and a shorter one
        // for the launchers, and the bare stock gun never needs any.
        for profile in Profile::ALL.into_iter().skip(1) {
            let uses = 200.0 / profile.cost(1);
            let seconds = match profile.billing() {
                Billing::Volley => uses * 0.16,
                _ => uses * 1.3,
            };
            assert!(
                (30.0..=300.0).contains(&seconds),
                "{profile:?} {seconds:.0}s"
            );
        }
        let mut game = empty_game();
        for _ in 0..120 {
            game.step(DT, fire());
        }
        assert_eq!(game.cargo.total(), 0.0);
        assert!(game.bullets.iter().any(|b| b.friendly));
    }

    #[test]
    fn the_arsenal_counts_toward_power_whichever_profile_is_active() {
        let mut game = empty_game();
        let bare = game.power();
        game.collect(Item::Surge(test_surge(Effect::Trait(Trait::Spread, 2))));
        let armed = game.power();
        assert!(armed > bare + 0.3, "{bare} -> {armed}");
        game.collect(Item::Surge(test_surge(Effect::Trait(Trait::Needles, 2))));
        let two = game.power();
        assert!(two > armed);
        settle(&mut game);
        game.select_weapon(0);
        assert_eq!(
            game.power(),
            two,
            "switching to the stock gun does not change power"
        );
        // Boosts do not raise power on their own while idle.
        game.collect(Item::Surge(test_surge(Effect::Stat(Stat::Damage, 1.0))));
        assert_eq!(game.power(), two);
    }

    #[test]
    fn the_arsenal_is_deterministic() {
        let run = |seed: u64| {
            let mut rng = Rng::new(seed);
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            spawn(&mut game, &Species::fatso(), Vec2::new(0.0, 500.0));
            for step in 0..600 {
                if step % 50 == 0 {
                    let source = Source::plain(2.0, SectorParams::HOME);
                    game.collect(Item::Surge(upgrades::roll_surge(&mut rng, &source)));
                    game.collect(Item::Material(Material::Metal, 30.0));
                }
                if step % 70 == 0 {
                    game.switch_weapon(1);
                }
                game.step(DT, fire());
            }
            (
                game.cargo,
                game.loadout.arsenal.clone(),
                game.stats,
                game.bullets.len(),
            )
        };
        assert_eq!(run(4), run(4));
        assert_ne!(run(4).1, run(5).1);
    }
}
