//! The geometric HUD's view-model: everything the always-visible display needs, derived from the
//! game as plain numbers (fractions, counts, states) so the adapter only has to draw them. The
//! functions here are pure and tested; nothing in the rules reads them.

use super::skills::Skill;
use super::tuning as t;
use super::{BodyKind, Game, Material, Tier, arsenal::Profile};
use crate::world::Standing;

/// The hull ring is cut into this many segments so damage is countable at a glance.
pub const HULL_SEGMENTS: u8 = 10;
/// Five threat pips.
pub const THREAT_PIPS: u8 = 5;
/// A hull or shield below this fraction is "low" (the ring flashes).
pub const LOW: f32 = 0.3;
/// Seconds a region name stays at full strength after entering, then the seconds it fades over.
pub const REGION_SHOW: f32 = 6.0;
pub const REGION_FADE: f32 = 2.0;
/// The region tag never fades below this (a quiet reminder of where the ship is).
pub const REGION_FLOOR: f32 = 0.3;
/// Hostiles this close count as pressure, and alert ones count from farther out.
pub const PRESSURE_NEAR: f32 = 1200.0;
pub const PRESSURE_HUNTING: f32 = 2000.0;

/// How lit one hull segment is (0 to 1) when the hull stands at `fraction` of its maximum.
/// Segments drain from the last one backwards, and a hurt hull always shows at least a sliver
/// on its last lit segment, so a hull above zero never reads as empty.
pub fn segment_fill(fraction: f32, index: u8) -> f32 {
    let lit = fraction.clamp(0.0, 1.0) * f32::from(HULL_SEGMENTS);
    (lit - f32::from(index)).clamp(0.0, 1.0)
}

/// Number of hull segments with any light in them.
pub fn segments_lit(fraction: f32) -> u8 {
    (0..HULL_SEGMENTS)
        .filter(|i| segment_fill(fraction, *i) > 0.0)
        .count() as u8
}

/// Which way a ring reads: full health, hurt, or in danger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    Good,
    Hurt,
    Low,
}

impl Health {
    pub fn of(fraction: f32) -> Self {
        if fraction < LOW {
            Self::Low
        } else if fraction < 0.6 {
            Self::Hurt
        } else {
            Self::Good
        }
    }
}

/// The three ability rings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    Parry,
    Dash,
    Ping,
}

impl Ability {
    pub const ALL: [Ability; 3] = [Ability::Parry, Ability::Dash, Ability::Ping];

    /// The key glyph drawn in the ring.
    pub fn key(self) -> &'static str {
        match self {
            Self::Parry => "D",
            Self::Dash => "SH",
            Self::Ping => "X",
        }
    }
}

/// What an ability ring shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingState {
    /// Not bought yet: a lock.
    Locked,
    /// Ready to use: a full, glowing ring.
    Ready,
    /// Recovering: an arc that fills.
    Cooling,
    /// In effect now (the parry arc is up).
    Active,
    /// Ready except for the shield it costs.
    NoEnergy,
    /// Jammed: greyed, with static, for the seconds in `left`.
    Jammed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbilityRing {
    pub ability: Ability,
    pub state: RingState,
    /// Bought but never used yet: the ring wears a NEW tag.
    pub fresh: bool,
    /// How recovered it is, 0 to 1 (one when ready).
    pub fill: f32,
    /// Seconds until ready (zero when ready).
    pub left: f32,
}

/// The state of an ability from its raw numbers: `level` of the skill (zero is locked),
/// seconds of `cooldown` left out of `total`, whether it is `active` now and whether the ship
/// has the shield it costs (`affordable`).
pub fn ability_ring(
    ability: Ability,
    level: u8,
    cooldown: f32,
    total: f32,
    active: bool,
    affordable: bool,
) -> AbilityRing {
    let fill = if total > 0.0 {
        (1.0 - cooldown / total).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let (state, fill) = if level == 0 {
        (RingState::Locked, 0.0)
    } else if active {
        (RingState::Active, 1.0)
    } else if cooldown > 0.0 {
        (RingState::Cooling, fill)
    } else if !affordable {
        (RingState::NoEnergy, 1.0)
    } else {
        (RingState::Ready, 1.0)
    };
    AbilityRing {
        ability,
        state,
        fresh: false,
        fill,
        left: cooldown.max(0.0),
    }
}

/// The weapon icon: the active profile, its level, the fuel arc and how many are owned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponIcon {
    pub profile: Profile,
    pub level: u8,
    /// The material it burns, and how full that hold is (1 for the free stock gun).
    pub material: Option<Material>,
    pub fuel: f32,
    pub dry: bool,
    /// Jammed: the gun is greyed with static.
    pub jammed: bool,
    /// Owned profiles, and the active one's position among them.
    pub owned: u8,
    pub index: u8,
}

/// A material pip: how full its hold is, and the amount for the small secondary number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CargoPip {
    pub material: Material,
    pub fill: f32,
    pub amount: f32,
    pub full: bool,
}

/// The standing meter shown near a civilization.
#[derive(Clone, Debug, PartialEq)]
pub struct StandingMeter {
    pub name: String,
    pub tier: Tier,
    /// Regard from hostile (0) to friendly (1).
    pub fraction: f32,
    pub regard: f32,
    pub color: [f32; 3],
    pub fallen: bool,
}

/// One organ in the bottom cluster: its kind, level and whether it works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrganIcon {
    pub organ: super::organs::Organ,
    pub level: u8,
    pub state: OrganState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrganState {
    /// Fitted and awake.
    Active,
    /// Fitted but the hold is dry.
    Asleep,
    /// A fresh bond working without a slot: the share of its time left.
    Bond(f32),
}

/// How many threat pips light: none with nothing around, else the verdict ladder (strong 1,
/// even 2 to 3, underpowered 4, outclassed 5), one more when several hostiles are hunting.
pub fn threat_pips(power: f32, threat: f32, near: usize, hunting: usize) -> u8 {
    if near == 0 && hunting == 0 {
        return 0;
    }
    let ratio = power / threat.max(0.01).powf(0.8);
    let base = if ratio < 0.6 {
        5
    } else if ratio < 0.85 {
        4
    } else if ratio < 1.3 {
        if ratio < 1.05 { 3 } else { 2 }
    } else {
        1
    };
    let crowd = u8::from(hunting >= 4);
    (base + crowd).min(THREAT_PIPS)
}

/// The opacity of the region tag: full while it is new, then fading to a quiet floor.
pub fn region_alpha(age: f32) -> f32 {
    if age <= REGION_SHOW {
        1.0
    } else {
        let fade = ((age - REGION_SHOW) / REGION_FADE).clamp(0.0, 1.0);
        1.0 - fade * (1.0 - REGION_FLOOR)
    }
}

/// One entry of the context hint line: a key and what it does right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub key: &'static str,
    pub action: String,
}

fn hint(key: &'static str, action: impl Into<String>) -> Hint {
    Hint {
        key,
        action: action.into(),
    }
}

/// The keys that matter right now, most relevant first: what the ship can do where it is (land,
/// tithe, bench) ahead of the standing verbs (fire, mine, parry, dash, ping). Never more than
/// `MAX_HINTS`; the full list lives behind the help key.
pub const MAX_HINTS: usize = 5;

impl Game {
    pub fn context_hints(&self) -> Vec<Hint> {
        let mut out = Vec::new();
        if self.game_over {
            out.push(hint("ENTER", "launch again"));
            return out;
        }
        if self.bench_open() {
            out.push(hint("UP DOWN", "row"));
            out.push(hint("LEFT RIGHT", "tab"));
            out.push(hint("ENTER", "buy"));
            out.push(hint("E", "close bench"));
            return out;
        }
        if self.latched_cord().is_some() {
            out.push(hint("SPACE", "shoot the cord"));
        }
        // What the interact key does is the prompt over the ship; the line keeps to the verbs.
        if self.is_landed() {
            out.push(hint("E", "bench"));
            out.push(hint("UP", "lift off"));
        } else {
            out.push(hint("SPACE", "fire"));
            out.push(hint("M", "mine"));
            if self.parry_unlocked() {
                out.push(hint("D", "parry"));
            }
            if self.dash_unlocked() {
                out.push(hint("SHIFT", "dash"));
            }
            out.push(hint("X", "ping"));
        }
        out.truncate(MAX_HINTS);
        out
    }
}

/// Everything the HUD draws, for one frame.
#[derive(Clone, Debug)]
pub struct HudModel {
    pub hull: f32,
    pub hull_max: f32,
    pub hull_fraction: f32,
    pub shield: f32,
    pub shield_max: f32,
    pub shield_fraction: f32,
    pub abilities: [AbilityRing; 3],
    pub weapon: WeaponIcon,
    pub cargo: [CargoPip; 3],
    pub threat: u8,
    pub lives: u32,
    pub score: u64,
    /// Score multiplier and the share of the chain's window left, while a chain runs.
    pub streak: Option<(f32, f32)>,
    pub sector: (i32, i32),
    pub region: String,
    pub region_alpha: f32,
    /// The territory the ship is in, as a meter.
    pub standing: Option<StandingMeter>,
    /// A graze boost is running: stacks and the share of its time left.
    pub boost: (u8, f32),
    /// The ship is mid-repair (auto or at a pad).
    pub repairing: bool,
    /// Seconds since the ship last took damage or spent shield.
    pub calm: f32,
    /// Where recent hits came from: (angle from the ship, share of the mark's time left).
    pub hurts: Vec<(f32, f32)>,
    /// The next-lure marker, if any.
    pub lure: Option<super::lure::Lure>,
    /// Jams, confusion and the glitch.
    pub jam: super::JamView,
    /// The organs that are fitted or on a bond's loan.
    pub organs: Vec<OrganIcon>,
    /// Worms on the hull, and how fat the fattest is (0 to 1).
    pub worms: (usize, f32),
    /// The realm the ship is announced to be in (none before the first tick).
    pub realm: Option<RealmTag>,
}

/// The realm tag of the HUD: the name, the kind, its colour and the axes it tests.
#[derive(Clone, Debug, PartialEq)]
pub struct RealmTag {
    pub name: String,
    pub title: &'static str,
    pub tint: [f32; 3],
    /// Primary axes first, then the mild one (see `mild`).
    pub axes: Vec<crate::realm::Axis>,
    /// How many of `axes` are primary (the rest, at most one, is mild).
    pub primary: usize,
    /// No stress here: the starter, a rest realm, or the edge of a realm.
    pub gentle: bool,
    pub alpha: f32,
}

impl Game {
    /// Hostile creatures near the ship: (any within `PRESSURE_NEAR`, alert within
    /// `PRESSURE_HUNTING`).
    pub fn pressure(&self) -> (usize, usize) {
        let Some(ship) = self.player().map(|p| p.position) else {
            return (0, 0);
        };
        let (mut near, mut hunting) = (0, 0);
        for body in self
            .bodies
            .iter()
            .filter(|b| b.active && b.kind == BodyKind::Creature && !b.follower)
        {
            // A civilization that leaves the ship be is no pressure at all.
            if self.civ_of(body).is_some_and(|(tid, _)| self.civ_calm(tid)) {
                continue;
            }
            if self.disguise(body).is_some() {
                continue;
            }
            let d = body.position.distance(ship);
            if body.alert && d < PRESSURE_HUNTING {
                hunting += 1;
            } else if d < PRESSURE_NEAR {
                near += 1;
            }
        }
        (near, hunting)
    }

    /// The threat pips for the current surroundings.
    pub fn threat_pips(&self) -> u8 {
        let (near, hunting) = self.pressure();
        threat_pips(self.power(), self.threat(), near, hunting)
    }

    /// The chain's multiplier and the share of its window left, while one runs.
    pub fn streak_view(&self) -> Option<(f32, f32)> {
        self.streak.view()
    }

    /// How long the current region has been announced, in seconds.
    pub fn region_age(&self) -> f32 {
        self.time - self.region_entered()
    }

    /// The HUD's view-model.
    pub fn hud(&self) -> HudModel {
        let (hull, hull_max, shield, shield_max) =
            self.player().map_or((0.0, 1.0, 0.0, 1.0), |s| {
                (
                    s.health.max(0.0),
                    s.max_health.max(1.0),
                    s.shield,
                    s.max_shield,
                )
            });
        let frac = |value: f32, max: f32| {
            if max > 0.0 {
                (value / max).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let skills = &self.loadout.skills;
        let abilities = [
            ability_ring(
                Ability::Parry,
                skills.level(Skill::Parry),
                self.parry_cooldown(),
                skills.parry_cooldown(),
                self.parry_active(),
                shield >= t::PARRY_COST,
            ),
            ability_ring(
                Ability::Dash,
                skills.level(Skill::Dash),
                self.dash_cooldown(),
                skills.dash_cooldown(),
                false,
                shield >= t::DASH_COST,
            ),
            // Pinging is free: it is only ever cooling or ready.
            ability_ring(
                Ability::Ping,
                1,
                self.ping_cooldown(),
                self.ping_recharge(),
                false,
                true,
            ),
        ];
        let mut abilities = abilities;
        for (i, ring) in abilities.iter_mut().take(2).enumerate() {
            ring.fresh = ring.state != RingState::Locked && !self.feel.used[i];
        }
        let jam = self.jam_view();
        for (ring, left) in abilities.iter_mut().zip([jam.parry, jam.dash]) {
            if left > 0.0 && ring.state != RingState::Locked {
                ring.state = RingState::Jammed;
                ring.left = left;
            }
        }
        let arsenal = &self.loadout.arsenal;
        let profile = arsenal.active;
        let owned = arsenal.owned();
        let material = profile.material();
        let weapon = WeaponIcon {
            profile,
            level: arsenal.level(profile),
            material,
            fuel: material.map_or(1.0, |m| self.cargo.fraction(m)),
            dry: !self.usable(profile),
            jammed: self.jammed(super::JamSystem::Weapons),
            owned: owned.len() as u8,
            index: owned.iter().position(|p| *p == profile).unwrap_or(0) as u8,
        };
        let cargo = Material::ALL.map(|material| CargoPip {
            material,
            fill: self.cargo.fraction(material),
            amount: self.cargo.amount(material),
            full: self.cargo.room(material) <= 0.5,
        });
        let standing = self.territory_report().map(|r| StandingMeter {
            name: r.name,
            tier: r.tier,
            fraction: ((r.regard + 100.0) / 200.0).clamp(0.0, 1.0),
            regard: r.regard,
            color: r.color,
            fallen: r.standing == Standing::Fallen,
        });
        let sector = self.sector();
        HudModel {
            hull,
            hull_max,
            hull_fraction: frac(hull, hull_max),
            shield,
            shield_max,
            shield_fraction: frac(shield, shield_max),
            abilities,
            weapon,
            cargo,
            threat: self.threat_pips(),
            lives: self.lives,
            score: self.score,
            streak: self.streak_view(),
            sector: (sector.x, sector.y),
            region: self
                .region()
                .map_or(String::new(), |r| r.name.to_uppercase()),
            region_alpha: region_alpha(self.region_age()),
            standing,
            boost: self.dash_boost(),
            repairing: self.is_repairing(),
            calm: self.player().map_or(0.0, |s| s.since_hit),
            hurts: self.hurt_marks(),
            lure: self.next_lure(),
            jam,
            organs: self.organ_icons(),
            realm: self.realm_tag(),
            worms: (
                self.latches().len(),
                self.latches()
                    .iter()
                    .map(|l| (l.fed / 120.0).clamp(0.0, 1.0))
                    .fold(0.0, f32::max),
            ),
        }
    }

    /// The organs to draw: fitted ones (asleep when the hold is dry) and a bond's loan.
    pub fn organ_icons(&self) -> Vec<OrganIcon> {
        let organs = &self.loadout.organs;
        let mut icons: Vec<OrganIcon> = organs
            .fitted()
            .iter()
            .filter_map(|&organ| {
                organs.strain(organ).map(|s| OrganIcon {
                    organ,
                    level: s.level,
                    state: if organs.dormant {
                        OrganState::Asleep
                    } else {
                        OrganState::Active
                    },
                })
            })
            .collect();
        if let Some((organ, left)) = organs.loan()
            && !organs.is_fitted(organ)
            && let Some(s) = organs.strain(organ)
        {
            icons.push(OrganIcon {
                organ,
                level: s.level,
                state: OrganState::Bond((left / t::BOND_LOAN).clamp(0.0, 1.0)),
            });
        }
        icons
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::Input;
    use crate::simulation::tests::{DT, empty_game};
    use bevy::prelude::Vec2;

    #[test]
    fn segments_drain_from_the_end_and_a_sliver_still_shows() {
        assert_eq!(segments_lit(1.0), 10);
        assert_eq!(segments_lit(0.0), 0);
        assert_eq!(segments_lit(0.01), 1);
        assert_eq!(segments_lit(0.55), 6);
        assert!((segment_fill(0.55, 5) - 0.5).abs() < 1e-4);
        assert_eq!(segment_fill(0.55, 6), 0.0);
        assert_eq!(segment_fill(0.55, 0), 1.0);
        // Out of range fractions clamp.
        assert_eq!(segments_lit(3.0), 10);
        assert_eq!(segments_lit(-1.0), 0);
    }

    #[test]
    fn health_reads_good_hurt_or_low() {
        assert_eq!(Health::of(1.0), Health::Good);
        assert_eq!(Health::of(0.5), Health::Hurt);
        assert_eq!(Health::of(0.1), Health::Low);
    }

    #[test]
    fn an_ability_ring_is_locked_cooling_ready_active_or_short_of_energy() {
        let ring = |level, cd, active, ok| ability_ring(Ability::Dash, level, cd, 2.0, active, ok);
        assert_eq!(ring(0, 0.0, false, true).state, RingState::Locked);
        assert_eq!(ring(0, 1.0, false, true).fill, 0.0);
        assert_eq!(ring(1, 0.0, false, true).state, RingState::Ready);
        let cooling = ring(1, 0.5, false, true);
        assert_eq!(cooling.state, RingState::Cooling);
        assert!((cooling.fill - 0.75).abs() < 1e-4);
        assert!((cooling.left - 0.5).abs() < 1e-4);
        assert_eq!(ring(1, 0.0, true, true).state, RingState::Active);
        assert_eq!(ring(1, 0.0, false, false).state, RingState::NoEnergy);
        // Cooling wins over a missing shield: it is not ready either way.
        assert_eq!(ring(1, 1.0, false, false).state, RingState::Cooling);
    }

    #[test]
    fn a_zero_length_cooldown_never_divides_by_zero() {
        let ring = ability_ring(Ability::Ping, 1, 0.0, 0.0, false, true);
        assert_eq!(ring.fill, 1.0);
        assert_eq!(ring.state, RingState::Ready);
    }

    #[test]
    fn threat_pips_follow_the_verdict_ladder_and_vanish_in_a_quiet_place() {
        assert_eq!(threat_pips(1.0, 1.0, 0, 0), 0);
        // Verdict thresholds at threat 1: ratio is the power.
        assert_eq!(threat_pips(0.5, 1.0, 3, 0), 5);
        assert_eq!(threat_pips(0.7, 1.0, 3, 0), 4);
        assert_eq!(threat_pips(1.0, 1.0, 3, 0), 3);
        assert_eq!(threat_pips(1.2, 1.0, 3, 0), 2);
        assert_eq!(threat_pips(2.0, 1.0, 3, 0), 1);
        // A crowd hunting adds one, capped at five.
        assert_eq!(threat_pips(2.0, 1.0, 0, 5), 2);
        assert_eq!(threat_pips(0.5, 1.0, 0, 9), 5);
    }

    #[test]
    fn the_region_tag_fades_to_a_floor_not_to_nothing() {
        assert_eq!(region_alpha(0.0), 1.0);
        assert_eq!(region_alpha(REGION_SHOW), 1.0);
        let mid = region_alpha(REGION_SHOW + REGION_FADE / 2.0);
        assert!(mid < 1.0 && mid > REGION_FLOOR);
        assert!((region_alpha(100.0) - REGION_FLOOR).abs() < 1e-5);
    }

    #[test]
    fn the_model_of_a_fresh_ship_is_full_with_locked_abilities_and_a_free_gun() {
        let mut game = empty_game();
        game.step(DT, Input::default());
        let hud = game.hud();
        assert!(hud.hull_fraction > 0.99);
        assert_eq!(hud.abilities[0].state, RingState::Locked);
        assert_eq!(hud.abilities[1].state, RingState::Locked);
        assert_eq!(hud.abilities[2].state, RingState::Ready);
        assert_eq!(hud.weapon.profile, Profile::Stock);
        assert_eq!(hud.weapon.fuel, 1.0);
        assert!(!hud.weapon.dry);
        assert_eq!(hud.cargo.len(), 3);
        assert_eq!(hud.threat, 0);
        assert_eq!(hud.lives, 3);
    }

    #[test]
    fn hints_lead_with_what_the_place_offers_and_stay_short() {
        let mut game = empty_game();
        game.step(DT, Input::default());
        let hints = game.context_hints();
        assert!(hints.len() <= MAX_HINTS);
        assert_eq!(hints[0].key, "SPACE");
        // Locked abilities are not advertised.
        assert!(hints.iter().all(|h| h.key != "D" && h.key != "SHIFT"));
        game.game_over = true;
        assert_eq!(game.context_hints()[0].key, "ENTER");
    }

    #[test]
    fn a_bought_ability_is_fresh_until_it_is_used() {
        use crate::simulation::skills::Skill;
        let mut game = empty_game();
        game.set_auto_ping(false);
        game.loadout.skills.raise(Skill::Dash);
        game.step(DT, Input::default());
        let hud = game.hud();
        assert!(hud.abilities[1].fresh);
        assert!(!hud.abilities[0].fresh, "a locked ring is not new");
        assert!(!hud.abilities[2].fresh);
        game.cargo.metal = 0.0;
        assert!(game.dash(None));
        assert!(!game.hud().abilities[1].fresh);
    }

    #[test]
    fn a_hit_leaves_a_direction_mark_that_fades() {
        let mut game = empty_game();
        game.set_auto_ping(false);
        game.step(DT, Input::default());
        let ship = game.player().unwrap().position;
        // A hostile shot right beside the ship, then damage with it close.
        game.bullets.push(crate::simulation::Bullet::hostile(
            ship + Vec2::new(0.0, 40.0),
            Vec2::new(0.0, -300.0),
            2.0,
            12.0,
        ));
        game.player_invulnerability = 0.0;
        for _ in 0..14 {
            game.step(DT, Input::default());
        }
        let marks = game.hud().hurts;
        assert!(!marks.is_empty(), "the hit was marked");
        assert!((marks[0].0 - std::f32::consts::FRAC_PI_2).abs() < 0.5);
        for _ in 0..60 {
            game.step(DT, Input::default());
        }
        assert!(game.hud().hurts.is_empty());
    }

    #[test]
    fn a_ping_puts_the_third_ring_on_cooldown() {
        let mut game = empty_game();
        assert!(game.ping());
        let ring = game.hud().abilities[2];
        assert_eq!(ring.state, RingState::Cooling);
        assert!(ring.left > 0.0);
    }
}
