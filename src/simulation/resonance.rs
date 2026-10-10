//! Resonance (`docs/CAPABILITIES.md` section 5, slice K7): a small table of item pairs that
//! do something extra when both are aboard. Two kinds, both bounded:
//!
//! - a **verb** changes *what* an existing hook does (a perfect parry scatters creatures, a
//!   dash cuts every cord). It adds no magnitude, so the threat model needs no new term;
//! - a **bonus** is a `Stat` effect of at most `BONUS_CAP` (25 percent), summed per stat and
//!   clamped again, then folded in by `Loadout::stats()` like any other effect.
//!
//! The UX rule: nothing here is a thing the player must learn. Every part, organ and skill is
//! still plainly better with more of it; a resonance is a bonus on top, and it always says
//! itself, in one sentence of the form "<A> does <effect> when used with <B>", on the bench
//! row of either partner (marked ACTIVE when both are aboard, otherwise a hint naming the
//! missing partner). Pieces are counted when owned or fitted, never by which weapon is
//! currently selected, so a build never loses a resonance by cycling guns.
//!
//! Default loadouts hold no pair, so their stats are bit-identical to before this slice.

use super::arsenal::Profile;
use super::bench::BenchAction;
use super::organs::Organ;
use super::skills::Skill;
use super::upgrades::{Effect, Loadout, Stat, Trait};
use super::*;

/// What a thing is to the synergy model: the four damage families and the nine verbs the
/// capability channels use (section 5.1). Every piece carries a small set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tag {
    Kinetic,
    Needle,
    Lance,
    Explosive,
    Parry,
    Dash,
    Jam,
    Phase,
    Blink,
    Field,
    Cord,
    Bio,
    Light,
}

impl Tag {
    pub const ALL: [Tag; 13] = [
        Self::Kinetic,
        Self::Needle,
        Self::Lance,
        Self::Explosive,
        Self::Parry,
        Self::Dash,
        Self::Jam,
        Self::Phase,
        Self::Blink,
        Self::Field,
        Self::Cord,
        Self::Bio,
        Self::Light,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Kinetic => "KINETIC",
            Self::Needle => "NEEDLE",
            Self::Lance => "LANCE",
            Self::Explosive => "EXPLOSIVE",
            Self::Parry => "PARRY",
            Self::Dash => "DASH",
            Self::Jam => "JAM",
            Self::Phase => "PHASE",
            Self::Blink => "BLINK",
            Self::Field => "FIELD",
            Self::Cord => "CORD",
            Self::Bio => "BIO",
            Self::Light => "LIGHT",
        }
    }
}

/// One thing a resonance can name: an organ, a bought skill or a trait (weapon traits count
/// as owned when their profile is in the arsenal; the others when a part or boost carries
/// them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    Organ(Organ),
    Skill(Skill),
    Trait(Trait),
}

impl Piece {
    /// Capitalized name, as the bench says it.
    pub fn name(self) -> String {
        match self {
            Self::Organ(o) => o.label().to_string(),
            Self::Skill(s) => s.label().to_uppercase(),
            Self::Trait(t) => t.label().to_uppercase(),
        }
    }

    pub fn tags(self) -> &'static [Tag] {
        match self {
            Self::Organ(o) => match o {
                Organ::Remora => &[Tag::Bio],
                Organ::Faraday => &[Tag::Jam],
                Organ::Veil => &[Tag::Phase, Tag::Dash],
                Organ::Skipjack => &[Tag::Blink, Tag::Dash],
            },
            Self::Skill(s) => match s {
                Skill::Parry => &[Tag::Parry],
                Skill::Dash => &[Tag::Dash],
                _ => &[],
            },
            Self::Trait(t) => match t {
                Trait::Spread | Trait::Homing | Trait::Broadside | Trait::Tailgun => {
                    &[Tag::Kinetic]
                }
                Trait::Needles => &[Tag::Needle],
                Trait::Pierce => &[Tag::Lance],
                Trait::Blast | Trait::Missiles | Trait::Mines | Trait::Nova => &[Tag::Explosive],
                Trait::Shears => &[Tag::Cord],
                Trait::Ballast | Trait::Aura => &[Tag::Field],
                Trait::Siphon => &[Tag::Bio],
                Trait::Hardening => &[Tag::Jam],
                Trait::Ram => &[],
            },
        }
    }

    /// Whether the loadout holds this piece (organs: working, fitted or on a bond's loan).
    pub fn present(self, loadout: &Loadout) -> bool {
        match self {
            Self::Organ(o) => loadout.organs.active(o).is_some(),
            Self::Skill(s) => loadout.skills.level(s) > 0,
            Self::Trait(t) => match Profile::from_trait(t) {
                Some(profile) => loadout.arsenal.owns(profile),
                None => loadout.trait_carried(t),
            },
        }
    }
}

/// A new behavior at an existing hook. Verbs are not magnitudes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    /// A perfect parry scatters the creatures around the ship.
    ParryScatter,
    /// A dash cuts every latched cord, however stout.
    DashCuts,
    /// A kill ends the recently-hit wait, so the Remora mends at once.
    KillMends,
    /// A dash readies the parry again.
    DashReadiesParry,
    /// The lunatic field cuts every latched cord at once.
    FieldCuts,
    /// Shots fired during a veil home at full strength.
    VeilSeeks,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Verb(Verb),
    Bonus(Stat, f32),
}

/// One row: `needs[0]` is the one the sentence is about, `needs[1]` its partner.
#[derive(Clone, Copy, Debug)]
pub struct Resonance {
    pub needs: [Piece; 2],
    pub kind: Kind,
}

/// No bonus row, and no stat in total, exceeds this share.
pub const BONUS_CAP: f32 = 0.25;
/// Most rows the table may ever hold (design cap).
pub const MAX_ROWS: usize = 30;
const _: () = assert!(TABLE.len() <= MAX_ROWS);

use Piece::{Organ as O, Skill as S, Trait as T};

/// The table: six verbs, then four bonuses.
pub const TABLE: [Resonance; 10] = [
    Resonance {
        needs: [O(Organ::Faraday), S(Skill::Parry)],
        kind: Kind::Verb(Verb::ParryScatter),
    },
    Resonance {
        needs: [O(Organ::Skipjack), T(Trait::Shears)],
        kind: Kind::Verb(Verb::DashCuts),
    },
    Resonance {
        needs: [T(Trait::Siphon), O(Organ::Remora)],
        kind: Kind::Verb(Verb::KillMends),
    },
    Resonance {
        needs: [O(Organ::Veil), S(Skill::Parry)],
        kind: Kind::Verb(Verb::DashReadiesParry),
    },
    Resonance {
        needs: [T(Trait::Aura), T(Trait::Shears)],
        kind: Kind::Verb(Verb::FieldCuts),
    },
    Resonance {
        needs: [O(Organ::Veil), T(Trait::Homing)],
        kind: Kind::Verb(Verb::VeilSeeks),
    },
    Resonance {
        needs: [T(Trait::Needles), T(Trait::Homing)],
        kind: Kind::Bonus(Stat::Range, 0.12),
    },
    Resonance {
        needs: [T(Trait::Nova), S(Skill::Parry)],
        kind: Kind::Bonus(Stat::Recharge, 0.15),
    },
    Resonance {
        needs: [T(Trait::Spread), T(Trait::Broadside)],
        kind: Kind::Bonus(Stat::FireRate, 0.10),
    },
    Resonance {
        needs: [T(Trait::Missiles), T(Trait::Homing)],
        kind: Kind::Bonus(Stat::ShotSpeed, 0.10),
    },
];

impl Resonance {
    pub fn active(&self, loadout: &Loadout) -> bool {
        self.needs.iter().all(|p| p.present(loadout))
    }

    /// What the first piece does, in the numbers of `tune`.
    pub fn effect(&self, tune: &Tunables) -> String {
        match self.kind {
            Kind::Verb(Verb::ParryScatter) => format!(
                "scatter creatures within {:.0} for {:.1} s on a perfect parry",
                tune.resonance_scatter_radius, tune.resonance_scatter_time
            ),
            Kind::Verb(Verb::DashCuts) => "cut every latched cord on a dash, however stout".into(),
            Kind::Verb(Verb::KillMends) => {
                "end the recently-hit wait on each kill, so the Remora mends at once".into()
            }
            Kind::Verb(Verb::DashReadiesParry) => "ready the parry again on every dash".into(),
            Kind::Verb(Verb::FieldCuts) => "cut every latched cord at once, however stout".into(),
            Kind::Verb(Verb::VeilSeeks) => "give shots full homing while the veil lasts".into(),
            Kind::Bonus(stat, amount) => {
                format!("add {:+.0}% {}", amount * 100.0, stat.label())
            }
        }
    }

    /// "<A> does <effect> when used with <B>".
    pub fn sentence(&self, tune: &Tunables) -> String {
        format!(
            "{} does {} when used with {}",
            self.needs[0].name(),
            self.effect(tune),
            self.needs[1].name()
        )
    }
}

impl Loadout {
    /// Whether a part or running boost carries the (non-weapon) trait.
    pub(super) fn trait_carried(&self, kind: Trait) -> bool {
        self.effects()
            .any(|e| matches!(*e, Effect::Trait(t, level) if t == kind && level > 0))
    }

    /// The rows whose pieces are all aboard.
    pub fn resonances(&self) -> impl Iterator<Item = &'static Resonance> + '_ {
        TABLE.iter().filter(move |r| r.active(self))
    }

    pub fn has_verb(&self, verb: Verb) -> bool {
        self.resonances().any(|r| r.kind == Kind::Verb(verb))
    }

    /// The stat effects of every active bonus row, each stat's total held to `BONUS_CAP`.
    pub(super) fn resonance_effects(&self) -> Vec<Effect> {
        let mut total: Vec<(Stat, f32)> = Vec::new();
        for r in self.resonances() {
            if let Kind::Bonus(stat, amount) = r.kind {
                let amount = amount.clamp(0.0, BONUS_CAP);
                match total.iter_mut().find(|(s, _)| *s == stat) {
                    Some((_, sum)) => *sum = (*sum + amount).min(BONUS_CAP),
                    None => total.push((stat, amount)),
                }
            }
        }
        total
            .into_iter()
            .map(|(stat, amount)| Effect::Stat(stat, amount))
            .collect()
    }

    /// The pieces a bench row stands for (organ, skill, weapon profile or a part's traits).
    fn bench_pieces(&self, action: BenchAction) -> Vec<Piece> {
        match action {
            BenchAction::Organ(o) => vec![Piece::Organ(o)],
            BenchAction::Skill(s) => vec![Piece::Skill(s)],
            BenchAction::Weapon(p) => p.weapon_trait().map(Piece::Trait).into_iter().collect(),
            BenchAction::Upgrade(i) | BenchAction::Reforge(i) => self
                .parts
                .get(i)
                .into_iter()
                .flat_map(|p| p.effects.iter())
                .filter_map(|e| match *e {
                    Effect::Trait(t, _) => Some(Piece::Trait(t)),
                    Effect::Stat(..) => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl Game {
    /// Whether a resonance verb is working now (read live from the loadout, so a bench change
    /// or a dormant organ is never stale).
    pub(super) fn resonance_verb(&self, verb: Verb) -> bool {
        self.loadout.has_verb(verb)
    }

    /// Perfect-parry verb: creatures around `origin` scatter for a moment.
    pub(super) fn parry_scatter(&mut self, origin: Vec2) {
        let (radius, time) = (
            self.tune.resonance_scatter_radius,
            self.tune.resonance_scatter_time,
        );
        for body in self.bodies.iter_mut().filter(|b| {
            b.active && b.kind == BodyKind::Creature && b.position.distance(origin) < radius
        }) {
            body.panic = body.panic.max(time);
            body.panic_from = origin;
        }
    }

    /// The resonance lines for a bench row: `(active, sentence)`, active ones first. A row
    /// shows every pair its piece belongs to, as ACTIVE or as a hint naming the partner it
    /// is missing, so the sentence is always readable before the pair is built.
    pub fn resonance_lines(&self, action: BenchAction) -> Vec<(bool, String)> {
        let pieces = self.loadout.bench_pieces(action);
        let mut lines = Vec::new();
        for r in &TABLE {
            let Some(mine) = r.needs.iter().position(|p| pieces.contains(p)) else {
                continue;
            };
            let active = r.active(&self.loadout);
            let text = if active {
                format!("RESONANCE ACTIVE: {}", r.sentence(&self.tune))
            } else {
                let partner = r.needs[1 - mine];
                format!(
                    "RESONATES WITH {}: {}",
                    partner.name(),
                    r.sentence(&self.tune)
                )
            };
            lines.push((active, text));
        }
        lines.sort_by_key(|(active, _)| !*active);
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::organs::Strain;
    use crate::simulation::tests::{add, empty_game};

    /// Fits a working organ the way the bench does.
    fn fit(game: &mut Game, which: Organ) {
        game.loadout.skills.raise(Skill::Symbiosis);
        game.loadout.organs.acquire(
            Strain {
                organ: which,
                level: 1,
                magnitude: 1.0,
            },
            &DEFAULT_TUNING,
        );
        game.cargo.crystal = 100.0;
        game.cargo.fuel = 100.0;
        game.cargo.biomass = 100.0;
        game.bench_organ(which).unwrap();
    }

    #[test]
    fn the_table_is_bounded_and_every_row_is_distinct() {
        let verbs = TABLE
            .iter()
            .filter(|r| matches!(r.kind, Kind::Verb(_)))
            .count();
        assert_eq!((verbs, TABLE.len() - verbs), (6, 4));
        assert!(TABLE.len() <= MAX_ROWS);
        for (i, a) in TABLE.iter().enumerate() {
            assert_ne!(a.needs[0], a.needs[1]);
            if let Kind::Bonus(_, amount) = a.kind {
                assert!(amount > 0.0 && amount <= BONUS_CAP);
            }
            for b in &TABLE[i + 1..] {
                assert!(a.needs != b.needs, "one row per pair");
            }
        }
        assert!(Tag::ALL.iter().all(|t| !t.label().is_empty()));
    }

    #[test]
    fn a_default_loadout_resonates_with_nothing_and_stats_are_unchanged() {
        let loadout = Loadout::default();
        assert_eq!(loadout.resonances().count(), 0);
        assert!(loadout.resonance_effects().is_empty());
        assert_eq!(loadout.stats(), super::super::upgrades::Stats::BASE);
    }

    #[test]
    fn a_bonus_needs_both_pieces_and_more_of_a_piece_never_hurts() {
        let mut loadout = Loadout::default();
        loadout.arsenal.acquire(Profile::Needles, 1);
        let alone = loadout.stats();
        loadout.arsenal.acquire(Profile::Homing, 1);
        let both = loadout.stats();
        assert!(both.shot_life > alone.shot_life, "range bonus appears");
        assert_eq!(loadout.resonances().count(), 1);
        loadout.arsenal.acquire(Profile::Homing, 1);
        assert!(loadout.stats().shot_life >= both.shot_life);
    }

    #[test]
    fn bonuses_on_one_stat_stay_inside_the_cap() {
        let mut loadout = Loadout::default();
        for p in [Profile::Needles, Profile::Homing, Profile::Missiles] {
            loadout.arsenal.acquire(p, 3);
        }
        let effects = loadout.resonance_effects();
        assert_eq!(effects.len(), 2, "range and shot speed");
        for e in effects {
            if let Effect::Stat(_, amount) = e {
                assert!(amount <= BONUS_CAP + 1e-6);
            }
        }
    }

    #[test]
    fn every_pair_states_itself_on_both_partners() {
        let game = empty_game();
        for r in &TABLE {
            let sentence = r.sentence(&game.tune);
            assert!(sentence.starts_with(&r.needs[0].name()));
            assert!(sentence.contains(" does "));
            assert!(sentence.contains(" when used with "));
            assert!(sentence.ends_with(&r.needs[1].name()));
        }
        let lines = game.resonance_lines(BenchAction::Organ(Organ::Faraday));
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].0);
        assert!(
            lines[0]
                .1
                .starts_with("RESONATES WITH PARRY: FARADAY does scatter")
        );
        let partner = game.resonance_lines(BenchAction::Skill(Skill::Parry));
        assert!(partner.iter().any(|(_, t)| {
            t.starts_with("RESONATES WITH FARADAY: FARADAY does") && t.ends_with("with PARRY")
        }));
    }

    #[test]
    fn an_active_pair_is_marked_active_and_a_perfect_parry_scatters() {
        let mut found = false;
        for _ in 0..20 {
            let mut game = empty_game();
            game.player_invulnerability = 1e9;
            game.bodies[0].angle = 0.0;
            game.bodies[0].shield = 60.0;
            game.bodies[0].max_shield = 60.0;
            while game.loadout.skills.raise(Skill::Parry).is_some() {}
            fit(&mut game, Organ::Faraday);
            assert!(game.loadout.has_verb(Verb::ParryScatter));
            let lines = game.resonance_lines(BenchAction::Organ(Organ::Faraday));
            assert!(lines[0].0 && lines[0].1.starts_with("RESONANCE ACTIVE: FARADAY does"));
            let at = game.bodies[0].position;
            let id = add(&mut game, BodyKind::Creature, at + Vec2::new(120.0, 0.0));
            game.parry();
            game.bullets.push(Bullet::hostile(
                at + Vec2::new(60.0, 0.0),
                Vec2::new(-400.0, 0.0),
                2.0,
                10.0,
            ));
            game.update_parry(0.0);
            if game.run.perfect_parries == 1 {
                let body = game.bodies.iter().find(|b| b.id == id).unwrap();
                assert!(body.panic > 0.0, "the creature scatters");
                found = true;
                break;
            }
        }
        assert!(found, "no perfect parry in twenty tries");
    }

    #[test]
    fn a_kill_ends_the_hit_wait_only_with_siphon_and_remora() {
        let mut game = empty_game();
        game.stats.siphon = 1;
        let at = game.bodies[0].position + Vec2::new(100.0, 0.0);
        let id = add(&mut game, BodyKind::Creature, at);
        let fallen = game.bodies.iter().find(|b| b.id == id).unwrap().clone();
        game.bodies[0].since_hit = 0.0;
        game.siphon(&fallen);
        assert_eq!(game.bodies[0].since_hit, 0.0, "no Remora, no verb");
        fit(&mut game, Organ::Remora);
        // The pair also needs the siphon trait on a part (not the stats shortcut).
        game.loadout.parts.push(super::super::upgrades::Part {
            name: "Leech".into(),
            slot: super::super::upgrades::Slot::Core,
            rarity: super::super::upgrades::Rarity::Common,
            grade: 1.0,
            effects: vec![Effect::Trait(Trait::Siphon, 1)],
            stem: String::new(),
            core: usize::MAX,
        });
        assert!(game.loadout.has_verb(Verb::KillMends));
        game.siphon(&fallen);
        assert!(game.bodies[0].since_hit >= game.tune.remora_quiet);
    }
}
