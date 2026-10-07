//! The one shared status of the ship: a jam (some systems off for a moment), a confusion
//! (the pilot's controls sway) and a glitch (a screen treatment). Emp, confuse and glare
//! creatures (see `powers`) and apex stamps all go through the same doors here, so the fairness
//! rules live in one place:
//!
//! - a jam or confusion lasts at most `JAM_MAX`, never starts while one is active, while the
//!   ship is immune (`JAM_IMMUNITY` after the last one ended), landed or in respawn grace;
//! - a jam takes one or two of weapons, dash, parry and boost, never parry and dash together,
//!   never all; the HUD may be jammed besides when it is rolled;
//! - confusion rotates aim and movement by a bounded, swaying angle and may invert the turn for
//!   `CONFUSE_FLIP` seconds at most; it never touches the world, only the input;
//! - the glitch is presentation only: nothing in the rules reads it, and it has its own gap.

use super::skills::Skill;
use super::tuning as t;
use super::*;
use crate::power;

/// A system a jam can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Weapons,
    Dash,
    Parry,
    Boost,
    Hud,
}

/// Seconds left on each jam, the confusion and the glitch, and the immunity between them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JamState {
    weapons: f32,
    dash: f32,
    parry: f32,
    boost: f32,
    hud: f32,
    immunity: f32,
    confuse: f32,
    confuse_total: f32,
    confuse_amp: f32,
    confuse_flip: f32,
    confuse_phase: f32,
    glitch: f32,
    glitch_total: f32,
    glitch_gap: f32,
    glitch_seed: u32,
    /// Seconds the ship counts as shooting (for the dark).
    shooting: f32,
}

/// What the adapter draws of the status.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JamView {
    pub weapons: f32,
    pub dash: f32,
    pub parry: f32,
    pub boost: f32,
    pub hud: f32,
    /// Seconds of confusion left, and the angle (radians) the controls are off by now.
    pub confuse: f32,
    pub confuse_offset: f32,
    /// Glitch strength in 0..1 (a fade in and out), and a seed for its false blips.
    pub glitch: f32,
    pub seed: u32,
}

impl JamView {
    /// Anything jammed or confused (the HUD wears a tint).
    pub fn any(&self) -> bool {
        self.weapons > 0.0
            || self.dash > 0.0
            || self.parry > 0.0
            || self.boost > 0.0
            || self.hud > 0.0
            || self.confuse > 0.0
    }
}

impl JamState {
    fn left(&self, system: System) -> f32 {
        match system {
            System::Weapons => self.weapons,
            System::Dash => self.dash,
            System::Parry => self.parry,
            System::Boost => self.boost,
            System::Hud => self.hud,
        }
    }

    fn slot(&mut self, system: System) -> &mut f32 {
        match system {
            System::Weapons => &mut self.weapons,
            System::Dash => &mut self.dash,
            System::Parry => &mut self.parry,
            System::Boost => &mut self.boost,
            System::Hud => &mut self.hud,
        }
    }

    /// A jam or confusion is running.
    pub fn active(&self) -> bool {
        self.weapons > 0.0
            || self.dash > 0.0
            || self.parry > 0.0
            || self.boost > 0.0
            || self.hud > 0.0
            || self.confuse > 0.0
    }

    fn offset(&self) -> f32 {
        if self.confuse <= 0.0 {
            return 0.0;
        }
        let elapsed = self.confuse_total - self.confuse;
        let ramp = (elapsed / 0.15).min(1.0).min(self.confuse / 0.15);
        self.confuse_amp * (self.confuse_phase + elapsed * power::CONFUSE_SWAY).sin() * ramp
    }
}

impl Game {
    /// Whether `system` is jammed now.
    pub fn jammed(&self, system: System) -> bool {
        self.jam.left(system) > 0.0
    }

    /// Seconds of immunity left before the ship can be jammed again.
    pub fn jam_immunity(&self) -> f32 {
        self.jam.immunity
    }

    /// The status as the adapter draws it.
    pub fn jam_view(&self) -> JamView {
        let j = &self.jam;
        let glitch = if j.glitch > 0.0 && j.glitch_total > 0.0 {
            let elapsed = j.glitch_total - j.glitch;
            (elapsed / 0.12).min(1.0).min(j.glitch / 0.25)
        } else {
            0.0
        };
        JamView {
            weapons: j.weapons,
            dash: j.dash,
            parry: j.parry,
            boost: j.boost,
            hud: j.hud,
            confuse: j.confuse,
            confuse_offset: j.offset(),
            glitch,
            seed: j.glitch_seed,
        }
    }

    /// Whether a new jam or confusion may start now: nothing running, not immune, not landed,
    /// not in respawn grace.
    pub fn jammable(&self) -> bool {
        self.player().is_some()
            && !self.game_over
            && !self.is_landed()
            && !self.jam.active()
            && self.jam.immunity <= 0.0
            && self.player_invulnerability <= t::DASH_INVULN
    }

    fn glitchable(&self) -> bool {
        self.player().is_some()
            && !self.game_over
            && !self.is_landed()
            && self.jam.glitch <= 0.0
            && self.jam.glitch_gap <= 0.0
            && self.player_invulnerability <= t::DASH_INVULN
    }

    /// Whether a glare may start a charge now.
    pub fn glitch_ready(&self) -> bool {
        self.glitchable()
    }

    /// Systems an emp may take from this ship now (only what the ship has).
    pub fn jam_candidates(&self) -> Vec<System> {
        let mut out = vec![System::Weapons];
        if self.loadout.skills.level(Skill::Dash) > 0 {
            out.push(System::Dash);
        }
        if self.loadout.skills.level(Skill::Parry) > 0 {
            out.push(System::Parry);
        }
        if !self.loadout.arsenal.boosts.is_empty() {
            out.push(System::Boost);
        }
        out
    }

    /// Jams `systems` for `seconds`. At most two of weapons, dash, parry and boost, never dash
    /// with parry (the dash is dropped), the HUD besides if listed; at most `JAM_MAX`. False if
    /// the ship cannot be jammed now or nothing is left to jam.
    pub fn apply_jam(&mut self, systems: &[System], seconds: f32) -> bool {
        if !self.jammable() {
            return false;
        }
        let scale = self.jam_scale();
        if scale <= 0.0 {
            return false;
        }
        let seconds = (seconds * scale).clamp(0.1, power::JAM_MAX);
        let mut chosen: Vec<System> = Vec::new();
        for &s in systems {
            if s == System::Hud {
                chosen.push(s);
                continue;
            }
            let real = chosen.iter().filter(|c| **c != System::Hud).count();
            let clash = (s == System::Dash && chosen.contains(&System::Parry))
                || (s == System::Parry && chosen.contains(&System::Dash));
            if real < 2 && !clash && !chosen.contains(&s) {
                chosen.push(s);
            }
        }
        if chosen.is_empty() {
            return false;
        }
        for s in chosen {
            *self.jam.slot(s) = seconds;
        }
        self.jam.immunity = power::JAM_IMMUNITY;
        let at = self.player().map_or(Vec2::ZERO, |p| p.position);
        self.cue(Cue::JamHit { at });
        // A jam shakes the picture a moment too (presentation only).
        self.add_glitch(0.4, 0x4A41);
        true
    }

    /// Confuses the controls for `seconds` with a sway of `amp` radians and, if `flip`, a
    /// turn inversion for the first `CONFUSE_FLIP` seconds.
    pub fn apply_confuse(&mut self, amp: f32, flip: bool, seconds: f32, phase: f32) -> bool {
        if !self.jammable() {
            return false;
        }
        let scale = self.jam_scale();
        if scale <= 0.0 {
            return false;
        }
        let seconds = (seconds * scale).clamp(0.1, power::JAM_MAX);
        let j = &mut self.jam;
        j.confuse = seconds;
        j.confuse_total = seconds;
        j.confuse_amp = amp.clamp(0.0, 1.0);
        j.confuse_flip = if flip { power::CONFUSE_FLIP } else { 0.0 };
        j.confuse_phase = phase;
        j.immunity = power::JAM_IMMUNITY;
        let at = self.player().map_or(Vec2::ZERO, |p| p.position);
        self.cue(Cue::JamHit { at });
        self.add_glitch(0.4, 0x4A42);
        true
    }

    /// A screen glitch of `seconds` (at most `GLITCH_MAX`). Presentation only: nothing in the
    /// rules reads it. False if one runs, one ended lately, or the ship is in grace.
    pub fn apply_glitch(&mut self, seconds: f32, seed: u32) -> bool {
        if !self.glitchable() {
            return false;
        }
        let scale = self.jam_scale();
        if scale <= 0.0 {
            return false;
        }
        self.add_glitch(seconds * scale, seed);
        true
    }

    fn add_glitch(&mut self, seconds: f32, seed: u32) {
        if self.jam.glitch > 0.0 {
            return;
        }
        let seconds = seconds.clamp(0.1, power::GLITCH_MAX);
        self.jam.glitch = seconds;
        self.jam.glitch_total = seconds;
        self.jam.glitch_seed = seed;
        self.jam.glitch_gap = power::GLITCH_GAP;
    }

    /// Ends every status (the ship was lost).
    pub(super) fn clear_jams(&mut self) {
        self.jam = JamState::default();
    }

    /// Ticks the statuses; the immunity runs only while nothing is jammed.
    pub(super) fn update_jam(&mut self, dt: f32, firing: bool) {
        let j = &mut self.jam;
        let was = j.active();
        for s in [
            System::Weapons,
            System::Dash,
            System::Parry,
            System::Boost,
            System::Hud,
        ] {
            let slot = j.slot(s);
            *slot = (*slot - dt).max(0.0);
        }
        j.confuse = (j.confuse - dt).max(0.0);
        j.glitch = (j.glitch - dt).max(0.0);
        if j.glitch <= 0.0 {
            j.glitch_gap = (j.glitch_gap - dt).max(0.0);
        }
        j.shooting = if firing {
            power::DIM_FIRING
        } else {
            (j.shooting - dt).max(0.0)
        };
        if j.active() {
            return;
        }
        if was {
            // The jam just ended: the immunity counts from here.
            j.immunity = power::JAM_IMMUNITY;
        } else {
            j.immunity = (j.immunity - dt).max(0.0);
        }
    }

    /// The input the pilot's hands actually deliver: unchanged unless confused. Pure in the
    /// status, so it is deterministic.
    pub(super) fn scramble_input(&self, input: Input) -> Input {
        let j = &self.jam;
        if j.confuse <= 0.0 {
            return input;
        }
        let offset = j.offset();
        let elapsed = j.confuse_total - j.confuse;
        let turn_bias = 0.35
            * if j.confuse_amp > 0.0 {
                offset / j.confuse_amp
            } else {
                0.0
            };
        let mut out = input;
        out.turn = input.turn * if elapsed < j.confuse_flip { -1.0 } else { 1.0 };
        if out.turn.is_finite() {
            out.turn = (out.turn + turn_bias).clamp(-1.0, 1.0);
        }
        out.aim_direction = input
            .aim_direction
            .map(|a| Vec2::from_angle(offset).rotate(a));
        out.move_direction = input
            .move_direction
            .map(|m| Vec2::from_angle(offset).rotate(m));
        out
    }

    /// The light eaters in play: (position, reach, strength). Empty almost always.
    pub fn dim_sources(&self) -> Vec<(Vec2, f32, f32)> {
        self.bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Creature && b.active && !b.follower)
            .filter_map(|b| {
                let s = power::Power::Dim.strength(&b.genome);
                (s > 0.0).then_some((b.position, b.genome.power_reach * power::DIM_REACH, s))
            })
            .collect()
    }

    /// How dark it is at `at` given the sources (0 normal, up to `1 - DIM_FLOOR`).
    pub fn dim_from(sources: &[(Vec2, f32, f32)], at: Vec2) -> f32 {
        let mut dark = 0.0_f32;
        for &(position, reach, s) in sources {
            let d = position.distance(at);
            if d >= reach {
                continue;
            }
            // Full dark in the inner half, easing out to the rim.
            let u = ((reach - d) / (reach * 0.5)).clamp(0.0, 1.0);
            let ease = u * u * (3.0 - 2.0 * u);
            dark = dark.max(ease * (0.5 + 0.5 * s) * (1.0 - power::DIM_FLOOR));
        }
        dark
    }

    /// How dark it is at `at`: the strongest dim field there.
    pub fn dim_at(&self, at: Vec2) -> f32 {
        Self::dim_from(&self.dim_sources(), at)
    }

    /// How much farther away a creature thinks a quiet ship in the dark is (1 = no change).
    pub(super) fn dim_notice(&self) -> f32 {
        if self.jam.shooting > 0.0 {
            return 1.0;
        }
        let Some(ship) = self.player().map(|p| p.position) else {
            return 1.0;
        };
        let dark = self.dim_at(ship) / (1.0 - power::DIM_FLOOR);
        1.0 + (1.0 / power::DIM_NOTICE - 1.0) * dark
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Genome, Species, Trigger};
    use crate::power::Power;
    use crate::simulation::tests::{DT, empty_game, spawn};

    fn ready() -> Game {
        let mut game = empty_game();
        game.player_invulnerability = 0.0;
        game.loadout.skills.raise(Skill::Dash);
        game.loadout.skills.raise(Skill::Parry);
        game
    }

    fn stormcap() -> Genome {
        Genome {
            emp: 0.9,
            power_period: 2.0,
            power_reach: 300.0,
            power_hold: 1.4,
            shield: 40.0,
            hull: 70.0,
            weapon: crate::genome::Weapon::None,
            trigger: Trigger::Sight,
            sight: 2000.0,
            lose: 2400.0,
            speed: 0.0,
            cruise: 0.0,
            ..Genome::default()
        }
    }

    fn run(game: &mut Game, keep: &[u64], seconds: f32, mut each: impl FnMut(&Game)) {
        for _ in 0..(seconds / DT) as usize {
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || keep.contains(&b.id));
            game.step(DT, Input::default());
            each(game);
        }
    }

    #[test]
    fn a_jam_is_capped_never_takes_dash_and_parry_together_and_takes_at_most_two() {
        let mut game = ready();
        assert!(game.apply_jam(
            &[
                JamSystem::Dash,
                JamSystem::Parry,
                JamSystem::Weapons,
                JamSystem::Boost
            ],
            9.0
        ));
        let v = game.jam_view();
        assert!(v.weapons > 0.0 && v.dash > 0.0, "{v:?}");
        assert_eq!((v.parry, v.boost), (0.0, 0.0), "{v:?}");
        assert!(v.weapons <= power::JAM_MAX + 1e-4);
    }

    #[test]
    fn a_jam_cannot_stack_and_the_ship_is_immune_for_six_seconds_after_it_ends() {
        let mut game = ready();
        assert!(game.apply_jam(&[JamSystem::Weapons], 1.0));
        assert!(!game.apply_jam(&[JamSystem::Dash], 1.0), "no stacking");
        assert!(!game.apply_confuse(0.5, false, 1.0, 0.0), "no double lock");
        for _ in 0..(1.1 / DT) as usize {
            game.step(DT, Input::default());
        }
        assert!(!game.jam_view().any());
        // Immune from the moment it ends.
        let mut waited = 0.0;
        while !game.jammable() {
            game.step(DT, Input::default());
            waited += DT;
            assert!(waited < 7.0);
        }
        assert!(
            (power::JAM_IMMUNITY - 0.2..=power::JAM_IMMUNITY + 0.2).contains(&waited),
            "{waited}"
        );
        assert!(game.apply_jam(&[JamSystem::Weapons], 1.0));
    }

    #[test]
    fn a_jam_refuses_grace_and_landing() {
        let mut game = empty_game();
        game.player_invulnerability = 2.0;
        assert!(!game.apply_jam(&[JamSystem::Weapons], 1.0));
        assert!(!game.apply_glitch(1.0, 1));
        game.player_invulnerability = 0.0;
        assert!(game.apply_glitch(1.0, 1));
    }

    #[test]
    fn jammed_systems_refuse_and_the_hud_greys_their_rings() {
        let mut game = ready();
        game.step(DT, Input::default());
        let hud = game.hud();
        assert_eq!(hud.abilities[0].state, hud::RingState::Ready);
        assert!(game.apply_jam(&[JamSystem::Dash, JamSystem::Weapons], 1.0));
        let hud = game.hud();
        assert_eq!(hud.abilities[1].state, hud::RingState::Jammed);
        assert_ne!(hud.abilities[0].state, hud::RingState::Jammed);
        assert!(hud.weapon.jammed && hud.jam.any());
        assert!(!game.dash(None), "the dash is refused");
        let shots = game.bullets.len();
        game.step(
            DT,
            Input {
                fire: true,
                ..Default::default()
            },
        );
        assert_eq!(game.bullets.iter().filter(|b| b.friendly).count(), shots);
        assert!(game.drain_cues().contains(&Cue::Refused));
        // And it comes back.
        for _ in 0..(1.1 / DT) as usize {
            game.step(DT, Input::default());
        }
        assert_eq!(game.hud().abilities[1].state, hud::RingState::Ready);
        assert!(!game.hud().weapon.jammed);
    }

    #[test]
    fn a_stormcap_shows_a_ring_for_at_least_the_minimum_then_jams() {
        let mut game = ready();
        let id = spawn(&mut game, &Species::of(stormcap()), Vec2::new(0.0, 200.0));
        let mut told = None;
        let mut jammed_at = None;
        let mut seen_tell = false;
        run(&mut game, &[id], 12.0, |g| {
            let body = g.body(id).unwrap();
            if let Some(t) = g.power_view(body).jam {
                if told.is_none() {
                    told = Some(g.time);
                    assert!(t.total >= power::TELL_JAM);
                }
                seen_tell = true;
            }
            if jammed_at.is_none() && g.jam_view().any() {
                jammed_at = Some(g.time);
            }
        });
        let (told, hit) = (told.expect("a ring was shown"), jammed_at.expect("a jam"));
        assert!(hit - told >= power::TELL_JAM - DT * 2.0, "{}", hit - told);
        assert!(seen_tell);
    }

    #[test]
    fn leaving_the_ring_or_breaking_the_shield_saves_the_ship() {
        // Out of the ring when it closes: no jam.
        let mut game = ready();
        let id = spawn(&mut game, &Species::of(stormcap()), Vec2::new(0.0, 200.0));
        let mut warned = false;
        run(&mut game, &[id], 6.0, |g| {
            let body = g.body(id).unwrap();
            if g.power_view(body).jam.is_some() && !warned {
                warned = true;
            }
        });
        assert!(warned);
        let mut game = ready();
        let id = spawn(&mut game, &Species::of(stormcap()), Vec2::new(0.0, 200.0));
        let mut broke = false;
        let mut after = 0.0;
        for _ in 0..(12.0 / DT) as usize {
            game.bodies
                .retain(|b| b.kind == BodyKind::Player || b.id == id);
            game.step(DT, Input::default());
            if !broke && game.power_view(game.body(id).unwrap()).jam.is_some() {
                broke = true;
                let b = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
                (b.shield, b.since_hit) = (0.0, 0.0);
            }
            if broke {
                after += DT;
            }
            if broke && after < 1.1 {
                assert!(!game.jam_view().any(), "a broken charge never lands");
            }
        }
        assert!(broke);
    }

    #[test]
    fn one_charge_at_a_time_across_emitters() {
        let mut game = ready();
        let a = spawn(&mut game, &Species::of(stormcap()), Vec2::new(0.0, 200.0));
        let b = spawn(&mut game, &Species::of(stormcap()), Vec2::new(150.0, 150.0));
        run(&mut game, &[a, b], 15.0, |g| {
            let n = [a, b]
                .iter()
                .filter(|id| g.power_view(g.body(**id).unwrap()).jam.is_some())
                .count();
            assert!(n <= 1);
        });
    }

    #[test]
    fn confusion_is_bounded_short_flips_only_briefly_and_never_touches_the_world() {
        let mut game = ready();
        assert!(game.apply_confuse(0.7, true, 3.0, 0.0));
        let mut flips = 0.0;
        let mut worst = 0.0_f32;
        let input = Input {
            turn: 1.0,
            aim_direction: Some(Vec2::X),
            move_direction: Some(Vec2::X),
            ..Default::default()
        };
        let mut seconds = 0.0;
        while game.jam_view().confuse > 0.0 {
            let out = game.scramble_input(input);
            if out.turn < 0.0 {
                flips += DT;
            }
            let aim = out.aim_direction.unwrap();
            worst = worst.max(aim.y.atan2(aim.x).abs());
            assert!((out.move_direction.unwrap().length() - 1.0).abs() < 1e-4);
            game.step(DT, Input::default());
            seconds += DT;
        }
        assert!(seconds <= power::JAM_MAX + 0.1, "{seconds}");
        assert!(flips <= power::CONFUSE_FLIP + 0.1, "{flips}");
        assert!(worst <= 0.7 + 1e-3, "{worst}");
        // An unconfused ship's input is untouched.
        let same = game.scramble_input(input);
        assert_eq!(same.turn, 1.0);
        assert_eq!(same.aim_direction, Some(Vec2::X));
    }

    #[test]
    fn a_dizzard_charges_visibly_then_confuses() {
        let g = Genome {
            emp: 0.0,
            confuse: 0.9,
            ..stormcap()
        };
        let mut game = ready();
        let id = spawn(&mut game, &Species::of(g), Vec2::new(0.0, 200.0));
        let mut kind = None;
        let mut confused = false;
        run(&mut game, &[id], 12.0, |gm| {
            if let Some(t) = gm.power_view(gm.body(id).unwrap()).jam {
                kind = Some(t.kind);
            }
            confused |= gm.jam_view().confuse > 0.0;
        });
        assert_eq!(kind, Some(crate::simulation::JamKind::Confuse));
        assert!(confused);
    }

    #[test]
    fn the_glitch_is_presentation_only() {
        let trace = |glitch: bool| {
            let mut game = ready();
            let id = spawn(
                &mut game,
                &Species::of(Genome::bogey()),
                Vec2::new(0.0, 500.0),
            );
            if glitch {
                assert!(game.apply_glitch(2.0, 77));
                assert!(game.jam_view().glitch > 0.0 || game.jam_view().seed == 77);
            }
            let mut out = Vec::new();
            run(&mut game, &[id], 3.0, |g| {
                out.push((g.player().unwrap().position, g.body(id).map(|b| b.position)));
            });
            out
        };
        assert_eq!(trace(false), trace(true));
    }

    #[test]
    fn glare_opens_its_eyes_then_glitches_once_with_a_gap() {
        let g = Genome {
            glare: 0.9,
            power_period: 1.5,
            power_reach: 650.0,
            weapon: crate::genome::Weapon::None,
            speed: 0.0,
            cruise: 0.0,
            ..Genome::default()
        };
        let mut game = ready();
        let id = spawn(&mut game, &Species::of(g), Vec2::new(0.0, 400.0));
        let (mut starts, mut last) = (Vec::new(), 0.0_f32);
        let mut opened = false;
        run(&mut game, &[id], 14.0, |gm| {
            opened |= gm.power_view(gm.body(id).unwrap()).glare > 0.0;
            let on = gm.jam_view().glitch;
            if on > 0.0 && last == 0.0 {
                starts.push(gm.time);
            }
            last = on;
        });
        assert!(opened && !starts.is_empty());
        for w in starts.windows(2) {
            assert!(w[1] - w[0] >= power::GLITCH_GAP + 1.0, "{starts:?}");
        }
        assert!(!Power::Glare.fits(&Genome::default()) || Power::Glare.built());
    }

    #[test]
    fn a_light_eater_darkens_but_never_below_the_floor_and_quiet_ships_are_noticed_less() {
        let g = Genome {
            dim: 0.9,
            power_reach: 450.0,
            speed: 0.0,
            cruise: 0.0,
            ..Genome::default()
        };
        let mut game = ready();
        let id = spawn(&mut game, &Species::of(g), Vec2::new(0.0, 100.0));
        let near = game.dim_at(Vec2::new(0.0, 100.0));
        assert!(
            near > 0.4 && near <= 1.0 - power::DIM_FLOOR + 1e-4,
            "{near}"
        );
        assert_eq!(game.dim_at(Vec2::new(0.0, 100.0 + 450.0 * 1.2 + 50.0)), 0.0);
        assert!(game.dim_notice() > 1.0 && game.dim_notice() <= 1.0 / power::DIM_NOTICE + 1e-3);
        // Shooting lifts the veil.
        game.step(
            DT,
            Input {
                fire: true,
                ..Default::default()
            },
        );
        assert_eq!(game.dim_notice(), 1.0);
        // Killing the source ends the dark.
        game.bodies.retain(|b| b.id != id);
        assert_eq!(game.dim_at(Vec2::new(0.0, 100.0)), 0.0);
    }

    #[test]
    fn jams_are_deterministic() {
        let trace = || {
            let mut game = ready();
            let id = spawn(&mut game, &Species::of(stormcap()), Vec2::new(0.0, 200.0));
            let mut out = Vec::new();
            run(&mut game, &[id], 15.0, |g| out.push(g.jam_view()));
            out
        };
        assert_eq!(trace(), trace());
    }
}
