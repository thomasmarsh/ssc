//! The region the ship is in, announced when it changes. The region of a sector is a pure
//! function (`region::region`); this module only decides when the ship has really moved from
//! one to the next, so a border does not flicker: a new region must hold the ship for
//! `region_hold` seconds and at least `region_cooldown` must have passed since the last
//! banner. Entering posts an `ENTERING` notice in the style of the extirpation one, and
//! counts toward the run's regions explored. A civilization's territory is announced by its
//! own notice (see `civ`), so it is only recorded here.

use super::*;
use crate::region::{Region, RegionKind, region};

/// Where the region tracking stands.
#[derive(Clone, Default)]
pub struct RegionState {
    /// The sector the region was last computed for, and the region there.
    here: Option<(SectorId, Region)>,
    /// The region the ship has announced.
    current: Option<Region>,
    /// A region the ship is in but has not yet held long enough: its key and the seconds held.
    candidate: Option<(u64, f32)>,
    /// Seconds since the last banner.
    since_banner: f32,
    /// Game time the current region was committed to (the HUD fades its tag from here).
    entered: f32,
    /// The sector whose area readout was last announced (slice K3).
    area_sector: Option<SectorId>,
}

/// The state digest hashes this, and the announcement memo is presentation only (it only
/// decides when a banner posts), so it stays out of the text.
impl std::fmt::Debug for RegionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegionState")
            .field("here", &self.here)
            .field("current", &self.current)
            .field("candidate", &self.candidate)
            .field("since_banner", &self.since_banner)
            .field("entered", &self.entered)
            .finish()
    }
}

impl Game {
    /// Per tick: notices a change of region, with hysteresis.
    pub(super) fn update_region(&mut self, dt: f32) {
        let sector = self.sector();
        if self.region.here.as_ref().map(|h| h.0) != Some(sector) {
            let r = region(self.seed, sector);
            self.region.here = Some((sector, r));
        }
        self.region.since_banner += dt;
        self.announce_area(sector);
        if let Some(current) = self.region.current.as_ref() {
            self.run
                .region_time
                .entry(current.key)
                .or_insert_with(|| (current.name.clone(), 0.0))
                .1 += dt;
        }
        let Some((_, here)) = self.region.here.clone() else {
            return;
        };
        let Some(current) = self.region.current.as_ref() else {
            self.enter_region(here, false);
            return;
        };
        if current.key == here.key {
            self.region.candidate = None;
            return;
        }
        let held = match self.region.candidate {
            Some((key, held)) if key == here.key => held + dt,
            _ => dt,
        };
        self.region.candidate = Some((here.key, held));
        if held >= self.tune.region_hold && self.region.since_banner >= self.tune.region_cooldown {
            self.enter_region(here, true);
        }
    }

    /// On entering a sector, posts what it asks of this build when that is worth saying: the
    /// level and verdict, the first missing answer in plain words, and a burst warning
    /// (`docs/CAPABILITIES.md` 3.4). An unremarkable area is not announced.
    fn announce_area(&mut self, sector: SectorId) {
        if self.region.area_sector == Some(sector) {
            return;
        }
        self.region.area_sector = Some(sector);
        let read = self.area_readout(sector);
        if !read.notable() {
            return;
        }
        let rarity = if read.mood() >= crate::readout::Mood::Danger {
            upgrades::Rarity::Epic
        } else {
            upgrades::Rarity::Rare
        };
        self.notify(format!("AREA  {}", read.headline_short()), rarity);
        // One more line, the most useful one; the rest lives on the HUD tag and the map.
        let more = read
            .missing()
            .next()
            .map(|n| n.text.clone())
            .or_else(|| read.burst.as_ref().map(|b| b.text()))
            .or_else(|| read.skirt_text());
        if let Some(text) = more {
            self.notify(text, rarity);
        }
    }

    /// Commits to a region: records it in the run and posts the banner.
    fn enter_region(&mut self, entered: Region, mid_run: bool) {
        self.run.regions.insert(entered.key);
        self.region.candidate = None;
        self.region.since_banner = if mid_run {
            0.0
        } else {
            self.tune.region_cooldown
        };
        if entered.kind != RegionKind::Civ {
            let text = format!("ENTERING  {}", entered.name.to_uppercase());
            self.notify(text, upgrades::Rarity::Epic);
        }
        self.region.current = Some(entered);
        self.region.entered = self.time;
    }

    /// Game time the announced region was entered.
    pub(super) fn region_entered(&self) -> f32 {
        self.region.entered
    }

    /// The region the ship is announced to be in (none before the first tick).
    pub fn region(&self) -> Option<&Region> {
        self.region.current.as_ref()
    }

    /// The name of the region of any sector (for the chart).
    pub fn region_of(&self, sector: SectorId) -> Region {
        region(self.seed, sector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, set_player};

    /// Two adjacent sectors of different, non-civilization regions away from HOME.
    fn border(seed: u64) -> (SectorId, SectorId) {
        for x in 4..30 {
            for y in -8..8 {
                let (a, b) = (SectorId { x, y }, SectorId { x: x + 1, y });
                let (ra, rb) = (region(seed, a), region(seed, b));
                if ra.key != rb.key && ra.kind != RegionKind::Civ && rb.kind != RegionKind::Civ {
                    return (a, b);
                }
            }
        }
        panic!("no region border");
    }

    fn banners(game: &Game) -> usize {
        game.notices
            .iter()
            .filter(|n| n.text.starts_with("ENTERING"))
            .count()
    }

    /// Holds the ship at `at` for `seconds`, returning how often the announced region changed.
    fn stay(game: &mut Game, at: Vec2, seconds: f32) -> usize {
        let mut changes = 0;
        let mut last = game.region().map(|r| r.key);
        for _ in 0..(seconds / 0.05) as usize {
            set_player(game, at, Vec2::ZERO);
            game.step(0.05, Input::default());
            let now = game.region().map(|r| r.key);
            changes += usize::from(now != last);
            last = now;
        }
        changes
    }

    #[test]
    fn the_start_is_announced_and_counted_and_a_new_region_needs_a_steady_stay() {
        let seed = 42;
        let (a, b) = border(seed);
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.step(DT, Input::default());
        assert_eq!(game.region().unwrap().name, "Homestead");
        assert_eq!(banners(&game), 1, "the start is announced like any region");
        assert_eq!(game.run.regions.len(), 1);
        // A brief visit (under region_hold) announces nothing.
        game.teleport(a.center());
        assert_eq!(
            stay(&mut game, a.center(), DEFAULT_TUNING.region_hold - 1.0),
            0
        );
        assert_eq!(game.region().unwrap().name, "Homestead");
        // A steady stay does.
        assert_eq!(stay(&mut game, a.center(), 2.0), 1);
        assert_eq!(game.region().unwrap().key, region(seed, a).key);
        assert!(
            game.notices.iter().any(|n| {
                n.text == format!("ENTERING  {}", region(seed, a).name.to_uppercase())
            })
        );
        assert_eq!(game.run.regions.len(), 2);
        assert!(game.run_report().lines[1].contains("REGIONS 2"));
        // The next region must also wait out the cooldown since that banner.
        let before = banners(&game);
        assert_eq!(
            stay(&mut game, b.center(), DEFAULT_TUNING.region_hold + 2.0),
            0
        );
        assert_eq!(banners(&game), before, "too soon after the last banner");
        assert_eq!(
            stay(&mut game, b.center(), DEFAULT_TUNING.region_cooldown),
            1
        );
        assert_eq!(game.region().unwrap().key, region(seed, b).key);
        assert_eq!(game.run.regions.len(), 3);
        // The time in each region adds up to the time played.
        let spent: f32 = game.run.region_time.values().map(|(_, s)| s).sum();
        assert!((spent - game.time).abs() < 0.5, "{spent} of {}", game.time);
        assert!(game.run.region_time.len() >= 3);
    }

    #[test]
    fn flying_along_a_border_does_not_flicker_the_banner() {
        let seed = 42;
        let (a, b) = border(seed);
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(a.center());
        stay(&mut game, a.center(), DEFAULT_TUNING.region_hold + 1.0);
        let mut changes = 0;
        // Two minutes of crossing back and forth every second.
        for k in 0..120 {
            let at = if k % 2 == 0 { b.center() } else { a.center() };
            changes += stay(&mut game, at, 1.0);
        }
        assert_eq!(changes, 0, "never held long enough to switch");
        // Lingering on the far side then back and forth every 4 seconds: a banner at most
        // every region_cooldown seconds.
        let mut changes = 0;
        for k in 0..30 {
            let at = if k % 2 == 0 { b.center() } else { a.center() };
            changes += stay(&mut game, at, 4.0);
        }
        assert!(
            changes as f32 <= 120.0 / DEFAULT_TUNING.region_cooldown + 1.0,
            "{changes} banners"
        );
        assert!(changes >= 2, "it does switch when the stays are real");
        // The count of regions entered is distinct regions, not banners.
        assert!(game.run.regions.len() <= 3);
    }

    #[test]
    fn a_territory_is_recorded_but_announced_by_its_own_notice() {
        let seed = crate::config::MASTER_SEED;
        let t = crate::territory::outpost(seed);
        let mut game = Game::new(seed);
        game.player_invulnerability = 1e9;
        game.teleport(t.capital.center());
        stay(
            &mut game,
            t.capital.center(),
            DEFAULT_TUNING.region_hold + 1.0,
        );
        let r = game.region().unwrap();
        assert_eq!(r.kind, RegionKind::Civ);
        assert_eq!(r.key, t.id);
        assert!(
            game.notices.iter().all(|n| !n.text.starts_with("ENTERING")
                || n.text.contains("OUTPOST")
                || n.text.contains("HOMESTEAD")),
            "no second banner for a territory"
        );
        assert!(game.run.regions.contains(&t.id));
    }

    #[test]
    fn any_charted_sector_can_be_named_for_the_chart() {
        let game = Game::new(7);
        let id = SectorId { x: 9, y: 4 };
        assert_eq!(game.region_of(id), region(7, id));
        assert_eq!(game.region_of(SectorId::ORIGIN).name, "Homestead");
    }
}
