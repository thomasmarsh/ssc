//! The epithet on the run summary: a pure function from what the run did to a silly title,
//! with a fixed priority order. Nothing here changes the rules.
//!
//! Order (the first that holds wins): Apex Hunter; Scourge of the REGION (extirpations, the
//! region the run spent most time in); Genocidal Prospector (an extirpation and a lot of ore);
//! Friend of the CIVILIZATION; Tithe Payer; Parry Dancer; Blink Addict; Gentle Cartographer
//! (wide travel, few kills); Reckless Wreck (all ships lost fast); Pacifist Prospector (ore,
//! hardly a kill); and the fallback, Wandering Hazard. Ties among regions or friends go to the
//! longer stay or the warmer regard, then to the lower key.

use super::*;

/// What the title is chosen from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TitleFacts {
    pub kills: u32,
    pub extirpated: usize,
    pub mined: f32,
    pub sectors: usize,
    pub regions: usize,
    pub deaths: u32,
    pub seconds: f32,
    pub perfect_parries: u32,
    pub dashes: u32,
    pub tithes: u32,
    pub apex_slain: usize,
    /// The region the ship spent the most time in.
    pub top_region: Option<String>,
    /// The warmest civilization that is friendly now.
    pub friend: Option<String>,
}

/// Capitalizes each word: "WEORIA STEPPE" becomes "Weoria Steppe".
pub fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The epithet for a run.
pub fn title(f: &TitleFacts, tune: &Tunables) -> String {
    if f.apex_slain >= 1 {
        return "Apex Hunter".into();
    }
    if f.extirpated >= tune.title_scourge_extirpations
        && let Some(region) = &f.top_region
    {
        return format!("Scourge of the {region}");
    }
    if f.extirpated >= 1 && f.mined >= tune.title_prospect_mined {
        return "Genocidal Prospector".into();
    }
    if let Some(friend) = &f.friend {
        return format!("Friend of the {friend}");
    }
    if f.tithes >= tune.title_tithes {
        return "Tithe Payer".into();
    }
    if f.perfect_parries >= tune.title_parries {
        return "Parry Dancer".into();
    }
    if f.dashes >= tune.title_dashes {
        return "Blink Addict".into();
    }
    if f.sectors >= tune.title_cartographer_sectors
        && f.regions >= tune.title_cartographer_regions
        && f.kills * tune.title_gentle_kills_per_sector <= f.sectors as u32
    {
        return "Gentle Cartographer".into();
    }
    if f.deaths >= tune.title_reckless_deaths && f.seconds < tune.title_reckless_seconds {
        return "Reckless Wreck".into();
    }
    if f.mined >= tune.title_hermit_mined && f.kills <= tune.title_hermit_kills {
        return "Pacifist Prospector".into();
    }
    "Wandering Hazard".into()
}

impl Game {
    /// The facts of the run so far.
    pub fn title_facts(&self) -> TitleFacts {
        let r = &self.run;
        let top_region = r
            .region_time
            .iter()
            .max_by(|a, b| a.1.1.total_cmp(&b.1.1).then(b.0.cmp(a.0)))
            .map(|(_, (name, _))| title_case(name));
        let friend = self
            .civs
            .regard
            .iter()
            .filter(|(id, reg)| {
                reg.tier == Tier::Friendly
                    && self.civ_standing(**id) != crate::territory::Standing::Fallen
            })
            .max_by(|a, b| a.1.value.total_cmp(&b.1.value).then(b.0.cmp(a.0)))
            .and_then(|(id, _)| self.civs.territories.get(id))
            .map(|t| title_case(&t.name(self.seed)));
        TitleFacts {
            kills: r.kills,
            extirpated: r.extirpated.len(),
            mined: r.total_mined(),
            sectors: r.sectors.len(),
            regions: r.regions.len(),
            deaths: r.deaths,
            seconds: self.time,
            perfect_parries: r.perfect_parries,
            dashes: r.dashes,
            tithes: r.tithes,
            apex_slain: r.apex_slain.len(),
            top_region,
            friend,
        }
    }

    /// The epithet of this run.
    pub fn run_title(&self) -> String {
        title(&self.title_facts(), &self.tune)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::{DT, empty_game};

    fn facts() -> TitleFacts {
        TitleFacts {
            kills: 50,
            sectors: 4,
            regions: 1,
            seconds: 900.0,
            ..TitleFacts::default()
        }
    }

    #[test]
    fn every_title_has_its_own_trigger_and_the_fallback_is_the_base() {
        assert_eq!(title(&facts(), &DEFAULT_TUNING), "Wandering Hazard");
        let with = |f: &dyn Fn(&mut TitleFacts)| {
            let mut x = facts();
            f(&mut x);
            title(&x, &DEFAULT_TUNING)
        };
        assert_eq!(with(&|f| f.apex_slain = 1), "Apex Hunter");
        assert_eq!(
            with(&|f| {
                f.extirpated = 2;
                f.top_region = Some("Weoria Steppe".into());
            }),
            "Scourge of the Weoria Steppe"
        );
        assert_eq!(
            with(&|f| {
                f.extirpated = 1;
                f.mined = 200.0;
            }),
            "Genocidal Prospector"
        );
        assert_eq!(
            with(&|f| f.friend = Some("Kraz Outpost".into())),
            "Friend of the Kraz Outpost"
        );
        assert_eq!(with(&|f| f.tithes = 5), "Tithe Payer");
        assert_eq!(with(&|f| f.perfect_parries = 10), "Parry Dancer");
        assert_eq!(with(&|f| f.dashes = 40), "Blink Addict");
        assert_eq!(
            with(&|f| {
                f.sectors = 20;
                f.regions = 5;
                f.kills = 4;
            }),
            "Gentle Cartographer"
        );
        assert_eq!(
            with(&|f| {
                f.deaths = 3;
                f.seconds = 120.0;
            }),
            "Reckless Wreck"
        );
        assert_eq!(
            with(&|f| {
                f.mined = 400.0;
                f.kills = 3;
            }),
            "Pacifist Prospector"
        );
    }

    #[test]
    fn near_misses_do_not_earn_a_title() {
        let with = |f: &dyn Fn(&mut TitleFacts)| {
            let mut x = facts();
            f(&mut x);
            title(&x, &DEFAULT_TUNING)
        };
        // One extirpation alone, or two with no region, is not a scourge.
        assert_eq!(with(&|f| f.extirpated = 1), "Wandering Hazard");
        assert_eq!(with(&|f| f.extirpated = 2), "Wandering Hazard");
        assert_eq!(with(&|f| f.tithes = 4), "Wandering Hazard");
        assert_eq!(with(&|f| f.perfect_parries = 9), "Wandering Hazard");
        assert_eq!(
            with(&|f| {
                f.deaths = 3;
                f.seconds = 900.0;
            }),
            "Wandering Hazard",
            "a slow death is not reckless"
        );
        assert_eq!(
            with(&|f| {
                f.sectors = 20;
                f.regions = 5;
                f.kills = 8;
            }),
            "Wandering Hazard",
            "too many kills to be gentle"
        );
    }

    #[test]
    fn priority_decides_between_titles_that_both_hold() {
        let mut f = facts();
        f.apex_slain = 2;
        f.tithes = 9;
        f.perfect_parries = 30;
        f.friend = Some("X".into());
        assert_eq!(title(&f, &DEFAULT_TUNING), "Apex Hunter");
        f.apex_slain = 0;
        assert_eq!(title(&f, &DEFAULT_TUNING), "Friend of the X");
        f.friend = None;
        assert_eq!(title(&f, &DEFAULT_TUNING), "Tithe Payer");
        f.tithes = 0;
        assert_eq!(title(&f, &DEFAULT_TUNING), "Parry Dancer");
        // Same facts, same title.
        assert_eq!(
            title(&f, &DEFAULT_TUNING),
            title(&f.clone(), &DEFAULT_TUNING)
        );
    }

    #[test]
    fn titles_are_title_cased() {
        assert_eq!(title_case("VRORRDXORK OUTPOST"), "Vrorrdxork Outpost");
        assert_eq!(title_case("weoria steppe"), "Weoria Steppe");
    }

    #[test]
    fn the_game_gathers_the_facts_and_breaks_ties_by_time_then_key() {
        let mut game = empty_game();
        game.step(DT, Input::default());
        assert_eq!(game.run_title(), "Wandering Hazard");
        // Most time wins; an exact tie goes to the lower key.
        game.run.region_time.insert(9, ("Late Reach".into(), 40.0));
        game.run.region_time.insert(3, ("Early Reach".into(), 40.0));
        game.run.region_time.insert(5, ("Short Reach".into(), 10.0));
        assert_eq!(
            game.title_facts().top_region.as_deref(),
            Some("Early Reach")
        );
        game.run.region_time.insert(9, ("Late Reach".into(), 41.0));
        assert_eq!(game.title_facts().top_region.as_deref(), Some("Late Reach"));
        // Extirpations and the region make a scourge.
        for lineage in 1..=2 {
            game.run.extirpated.push(run::Extirpation {
                name: format!("S{lineage}"),
                lineage,
                sectors: 1,
                at: SectorId::ORIGIN,
                depth: 0.0,
            });
        }
        assert_eq!(game.run_title(), "Scourge of the Late Reach");
        assert!(game.run_report().title.contains("Scourge"));
    }

    #[test]
    fn a_friendly_civilization_makes_a_friend_and_the_warmest_wins() {
        let a = crate::territory::outpost(crate::config::MASTER_SEED);
        let mut game = Game::new(crate::config::MASTER_SEED);
        game.register_territory(a);
        game.set_regard(a.id, 80.0);
        game.step(DT, Input::default());
        let facts = game.title_facts();
        let name = title_case(&a.name(crate::config::MASTER_SEED));
        assert_eq!(facts.friend.as_deref(), Some(name.as_str()));
        assert_eq!(game.run_title(), format!("Friend of the {name}"));
        // A fallen civilization is no friend.
        game.civs.fall.insert(
            a.id,
            crate::territory::Fall {
                capital: true,
                elder: false,
            },
        );
        assert_eq!(game.title_facts().friend, None);
    }

    #[test]
    fn the_counters_behind_the_titles_are_counted_where_they_happen() {
        let mut game = empty_game();
        game.step(DT, Input::default());
        assert_eq!((game.run.perfect_parries, game.run.dashes), (0, 0));
        game.run.dashes += 1;
        game.reset();
        assert_eq!(game.run.dashes, 0);
        assert!(game.run.region_time.is_empty());
    }
}
