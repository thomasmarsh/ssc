//! The realm the ship is in. A realm is a pure function of a sector (`realm::realm`); this module
//! decides when the ship has really moved from one realm to the next (the same hysteresis as
//! `regions`, with a longer hold and a longer gap), posts the `ENTERING THE REALM OF` banner in
//! the style of the extirpation notice, and counts realms toward the run. It also holds the
//! effects of the sector the ship is in (what the ship's own reach, sensors, gravity and mining
//! see), which are not hysteretic: they follow the ship's sector and fade across borders on their
//! own (see `realm::EDGE_RAMP`).

use super::*;
use crate::realm::{Effects, Realm, RealmKind, realm};
use tuning::{REALM_COOLDOWN, REALM_HOLD};

/// Where the realm tracking stands.
#[derive(Clone, Debug, Default)]
pub struct RealmState {
    /// The sector the realm was last computed for, and the realm there.
    here: Option<(SectorId, Realm)>,
    /// The realm the ship has announced.
    current: Option<Realm>,
    /// A realm the ship is in but has not yet held long enough: its key and the seconds held.
    candidate: Option<(u64, f32)>,
    /// Seconds since the last banner.
    since_banner: f32,
    /// Game time the current realm was committed to.
    entered: f32,
    /// Until when a fizzled dash and parry stay locked.
    fizzle_lock: [f32; 2],
}

impl Game {
    /// Per tick: notices a change of realm, with hysteresis. The starting realm is recorded but
    /// not announced (it is the gentle start); every later one gets a banner.
    pub(super) fn update_realm(&mut self, dt: f32) {
        let sector = self.sector();
        if self.realms.here.as_ref().map(|h| h.0) != Some(sector) {
            let r = realm(self.seed, sector);
            self.realms.here = Some((sector, r));
        }
        self.realms.since_banner += dt;
        let Some((_, here)) = self.realms.here.clone() else {
            return;
        };
        let Some(current) = self.realms.current.as_ref() else {
            self.enter_realm(here, false);
            return;
        };
        if current.key == here.key {
            self.realms.candidate = None;
            return;
        }
        let held = match self.realms.candidate {
            Some((key, held)) if key == here.key => held + dt,
            _ => dt,
        };
        self.realms.candidate = Some((here.key, held));
        if held >= REALM_HOLD && self.realms.since_banner >= REALM_COOLDOWN {
            self.enter_realm(here, true);
        }
    }

    /// Commits to a realm: records it in the run and, past the start, posts the banner.
    fn enter_realm(&mut self, entered: Realm, mid_run: bool) {
        if self.run.realms.insert(entered.key) {
            self.run.realm_names.push(entered.name.clone());
        }
        self.realms.candidate = None;
        self.realms.since_banner = if mid_run { 0.0 } else { REALM_COOLDOWN };
        if mid_run {
            self.notify(entered.banner(), upgrades::Rarity::Epic);
        }
        self.realms.current = Some(entered);
        self.realms.entered = self.time;
    }

    /// The realm the ship is announced to be in (none before the first tick).
    pub fn realm(&self) -> Option<&Realm> {
        self.realms.current.as_ref()
    }

    /// The HUD's realm tag: the announced realm, with how much of it the ship's own sector
    /// feels (a border reads as gentle).
    pub fn realm_tag(&self) -> Option<super::hud::RealmTag> {
        let current = self.realms.current.as_ref()?;
        let spec = current.spec();
        let felt = self
            .realms
            .here
            .as_ref()
            .filter(|(_, r)| r.key == current.key)
            .map_or(current.intensity, |(_, r)| r.intensity);
        let mut axes = spec.primary.to_vec();
        let primary = axes.len();
        axes.extend(spec.mild);
        Some(super::hud::RealmTag {
            name: current.name.to_uppercase(),
            title: spec.title,
            tint: current.tint(),
            axes,
            primary,
            gentle: spec.primary.is_empty() || felt < 0.15,
            alpha: super::hud::region_alpha(self.time - self.realms.entered),
        })
    }

    /// The details panel's lines for the realm of the ship's sector: its name and kind, what it
    /// tests, and what it changes here (biggest first, as signed percents).
    pub fn realm_lines(&self) -> Vec<String> {
        let r = self.realm_of(self.sector());
        let mut lines = vec![
            format!("REALM  {}", r.name.to_uppercase()),
            r.spec().title.to_string(),
            r.tests_line(),
        ];
        let changes = r.changes();
        if r.intensity < 0.15 && !r.spec().primary.is_empty() {
            lines.push("the edge of the realm: its effects are still faint".to_string());
        }
        for pair in changes.chunks(2) {
            lines.push(
                pair.iter()
                    .map(|(label, pct)| format!("{label} {pct:+}%"))
                    .collect::<Vec<_>>()
                    .join("   "),
            );
        }
        lines.push(r.spec().blurb.to_string());
        lines
    }

    /// The realm of any sector (for the charts and the map).
    pub fn realm_of(&self, sector: SectorId) -> Realm {
        realm(self.seed, sector)
    }

    /// What the realm of the ship's own sector does to the ship and the world around it.
    pub fn realm_effects(&self) -> Effects {
        crate::realm::effects(self.seed, self.sector())
    }

    /// Whether an energy ability (0 dash, 1 parry) fizzles as it is raised, in a realm that
    /// dulls them. Deterministic: a hash of the seed, the game time and the ability, so replays
    /// agree and no stream moves. A fizzle costs nothing but locks the ability for `FIZZLE_LOCK`
    /// seconds, so a mashed key cannot reroll it every frame.
    pub(super) fn ability_fizzles(&mut self, ability: i32) -> bool {
        let chance = self
            .realm_effects()
            .fizzle
            .clamp(0.0, crate::realm::MAX_FIZZLE);
        if chance <= 0.0 {
            return false;
        }
        if self.realms.fizzle_lock[ability as usize] > self.time {
            return true;
        }
        let tick = (self.time * 20.0) as i32;
        let roll = (world::hash2(self.seed ^ tuning::FIZZLE_SALT, tick, ability) >> 40) as f32
            / 16_777_216.0;
        if roll < chance {
            self.realms.fizzle_lock[ability as usize] = self.time + tuning::FIZZLE_LOCK;
            return true;
        }
        false
    }

    /// Whether the ship is in the realm of kind `kind`.
    pub fn in_realm(&self, kind: RealmKind) -> bool {
        self.realms
            .here
            .as_ref()
            .is_some_and(|(_, r)| r.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realm::{CATALOG, Realm};
    use crate::simulation::tests::{DT, empty_game, set_player};
    use tuning::REALM_COOLDOWN;

    const SEED: u64 = 42;

    /// A sector deep inside a realm of kind `id` (full strength), found by scanning outward.
    fn deep_in(seed: u64, id: &str) -> SectorId {
        let kind = RealmKind::by_id(id).expect("a catalog kind");
        for r in (16..600).step_by(3) {
            for k in (-r..=r).step_by(7) {
                for (x, y) in [(r, k), (-r, k), (k, r), (k, -r)] {
                    let s = SectorId { x, y };
                    let (_, found, intensity) = crate::realm::identity(seed, s);
                    if found == kind && intensity >= 0.95 {
                        return s;
                    }
                }
            }
        }
        panic!("no {id} realm found");
    }

    /// Two sectors in different realms, neither the starter, with the ring between them.
    fn border(seed: u64) -> (SectorId, SectorId) {
        for x in 20..400 {
            for y in (-40..40).step_by(3) {
                let (a, b) = (SectorId { x, y }, SectorId { x: x + 1, y });
                let (ra, rb) = (crate::realm::realm(seed, a), crate::realm::realm(seed, b));
                if ra.key != rb.key && ra.kind != RealmKind::CRADLE && rb.kind != RealmKind::CRADLE
                {
                    return (a, b);
                }
            }
        }
        panic!("no realm border");
    }

    /// The banners seen over a test, in order (notices fade, so they are logged as they post).
    #[derive(Default)]
    struct Log(Vec<String>);

    fn banners(game: &Game) -> usize {
        game.notices
            .iter()
            .filter(|n| n.text.starts_with("ENTERING THE REALM"))
            .count()
    }

    fn stay(game: &mut Game, at: Vec2, seconds: f32, log: &mut Log) -> usize {
        let mut changes = 0;
        let mut last = game.realm().map(|r| r.key);
        for _ in 0..(seconds / 0.05) as usize {
            set_player(game, at, Vec2::ZERO);
            game.step(0.05, Input::default());
            let now = game.realm().map(|r| r.key);
            changes += usize::from(now != last);
            last = now;
            for n in game
                .notices
                .iter()
                .filter(|n| n.text.starts_with("ENTERING THE REALM"))
            {
                if !log.0.contains(&n.text) {
                    assert_eq!(n.rarity, upgrades::Rarity::Epic, "the extirpation style");
                    log.0.push(n.text.clone());
                }
            }
        }
        changes
    }

    #[test]
    fn the_start_is_the_gentle_starter_and_is_not_announced() {
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.step(DT, Input::default());
        let r = game.realm().expect("a realm after the first tick");
        assert_eq!(r.kind, RealmKind::CRADLE);
        assert_eq!(banners(&game), 0, "the gentle start has no banner");
        assert_eq!(game.run.realms.len(), 1);
        assert!(game.realm_effects().is_neutral());
        let tag = game.realm_tag().expect("a tag");
        assert!(tag.gentle && tag.axes.is_empty() && tag.name == r.name.to_uppercase());
        assert_eq!(game.hud().realm, Some(tag));
    }

    /// A new realm needs a steady stay and the cooldown, posts the banner once in the
    /// extirpation style, and a border walked back and forth never flickers it.
    #[test]
    fn the_realm_banner_has_hysteresis() {
        let (a, b) = border(SEED);
        let mut log = Log::default();
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.step(DT, Input::default());
        game.teleport(a.center());
        assert_eq!(stay(&mut game, a.center(), REALM_HOLD - 1.5, &mut log), 0);
        assert_eq!(game.realm().unwrap().kind, RealmKind::CRADLE);
        assert!(log.0.is_empty(), "a brief visit announces nothing");
        assert_eq!(stay(&mut game, a.center(), REALM_COOLDOWN, &mut log), 1);
        let ra = crate::realm::realm(SEED, a);
        assert_eq!(game.realm().unwrap().key, ra.key);
        assert_eq!(log.0.len(), 1);
        assert_eq!(
            log.0[0],
            format!("ENTERING THE REALM OF {}", ra.name.to_uppercase())
        );
        assert_eq!(game.run.realms.len(), 2);
        assert!(game.run_report().lines[1].contains("REALMS 2"));
        // Crossing every second never holds the new realm long enough.
        let mut changes = 0;
        for k in 0..60 {
            let at = if k % 2 == 0 { b.center() } else { a.center() };
            changes += stay(&mut game, at, 1.0, &mut log);
        }
        assert_eq!(changes, 0, "a flicker would show");
        assert_eq!(log.0.len(), 1);
        // A real stay on the far side switches once, after the cooldown, and counts the realm.
        assert_eq!(
            stay(&mut game, b.center(), REALM_COOLDOWN + REALM_HOLD, &mut log),
            1
        );
        assert_eq!(log.0.len(), 2);
        assert_eq!(game.run.realms.len(), 3);
        let lines = game.run_report().lines;
        assert!(lines.iter().any(|l| l.starts_with("REALMS VISITED")));
        let _ = banners(&game);
    }

    #[test]
    fn the_hud_tag_and_details_name_the_realm_and_what_it_stresses() {
        let spot = deep_in(SEED, "veil");
        let mut game = Game::new(SEED);
        game.player_invulnerability = 1e9;
        game.teleport(spot.center());
        stay(
            &mut game,
            spot.center(),
            REALM_COOLDOWN + REALM_HOLD + 1.0,
            &mut Log::default(),
        );
        let tag = game.hud().realm.expect("a tag");
        assert_eq!(tag.title, "THE VEIL");
        assert!(!tag.gentle);
        assert_eq!(
            tag.axes[..tag.primary],
            [crate::realm::Axis::Range, crate::realm::Axis::Sensors]
        );
        assert_eq!(tag.axes.len(), 3, "two primary and a mild one");
        assert!(tag.name.ends_with("SHROUD") && tag.tint.iter().all(|c| (0.0..=1.0).contains(c)));
        let lines = game.realm_lines();
        assert!(lines[0].starts_with("REALM  ") && lines[0].ends_with("SHROUD"));
        assert_eq!(lines[1], "THE VEIL");
        assert!(lines[2].contains("RANGE") && lines[2].contains("(DEFENSE)"));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("WEAPON RANGE -35%") && l.contains("SENSORS -45%")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_veil_shortens_shots_and_sensors_and_a_cradle_does_not() {
        let mut shots = Vec::new();
        for spot in [SectorId::ORIGIN, deep_in(SEED, "veil")] {
            let mut game = empty_game();
            game.teleport(spot.center());
            set_player(&mut game, spot.center(), Vec2::ZERO);
            game.bodies.retain(|b| b.kind == BodyKind::Player);
            game.step(
                DT,
                Input {
                    fire: true,
                    ..Input::default()
                },
            );
            let shot = game.bullets.iter().find(|b| b.friendly).expect("a shot");
            shots.push((shot.remaining, game.realm_effects()));
        }
        let (home, veil) = (shots[0].0, shots[1].0);
        assert!(home > 1.5, "{home}");
        assert!((veil / home - 0.65).abs() < 0.03, "{veil} against {home}");
        assert!(shots[0].1.is_neutral() && shots[1].1.sensor < 0.6);
    }

    #[test]
    fn dead_reach_fizzles_dash_and_parry_deterministically_and_nowhere_else() {
        let rate = |spot: SectorId| {
            let mut game = empty_game();
            game.teleport(spot.center());
            game.step(DT, Input::default());
            let mut fizzled = 0;
            for k in 0..400 {
                game.time += 1.0;
                game.realms.fizzle_lock = [0.0; 2];
                fizzled += usize::from(game.ability_fizzles(k % 2));
            }
            (fizzled as f32 / 400.0, game.time)
        };
        let dead = deep_in(SEED, "dead_reach");
        let (share, _) = rate(dead);
        let want = RealmKind::by_id("dead_reach")
            .unwrap()
            .spec()
            .effects
            .fizzle;
        assert!((share - want).abs() < 0.07, "{share} against {want}");
        assert_eq!(rate(dead).0, share, "the rolls repeat");
        assert_eq!(rate(SectorId::ORIGIN).0, 0.0);
        // A fizzle locks the ability for a moment, and the lock holds even against a lucky roll.
        let mut game = empty_game();
        game.teleport(dead.center());
        game.step(DT, Input::default());
        let mut at = 0.0;
        while !game.ability_fizzles(0) {
            at += 0.5;
            game.time += 0.5;
            assert!(at < 600.0, "a fizzle should happen");
        }
        game.time += tuning::FIZZLE_LOCK * 0.5;
        assert!(game.ability_fizzles(0), "still locked");
    }

    #[test]
    fn iron_plates_turn_light_hits_away_and_glass_breaks_easily() {
        let hit = |id: &str, amount: f32| -> f32 {
            let mut game = empty_game();
            let species = crate::genome::Species::bogey();
            let at = crate::world::SectorId::ORIGIN.center() + Vec2::new(900.0, 0.0);
            let creature = crate::simulation::tests::spawn(&mut game, &species, at);
            let kind = RealmKind::by_id(id).unwrap();
            let e = kind.spec().effects;
            let body = game.bodies.iter_mut().find(|b| b.id == creature).unwrap();
            body.genes.foe = e.foe;
            body.shield = 100.0;
            body.health = 100.0;
            let before = body.health + body.shield * e.foe.shield;
            damage(body, amount, 0.0);
            (before - (body.health + body.shield * e.foe.shield)) / amount
        };
        // Against iron a light hit loses most of itself; a heavy one barely notices.
        let (light, heavy) = (hit("iron_tide", 8.0), hit("iron_tide", 80.0));
        assert!(
            light < 0.5 && heavy > 0.85 && heavy > light,
            "{light} {heavy}"
        );
        assert!(
            light >= tuning::PLATING_FLOOR - 1e-3,
            "never entirely: {light}"
        );
        // The shield is a bigger pool: more raw damage soaks in before the hull is touched.
        let mut game = empty_game();
        let species = crate::genome::Species::bogey();
        let c = crate::simulation::tests::spawn(&mut game, &species, Vec2::new(900.0, 0.0));
        let body = game.bodies.iter_mut().find(|b| b.id == c).unwrap();
        body.genes.foe = RealmKind::by_id("iron_tide").unwrap().spec().effects.foe;
        (body.shield, body.health) = (50.0, 50.0);
        damage(body, 100.0, 0.0);
        assert_eq!(body.health, 50.0, "2.4 x 50 of shield soaks 100 whole");
        // Glass: the same hit does more to the hull (it has less of it).
        let glass = hit("glass_seas", 40.0);
        assert!(
            (glass - 1.0).abs() < 0.01,
            "damage dealt is what was dealt: {glass}"
        );
    }

    #[test]
    fn the_realm_pulls_gravity_and_pays_mining_and_changes_jams() {
        let crush = crate::realm::effects(SEED, deep_in(SEED, "crush"));
        let gold = crate::realm::effects(SEED, deep_in(SEED, "quiet_gold"));
        assert!(crush.gravity > 1.5 && crush.wells > 2.0);
        assert!(gold.mining > 1.5 && gold.threat < 0.7);
        let dead = crate::realm::effects(SEED, deep_in(SEED, "dead_reach"));
        assert!(dead.jam_time > 1.3);
    }

    #[test]
    fn every_catalog_row_has_a_blurb_that_names_a_dimension_it_stresses() {
        for spec in &CATALOG {
            let probe = Realm {
                key: 1,
                kind: RealmKind::by_id(spec.id).unwrap(),
                name: "Probe".into(),
                intensity: 1.0,
                effects: spec.effects,
            };
            assert!(!spec.blurb.is_empty());
            for axis in probe.stress() {
                assert!(probe.stress_line().contains(axis.label()), "{}", spec.id);
            }
        }
    }
}
