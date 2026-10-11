//! Developer toggles (docs/DEVTOOLS.md, Phase A). Pure data on the `Game` plus the few hooks the
//! rules read, so the in-game panel is only a view onto it. Everything is off by default and a
//! default `DevState` changes nothing: the hooks are plain reads of these flags, take no random
//! draws and add no work to a normal run. The panel itself is gated by the environment variable
//! `SSC_DEV=1` (see `enabled`); the library never reads the environment on its own.

use super::arsenal::Profile;
use super::organs::{Organ, Strain};
use super::skills::Skill;
use super::upgrades::{self, Rarity};
use super::*;
use crate::genome::{Genome, Species};

/// Time scales the panel steps through (1.0 is normal).
pub const TIME_SCALES: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
const NORMAL_SCALE: usize = 2;
/// Salt of the private stream that rolls granted parts, so they never touch the loot stream.
const GRANT_SALT: u64 = 0xDE57_0000_0000_6A17;
/// Sectors the teleport target may be set to, either way.
pub const TARGET_LIMIT: i32 = 999;
/// Attempts to roll a part of exactly the wanted rarity.
const GRANT_TRIES: usize = 600;

/// Whether the developer panel is switched on for this process: `SSC_DEV=1`.
pub fn enabled() -> bool {
    std::env::var("SSC_DEV").is_ok_and(|v| v == "1")
}

/// What can be spawned at the ship: the wild classics, then the authored specimens (the names
/// `SSC_SPECIMEN` takes).
pub const SPAWNS: [&str; 47] = [
    "bogey",
    "fatso",
    "lunatic",
    "leech",
    "smarty",
    "serpent",
    "skipjack",
    "veilwing",
    "hullpick",
    "stormcap",
    "longslinger",
    "softslinger",
    "wildslinger",
    "wildsong",
    "wildmulti",
    "multijammer",
    "multioozer",
    "argus",
    "gloomfeeder",
    "dizzard",
    "pushwhale",
    "tarbloom",
    "lenswyrm",
    "tidegorger",
    "splitter",
    "murmur",
    "dirgewhale",
    "lurefish",
    "hullworm",
    "remora",
    "weaver",
    "slinger",
    "runekeeper",
    "oozer",
    "seamer",
    "squid",
    "octopus",
    "snake",
    "crab",
    "jelly",
    "ray",
    "starfish",
    "puffer",
    "plumeworm",
    "treeling",
    "ribwyrm",
    "builder",
];

/// The authored genome behind a spawn or `SSC_SPECIMEN` name; unknown names give the default.
pub fn specimen_genome(name: &str) -> Genome {
    match name {
        "bogey" => Genome::bogey(),
        "fatso" => Genome::fatso(),
        "lunatic" => Genome::lunatic(),
        "leech" => Genome::leech(),
        "smarty" => Genome::smarty(),
        "serpent" => Genome::serpent(),
        "skipjack" => Genome::skipjack(),
        "veilwing" => Genome::veilwing(),
        "hullpick" => Genome::hullpick(),
        "stormcap" => Genome::stormcap(),
        "wildslinger" | "wildsong" | "wildmulti" => {
            let seed = match name {
                "wildslinger" => 639,
                "wildsong" => 322,
                _ => 242,
            };
            Genome::sample(
                &mut crate::world::Rng::new(seed),
                &crate::world::SectorParams {
                    depth: 30.0,
                    danger: 0.5,
                    aggression: 0.5,
                    density: 0.5,
                    distortion: 0.5,
                    tech: 0.5,
                    swarm: 0.5,
                },
            )
        }
        "longslinger" => {
            let mut g = Genome::slinger();
            g.anatomy = Some(crate::anatomy::AnimalSpecimen {
                genome: crate::anatomy::AnimalGenome {
                    archetype: crate::anatomy::Archetype::Crab,
                    segments: 3,
                    limbs: 8,
                    limb_len: 1,
                    ..Default::default()
                },
                seed: 2,
            });
            g.appearance.organ_reach = 1.6;
            crate::power::stamp(&mut g, crate::power::Power::Emp, 0.9);
            g.limited()
        }
        "softslinger" => {
            let mut g = Genome::slinger();
            g.appearance.surface = crate::development::Surface::Soft;
            g.limbs = 0;
            g.sides = 3;
            g.aspect = 1.6;
            g
        }
        "multijammer" => {
            let mut g = Genome::stormcap();
            crate::power::stamp(&mut g, crate::power::Power::Confuse, 0.8);
            crate::power::stamp(&mut g, crate::power::Power::Glare, 0.7);
            g
        }
        "multioozer" => {
            let mut g = Genome::oozer();
            crate::power::stamp(&mut g, crate::power::Power::Repel, 0.7);
            crate::power::stamp(&mut g, crate::power::Power::Song, 0.7);
            g
        }
        "argus" => Genome::argus(),
        "gloomfeeder" => Genome::gloomfeeder(),
        "dizzard" => Genome::dizzard(),
        "pushwhale" => Genome::pushwhale(),
        "tarbloom" => Genome::tarbloom(),
        "lenswyrm" => Genome::lenswyrm(),
        "tidegorger" => Genome::tidegorger(),
        "splitter" => Genome::splitter(),
        "murmur" => Genome::murmur(),
        "dirgewhale" => Genome::dirgewhale(),
        "lurefish" => Genome::lurefish(),
        "hullworm" => Genome::hullworm(),
        "remora" => Genome::remora(),
        "weaver" => Genome::weaver(),
        "slinger" => Genome::slinger(),
        "runekeeper" => Genome::runekeeper(),
        "oozer" => Genome::oozer(),
        "seamer" => Genome::seamer(),
        "builder" => Genome::builder(),
        _ => crate::bodyplan::specimen_by_name(name).unwrap_or_default(),
    }
}

/// The developer switches and selections. `Default` is the normal game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DevState {
    /// Nothing hurts the hull or the shield.
    pub invulnerable: bool,
    /// The hold is kept full, so no weapon or boost ever runs dry.
    pub infinite_ammo: bool,
    /// Everything that charges materials (bench, pads, grafts, the chart) is free.
    pub free_purchases: bool,
    /// Parry, dash and ping are always ready.
    pub no_cooldowns: bool,
    /// Dying costs no life.
    pub unlimited_lives: bool,
    /// Creatures and stations neither move, shoot nor hunt.
    pub freeze_enemies: bool,
    /// Index into `TIME_SCALES`.
    time_scale: usize,
    /// Index into `Rarity::ALL` for the part grant.
    part_rarity: usize,
    /// Index into `SPAWNS`.
    spawn: usize,
    /// The teleport target, in sectors.
    pub target: (i32, i32),
    /// Parts granted so far (keys the private roll stream).
    grants: u32,
    /// Draws the area readout tags (level, band, needs) on the HUD; off in normal play, since
    /// the player is told difficulty by feel and by the threat pips (`docs/LEGIBILITY.md`).
    pub area_overlay: bool,
}

impl Default for DevState {
    fn default() -> Self {
        Self {
            invulnerable: false,
            infinite_ammo: false,
            free_purchases: false,
            no_cooldowns: false,
            unlimited_lives: false,
            freeze_enemies: false,
            time_scale: NORMAL_SCALE,
            part_rarity: 0,
            spawn: 0,
            target: (0, 0),
            grants: 0,
            area_overlay: false,
        }
    }
}

impl DevState {
    /// Whether any toggle is on (the HUD wears a DEV tag): the selections and one-shot actions
    /// do not count, only the switches and a changed time scale.
    pub fn active(&self) -> bool {
        self.invulnerable
            || self.infinite_ammo
            || self.free_purchases
            || self.no_cooldowns
            || self.unlimited_lives
            || self.freeze_enemies
            || self.time_scale != NORMAL_SCALE
    }

    pub fn time_scale(&self) -> f32 {
        TIME_SCALES[self.time_scale]
    }

    pub fn part_rarity(&self) -> Rarity {
        Rarity::ALL[self.part_rarity]
    }

    pub fn spawn_name(&self) -> &'static str {
        SPAWNS[self.spawn]
    }
}

/// One row of the developer panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevRow {
    Invulnerable,
    InfiniteAmmo,
    FreePurchases,
    NoCooldowns,
    UnlimitedLives,
    FreezeEnemies,
    TimeScale,
    FillHold,
    GrantSkills,
    GrantWeapons,
    GrantOrgans,
    GrantPart,
    TargetX,
    TargetY,
    Teleport,
    Spawn,
    AreaOverlay,
}

impl DevRow {
    pub const ALL: [DevRow; 17] = [
        Self::Invulnerable,
        Self::InfiniteAmmo,
        Self::FreePurchases,
        Self::NoCooldowns,
        Self::UnlimitedLives,
        Self::FreezeEnemies,
        Self::TimeScale,
        Self::FillHold,
        Self::GrantSkills,
        Self::GrantWeapons,
        Self::GrantOrgans,
        Self::GrantPart,
        Self::TargetX,
        Self::TargetY,
        Self::Teleport,
        Self::Spawn,
        Self::AreaOverlay,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Invulnerable => "INVULNERABLE",
            Self::InfiniteAmmo => "INFINITE FUEL + AMMO",
            Self::FreePurchases => "FREE PURCHASES",
            Self::NoCooldowns => "NO COOLDOWNS",
            Self::UnlimitedLives => "UNLIMITED LIVES",
            Self::FreezeEnemies => "FREEZE ENEMIES",
            Self::TimeScale => "TIME SCALE",
            Self::FillHold => "MAX MATERIALS",
            Self::GrantSkills => "GRANT ALL SKILLS",
            Self::GrantWeapons => "GRANT ALL WEAPONS",
            Self::GrantOrgans => "GRANT ALL ORGANS",
            Self::GrantPart => "GRANT FITTED PART",
            Self::TargetX => "TARGET SECTOR X",
            Self::TargetY => "TARGET SECTOR Y",
            Self::Teleport => "TELEPORT",
            Self::Spawn => "SPAWN AT SHIP",
            Self::AreaOverlay => "AREA OVERLAY",
        }
    }

    /// What the row says about itself, for the line under the list.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Invulnerable => "hull and shield take no harm",
            Self::InfiniteAmmo => "the hold stays full: no weapon or boost runs dry",
            Self::FreePurchases => "the bench, pads, grafts and the chart charge nothing",
            Self::NoCooldowns => "parry, dash and ping are always ready",
            Self::UnlimitedLives => "dying costs no life",
            Self::FreezeEnemies => "creatures and stations do not move, shoot or hunt",
            Self::TimeScale => "left and right step 0.25x to 4x",
            Self::FillHold => "enter fills every material to the cap",
            Self::GrantSkills => "enter raises every bench skill to its top level",
            Self::GrantWeapons => "enter raises every weapon to its top level",
            Self::GrantOrgans => "enter owns every organ at top level and fits what slots allow",
            Self::GrantPart => "left and right choose the rarity, enter fits a part",
            Self::TargetX | Self::TargetY => "left and right step one sector",
            Self::Teleport => "enter jumps to the middle of the target sector",
            Self::Spawn => "left and right choose, enter places three near the ship",
            Self::AreaOverlay => {
                "the old numeric area tags, sector and realm, and the way to the keeper"
            }
        }
    }

    /// Whether the row is a one-shot action (enter does it) rather than a value.
    pub fn is_action(self) -> bool {
        matches!(
            self,
            Self::FillHold
                | Self::GrantSkills
                | Self::GrantWeapons
                | Self::GrantOrgans
                | Self::Teleport
        )
    }
}

fn on_off(on: bool) -> String {
    if on { "ON" } else { "OFF" }.to_string()
}

fn wrap(index: usize, len: usize, dir: i32) -> usize {
    (index as i32 + dir.signum()).rem_euclid(len as i32) as usize
}

impl Game {
    /// What the panel shows on the right of a row.
    pub fn dev_value(&self, row: DevRow) -> String {
        let dev = &self.dev;
        match row {
            DevRow::Invulnerable => on_off(dev.invulnerable),
            DevRow::InfiniteAmmo => on_off(dev.infinite_ammo),
            DevRow::FreePurchases => on_off(dev.free_purchases),
            DevRow::NoCooldowns => on_off(dev.no_cooldowns),
            DevRow::UnlimitedLives => on_off(dev.unlimited_lives),
            DevRow::FreezeEnemies => on_off(dev.freeze_enemies),
            DevRow::AreaOverlay => on_off(dev.area_overlay),
            DevRow::TimeScale => format!("{}x", dev.time_scale()),
            DevRow::GrantPart => dev.part_rarity().label().to_uppercase(),
            DevRow::TargetX => dev.target.0.to_string(),
            DevRow::TargetY => dev.target.1.to_string(),
            DevRow::Spawn => dev.spawn_name().to_uppercase(),
            DevRow::FillHold
            | DevRow::GrantSkills
            | DevRow::GrantWeapons
            | DevRow::GrantOrgans
            | DevRow::Teleport => String::new(),
        }
    }

    /// Changes a row: `dir` is -1 or 1 for left and right, 0 for enter. Toggles flip on any of
    /// them, selections step on left and right, actions run on enter.
    pub fn dev_change(&mut self, row: DevRow, dir: i32) {
        let dev = &mut self.dev;
        match row {
            DevRow::Invulnerable => dev.invulnerable = !dev.invulnerable,
            DevRow::InfiniteAmmo => dev.infinite_ammo = !dev.infinite_ammo,
            DevRow::FreePurchases => dev.free_purchases = !dev.free_purchases,
            DevRow::NoCooldowns => dev.no_cooldowns = !dev.no_cooldowns,
            DevRow::UnlimitedLives => dev.unlimited_lives = !dev.unlimited_lives,
            DevRow::FreezeEnemies => dev.freeze_enemies = !dev.freeze_enemies,
            DevRow::AreaOverlay => dev.area_overlay = !dev.area_overlay,
            DevRow::TimeScale if dir != 0 => {
                dev.time_scale = (dev.time_scale as i32 + dir.signum())
                    .clamp(0, TIME_SCALES.len() as i32 - 1)
                    as usize;
            }
            DevRow::GrantPart if dir != 0 => {
                dev.part_rarity = wrap(dev.part_rarity, Rarity::ALL.len(), dir);
            }
            DevRow::GrantPart => self.dev_grant_part(),
            DevRow::TargetX if dir != 0 => {
                dev.target.0 = (dev.target.0 + dir.signum()).clamp(-TARGET_LIMIT, TARGET_LIMIT);
            }
            DevRow::TargetY if dir != 0 => {
                dev.target.1 = (dev.target.1 + dir.signum()).clamp(-TARGET_LIMIT, TARGET_LIMIT);
            }
            DevRow::Spawn if dir != 0 => dev.spawn = wrap(dev.spawn, SPAWNS.len(), dir),
            DevRow::Spawn => {
                let name = self.dev.spawn_name();
                self.dev_spawn(name);
            }
            DevRow::FillHold if dir == 0 => self.dev_fill_hold(),
            DevRow::GrantSkills if dir == 0 => self.dev_grant_skills(),
            DevRow::GrantWeapons if dir == 0 => self.dev_grant_weapons(),
            DevRow::GrantOrgans if dir == 0 => self.dev_grant_organs(),
            DevRow::Teleport if dir == 0 => {
                let (x, y) = self.dev.target;
                self.dev_teleport(x, y);
            }
            _ => {}
        }
        self.sync_dev();
    }

    /// Pushes the flags that live inside other state (the hold) out of `dev`.
    pub(super) fn sync_dev(&mut self) {
        self.cargo.dev_free = self.dev.free_purchases;
    }

    /// The grace time the damage rules read: endless while invulnerable, else the real one.
    pub(super) fn guard_time(&self) -> f32 {
        if self.dev.invulnerable {
            1e9
        } else {
            self.player_invulnerability
        }
    }

    /// End-of-step upkeep of the continuous toggles. Nothing runs unless one is on.
    pub(super) fn apply_dev(&mut self) {
        if !self.dev.active() {
            return;
        }
        if self.dev.invulnerable
            && let Some(ship) = self.bodies.iter_mut().find(|b| b.kind == BodyKind::Player)
        {
            ship.health = ship.max_health;
            ship.shield = ship.max_shield;
        }
        if self.dev.infinite_ammo {
            self.dev_fill_hold();
        }
        if self.dev.no_cooldowns {
            self.parry.cooldown = 0.0;
            self.dash.cooldown = 0.0;
            self.ping.cooldown = 0.0;
        }
        if self.dev.freeze_enemies {
            for body in self
                .bodies
                .iter_mut()
                .filter(|b| matches!(b.kind, BodyKind::Creature | BodyKind::Base))
            {
                body.velocity = Vec2::ZERO;
            }
        }
    }

    /// Fills every material to the cap.
    pub fn dev_fill_hold(&mut self) {
        for kind in Material::ALL {
            let room = self.cargo.room(kind);
            self.cargo.add(kind, room);
        }
    }

    /// Every bench skill to its top level.
    pub fn dev_grant_skills(&mut self) {
        for skill in Skill::ALL {
            while self.loadout.skills.raise(skill).is_some() {}
        }
        self.refresh_stats();
    }

    /// Every weapon profile owned at its top level.
    pub fn dev_grant_weapons(&mut self) {
        for profile in Profile::ALL {
            self.loadout.arsenal.acquire(profile, profile.max_level());
        }
        self.refresh_stats();
    }

    /// Every organ owned at its top level, and fitted as far as the slots go (the first graft
    /// would cost materials, so the fitting is done free).
    pub fn dev_grant_organs(&mut self) {
        for organ in Organ::ALL {
            let strain = Strain::typical(organ);
            for _ in 0..tuning::ORGAN_LEVELS {
                self.loadout.organs.acquire(strain, &self.tune);
            }
        }
        let was_free = self.cargo.dev_free;
        self.cargo.dev_free = true;
        for organ in Organ::ALL {
            if !self.loadout.organs.is_fitted(organ) {
                let _ = self.bench_organ(organ);
            }
        }
        self.cargo.dev_free = was_free;
        self.refresh_stats();
    }

    /// Rolls a part of exactly the chosen rarity on a private stream and bolts it on.
    pub fn dev_grant_part(&mut self) {
        let rarity = self.dev.part_rarity();
        self.dev.grants += 1;
        let mut rng = Rng::new(self.seed ^ GRANT_SALT ^ u64::from(self.dev.grants));
        let mut source = upgrades::Source::plain(self.threat().max(1.0), self.params());
        source.min_rarity = rarity;
        let mut part = upgrades::roll_part(&mut rng, &source);
        for _ in 0..GRANT_TRIES {
            if part.rarity == rarity {
                break;
            }
            part = upgrades::roll_part(&mut rng, &source);
        }
        let name = part.name.clone();
        let rarity = part.rarity;
        let outcome = self.loadout.acquire(part);
        self.refresh_stats();
        let note = match outcome.installed {
            Some(upgrades::Install::Scrapped(_)) => format!("DEV  {name} SCRAPPED (slot full)"),
            _ => format!("DEV  {name}"),
        };
        self.notify(note, rarity);
    }

    /// Moves the ship to the middle of a sector.
    pub fn dev_teleport(&mut self, x: i32, y: i32) {
        let size = world::SECTOR_SIZE;
        self.teleport(Vec2::new(x as f32 * size, y as f32 * size));
    }

    /// Places three of a named species or authored specimen around the ship, like `SSC_SPECIMEN`.
    pub fn dev_spawn(&mut self, name: &str) {
        let genome = specimen_genome(name);
        let ship = self.player().map_or(Vec2::ZERO, |p| p.position);
        for k in 0..3 {
            let at = ship + Vec2::from_angle(0.6 + k as f32 * 2.1) * (420.0 + 90.0 * k as f32);
            self.place_creature(&Species::of(genome), at);
        }
    }

    /// Steps the game for one frame at the developer time scale: slower scales shorten the
    /// step, faster ones run several. At the normal scale this is exactly `step`.
    pub fn step_scaled(&mut self, dt: f32, input: Input) {
        let scale = self.dev.time_scale();
        if scale >= 1.0 {
            for _ in 0..scale as u32 {
                self.step(dt, input);
            }
        } else {
            self.step(dt * scale, input);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn fly(game: &mut Game, steps: usize) {
        for n in 0..steps {
            let input = Input {
                thrust: 1.0,
                turn: if n % 90 < 30 { 1.0 } else { 0.0 },
                fire: n % 7 < 3,
                ..Input::default()
            };
            game.step(DT, input);
        }
    }

    /// A compact fingerprint of everything the toggles could touch.
    fn print(game: &Game) -> Vec<f32> {
        let ship = game.player().expect("ship");
        let mut out = vec![
            ship.position.x,
            ship.position.y,
            ship.health,
            ship.shield,
            game.cargo.metal,
            game.cargo.volatiles,
            game.cargo.crystal,
            game.lives as f32,
            game.score as f32,
            game.time,
            game.bodies.len() as f32,
            game.bullets.len() as f32,
        ];
        out.extend(
            game.bodies
                .iter()
                .take(40)
                .map(|b| b.position.x + b.position.y),
        );
        out
    }

    #[test]
    fn the_default_state_is_the_normal_game() {
        let dev = DevState::default();
        assert!(!dev.active());
        assert_eq!(dev.time_scale(), 1.0);
        assert_eq!(Game::new(7).dev, dev);
    }

    #[test]
    fn a_default_dev_state_changes_nothing() {
        let mut plain = Game::new(crate::config::MASTER_SEED);
        let mut touched = Game::new(crate::config::MASTER_SEED);
        // Turn every switch on and back off again: the state returns to default exactly.
        for row in DevRow::ALL.into_iter().take(6) {
            touched.dev_change(row, 0);
        }
        for row in DevRow::ALL.into_iter().take(6) {
            touched.dev_change(row, 0);
        }
        assert_eq!(touched.dev, DevState::default());
        fly(&mut plain, 600);
        fly(&mut touched, 600);
        assert_eq!(print(&plain), print(&touched));
        assert_eq!(plain.state_digest(), touched.state_digest());
        // And the scaled step is the plain step at the normal scale.
        let mut scaled = Game::new(crate::config::MASTER_SEED);
        for n in 0..600 {
            let input = Input {
                thrust: 1.0,
                turn: if n % 90 < 30 { 1.0 } else { 0.0 },
                fire: n % 7 < 3,
                ..Input::default()
            };
            scaled.step_scaled(DT, input);
        }
        assert_eq!(print(&plain), print(&scaled));
        assert_eq!(plain.state_digest(), scaled.state_digest());
    }

    #[test]
    fn active_follows_the_switches_and_the_time_scale() {
        let mut game = Game::new(1);
        assert!(!game.dev.active());
        game.dev_change(DevRow::Invulnerable, 0);
        assert!(game.dev.active());
        game.dev_change(DevRow::Invulnerable, 1);
        assert!(!game.dev.active());
        game.dev_change(DevRow::TimeScale, 1);
        assert!(game.dev.active());
        assert_eq!(game.dev.time_scale(), 2.0);
        game.dev_change(DevRow::TimeScale, -1);
        game.dev_change(DevRow::TimeScale, -1);
        assert_eq!(game.dev.time_scale(), 0.5);
        // Selections alone are not a toggle.
        game.dev_change(DevRow::TimeScale, 1);
        game.dev_change(DevRow::GrantPart, 1);
        game.dev_change(DevRow::Spawn, 1);
        game.dev_change(DevRow::TargetX, 1);
        assert!(!game.dev.active());
    }

    #[test]
    fn the_time_scale_clamps_and_scales_the_step() {
        let mut game = Game::new(1);
        for _ in 0..9 {
            game.dev_change(DevRow::TimeScale, 1);
        }
        assert_eq!(game.dev.time_scale(), 4.0);
        let before = game.time;
        game.step_scaled(DT, Input::default());
        assert!((game.time - before - 4.0 * DT).abs() < 1e-4);
        for _ in 0..9 {
            game.dev_change(DevRow::TimeScale, -1);
        }
        assert_eq!(game.dev.time_scale(), 0.25);
        let before = game.time;
        game.step_scaled(DT, Input::default());
        assert!((game.time - before - 0.25 * DT).abs() < 1e-4);
    }

    #[test]
    fn invulnerability_keeps_the_hull_and_shield_whole() {
        let mut game = Game::new(3);
        game.dev_change(DevRow::Invulnerable, 0);
        game.player_invulnerability = 0.0;
        for body in game
            .bodies
            .iter_mut()
            .filter(|b| b.kind == BodyKind::Player)
        {
            body.health = 1.0;
            body.shield = 0.0;
        }
        let at = game.player().expect("ship").position;
        game.place_creature(&Species::of(Genome::fatso()), at + Vec2::new(60.0, 0.0));
        for _ in 0..240 {
            game.step(DT, Input::default());
        }
        let ship = game.player().expect("ship");
        assert_eq!(ship.health, ship.max_health);
        assert_eq!(ship.shield, ship.max_shield);
        assert_eq!(game.lives, 3);
    }

    #[test]
    fn unlimited_lives_keep_the_count_through_a_death() {
        let mut game = Game::new(3);
        game.dev_change(DevRow::UnlimitedLives, 0);
        for _ in 0..4 {
            game.player_invulnerability = 0.0;
            for body in game
                .bodies
                .iter_mut()
                .filter(|b| b.kind == BodyKind::Player)
            {
                body.health = 0.0;
            }
            game.step(DT, Input::default());
        }
        assert_eq!(game.lives, 3);
        assert!(!game.game_over);
        assert!(game.run.deaths >= 1);
    }

    #[test]
    fn free_purchases_charge_nothing_and_always_afford() {
        let mut game = Game::new(3);
        let price = [(Material::Crystal, 50.0), (Material::Metal, 80.0)];
        assert!(!game.cargo.can_afford(&price));
        game.dev_change(DevRow::FreePurchases, 0);
        assert!(game.cargo.can_afford(&price));
        assert!(game.cargo.spend(&price));
        assert_eq!(game.cargo.total(), 0.0);
        game.dev_change(DevRow::FreePurchases, 0);
        assert!(!game.cargo.spend(&price));
    }

    #[test]
    fn free_purchases_survive_a_restart_and_a_cargo_swap() {
        let mut game = Game::new(3);
        game.dev_change(DevRow::FreePurchases, 0);
        game.reset();
        assert!(game.dev.free_purchases);
        game.step(DT, Input::default());
        assert!(game.cargo.can_afford(&[(Material::Metal, 5.0)]));
        game.cargo = Cargo::default();
        game.step(DT, Input::default());
        assert!(game.cargo.can_afford(&[(Material::Metal, 5.0)]));
    }

    #[test]
    fn infinite_ammo_keeps_the_hold_full_while_firing() {
        let mut game = Game::new(3);
        game.dev_grant_weapons();
        game.dev_change(DevRow::InfiniteAmmo, 0);
        for _ in 0..300 {
            game.step(
                DT,
                Input {
                    fire: true,
                    ..Input::default()
                },
            );
        }
        for kind in Material::ALL {
            assert_eq!(game.cargo.amount(kind), game.cargo.cap(kind));
        }
    }

    #[test]
    fn max_materials_fills_every_material() {
        let mut game = Game::new(3);
        game.dev_change(DevRow::FillHold, 0);
        for kind in Material::ALL {
            assert_eq!(game.cargo.amount(kind), game.cargo.cap(kind));
        }
    }

    #[test]
    fn no_cooldowns_leave_parry_dash_and_ping_ready() {
        let mut game = Game::new(3);
        game.dev_grant_skills();
        game.dev_change(DevRow::NoCooldowns, 0);
        game.parry();
        game.dash(None);
        game.ping();
        game.step(DT, Input::default());
        assert_eq!(game.parry_cooldown(), 0.0);
        assert_eq!(game.dash_cooldown(), 0.0);
        assert_eq!(game.ping_cooldown(), 0.0);
    }

    #[test]
    fn grants_fill_skills_weapons_and_organs() {
        let mut game = Game::new(3);
        game.dev_change(DevRow::GrantSkills, 0);
        game.dev_change(DevRow::GrantWeapons, 0);
        game.dev_change(DevRow::GrantOrgans, 0);
        for skill in Skill::ALL {
            assert_eq!(game.loadout.skills.level(skill), skill.max_level());
        }
        for profile in Profile::ALL {
            assert_eq!(game.loadout.arsenal.level(profile), profile.max_level());
        }
        for organ in Organ::ALL {
            let strain = game.loadout.organs.strain(organ).expect("owned");
            assert_eq!(strain.level, tuning::ORGAN_LEVELS);
        }
        assert!(!game.loadout.organs.fitted().is_empty());
        // The granting leaves the free flag as it found it.
        assert!(!game.cargo.dev_free);
        assert_eq!(game.cargo.total(), 0.0);
    }

    #[test]
    fn a_granted_part_has_the_chosen_rarity_and_is_deterministic() {
        for (n, rarity) in Rarity::ALL.into_iter().enumerate() {
            let mut a = Game::new(5);
            let mut b = Game::new(5);
            for _ in 0..n {
                a.dev_change(DevRow::GrantPart, 1);
                b.dev_change(DevRow::GrantPart, 1);
            }
            assert_eq!(a.dev.part_rarity(), rarity);
            a.dev_change(DevRow::GrantPart, 0);
            b.dev_change(DevRow::GrantPart, 0);
            assert_eq!(a.loadout, b.loadout);
            let note = a.notices.last().expect("a note");
            assert_eq!(note.rarity, rarity);
            assert!(a.notices.last().is_some_and(|n| n.text.starts_with("DEV")));
        }
    }

    #[test]
    fn a_grant_leaves_the_game_streams_alone() {
        let mut plain = Game::new(5);
        let mut granted = Game::new(5);
        granted.dev_change(DevRow::GrantPart, 0);
        let before = granted.loadout.parts.len() + granted.loadout.arsenal.owned().len();
        assert!(before > 1);
        assert_eq!(plain.loot.next_u64(), granted.loot.next_u64());
        assert_eq!(plain.rng.next_u64(), granted.rng.next_u64());
    }

    #[test]
    fn teleport_goes_to_the_middle_of_the_target_sector() {
        let mut game = Game::new(5);
        game.dev_change(DevRow::TargetX, 1);
        game.dev_change(DevRow::TargetX, 1);
        game.dev_change(DevRow::TargetY, -1);
        assert_eq!(game.dev.target, (2, -1));
        game.dev_change(DevRow::Teleport, 0);
        game.step(DT, Input::default());
        assert_eq!(game.sector(), crate::world::SectorId { x: 2, y: -1 });
    }

    #[test]
    fn spawning_places_three_creatures_by_name() {
        let mut game = Game::new(5);
        let before = game.bodies.len();
        for name in SPAWNS {
            let mut fresh = Game::new(5);
            let count = fresh.bodies.len();
            fresh.dev_spawn(name);
            assert!(fresh.bodies.len() >= count + 3, "{name}");
        }
        game.dev_change(DevRow::Spawn, 0);
        assert!(game.bodies.len() >= before + 3);
        assert_eq!(game.dev.spawn_name(), "bogey");
        game.dev_change(DevRow::Spawn, -1);
        assert_eq!(game.dev.spawn_name(), "builder");
    }

    #[test]
    fn the_animal_specimens_spawn_three_whole_bodies_each() {
        for name in crate::bodyplan::SPECIMENS {
            let mut game = Game::new(5);
            let before = game.chains.len();
            game.dev_spawn(name);
            assert_eq!(game.chains.len(), before + 3, "{name}");
            let parts = specimen_genome(name).parts() as usize;
            assert!(game.chains.values().rev().take(3).all(|c| c.len() == parts));
            for _ in 0..300 {
                game.step(DT, Input::default());
            }
        }
    }

    #[test]
    fn frozen_enemies_stay_where_they_are_and_hold_their_fire() {
        let mut game = Game::new(5);
        game.dev_change(DevRow::FreezeEnemies, 0);
        game.dev_change(DevRow::Invulnerable, 0);
        game.dev_spawn("argus");
        let start: Vec<_> = game
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature)
            .map(|b| (b.id, b.position))
            .collect();
        assert!(!start.is_empty());
        for _ in 0..120 {
            game.step(DT, Input::default());
        }
        for (id, at) in start {
            let now = game.body(id).expect("alive").position;
            assert!(
                now.distance(at) < 1.0,
                "creature {id} moved {}",
                now.distance(at)
            );
        }
        assert!(game.bullets.iter().all(|b| b.friendly));
    }

    #[test]
    fn every_row_has_text_and_every_spawn_name_is_known() {
        for row in DevRow::ALL {
            assert!(!row.label().is_empty() && !row.hint().is_empty());
        }
        let default = Genome::default();
        for name in SPAWNS {
            assert!(
                name == "bogey" || specimen_genome(name) != default,
                "{name} has no genome"
            );
        }
    }
}
