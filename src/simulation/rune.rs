//! Stationary sigil mines. Ordinary mines retain their own countdown and blast rules.
use super::powers::PowerState;
use super::*;
use crate::power::Power;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payload {
    Blast,
    Slow,
    Push,
    Jam,
}
impl Payload {
    /// A carrier keeps one readable payload for its lifetime. No random stream is consumed.
    pub fn from_gene(rune: f32) -> Self {
        match ((rune * 131.0).fract() * 4.0) as u8 {
            0 => Self::Blast,
            1 => Self::Slow,
            2 => Self::Push,
            _ => Self::Jam,
        }
    }
    pub fn color(self) -> [f32; 3] {
        match self {
            Self::Blast => [1.0, 0.25, 0.2],
            Self::Slow => [0.2, 0.65, 1.0],
            Self::Push => [0.95, 0.98, 1.0],
            Self::Jam => [0.8, 0.35, 1.0],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sigil {
    pub owner: u64,
    pub payload: Payload,
    pub shot: bool,
    /// A newly cast warning starts aging on the following simulation tick.
    pub fresh: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuneField {
    pub owner: u64,
    pub position: Vec2,
    pub payload: Payload,
    pub left: f32,
    pub age: f32,
}

impl Game {
    fn rune_owner_live(b: &Body) -> bool {
        b.kind == BodyKind::Creature
            && b.active
            && b.health > 0.0
            && !b.consumed
            && !b.follower
            && b.position.is_finite()
            && Power::Rune.active(&b.genome)
    }
    pub(super) fn cleanup_runes(&mut self) {
        if self.rune_fields.is_empty() && !self.mines.iter().any(|m| m.sigil.is_some()) {
            return;
        }
        let live: Vec<_> = self
            .bodies
            .iter()
            .filter(|b| Self::rune_owner_live(b))
            .map(|b| b.id)
            .collect();
        let active = &self.active;
        self.mines.retain(|m| {
            m.sigil.is_none_or(|s| {
                live.contains(&s.owner)
                    && m.position.is_finite()
                    && active.contains(&SectorId::containing(m.position))
            })
        });
        self.rune_fields.retain(|f| {
            live.contains(&f.owner)
                && f.position.is_finite()
                && active.contains(&SectorId::containing(f.position))
                && f.left > 0.0
        });
    }
    pub(super) fn step_rune(&mut self, index: usize, state: &mut PowerState, dt: f32) {
        state.rune_clock -= dt;
        if state.rune_clock > 0.0 {
            return;
        }
        let body = &self.bodies[index];
        let (owner, at, g) = (body.id, body.position, body.genome);
        state.rune_clock = g.power_params(Power::Rune).period;
        if body.phased
            || body.contact_cooldown > 0.0
            || !body.alert
            || self.sanctuary && self.sector() == SectorId::ORIGIN
            || self.is_landed()
            || self.player_invulnerability > 0.0
        {
            return;
        }
        let sector = SectorId::containing(at);
        let builders = self
            .bodies
            .iter()
            .filter(|b| {
                b.active
                    && !b.follower
                    && Power::Rune.active(&b.genome)
                    && SectorId::containing(b.position) == sector
                    && b.id < owner
            })
            .count();
        if builders >= 2 {
            return;
        }
        let Some(ship) = self.player().map(|p| p.position) else {
            return;
        };
        if ship.distance(at) > g.power_params(Power::Rune).reach || !at.is_finite() {
            return;
        }
        self.cleanup_runes();
        if self.mines.len() >= self.tune.weapon_max_mines
            || self.mines.iter().filter(|m| m.sigil.is_some()).count() + self.rune_fields.len()
                >= self.tune.rune_global_cap
            || self
                .mines
                .iter()
                .filter(|m| m.sigil.is_some_and(|s| s.owner == owner))
                .count()
                + self.rune_fields.iter().filter(|f| f.owner == owner).count()
                >= self.tune.rune_owner_cap
        {
            return;
        }
        // First candidate targets the current position, never a future tracking position.
        // Rejected candidates are fixed salted offsets, so body iteration order is irrelevant.
        for k in 0..12 {
            let h =
                crate::world::hash2(self.seed ^ 0x5255_4E45 ^ owner, state.rune_casts as i32, k);
            let angle = (h >> 40) as f32 / 16_777_216.0 * TAU;
            let offset = if k == 0 {
                Vec2::ZERO
            } else {
                Vec2::from_angle(angle) * (130.0 + k as f32 * 14.0)
            };
            let target = ship + offset;
            if target.distance(at) > g.power_params(Power::Rune).reach
                || target.distance(at) < self.tune.rune_radius + self.bodies[index].radius + 30.0
                || !self.active.contains(&SectorId::containing(target))
            {
                continue;
            }
            if self.bodies.iter().any(|b| {
                b.active
                    && is_fixed(b)
                    && b.id != owner
                    && b.position.distance(target) < b.radius + self.tune.rune_radius + 24.0
            }) || self
                .mines
                .iter()
                .any(|m| m.position.distance(target) < 2.0 * self.tune.rune_radius + 50.0)
                || self
                    .mines
                    .iter()
                    .filter(|m| {
                        m.sigil.is_some()
                            && SectorId::containing(m.position) == SectorId::containing(target)
                    })
                    .count()
                    + self
                        .rune_fields
                        .iter()
                        .filter(|f| {
                            SectorId::containing(f.position) == SectorId::containing(target)
                        })
                        .count()
                    >= self.tune.rune_sector_cap
            {
                continue;
            }
            self.lay_mine(Mine {
                sigil: Some(Sigil {
                    owner,
                    payload: Payload::from_gene(g.rune),
                    shot: false,
                    fresh: true,
                }),
                position: target,
                velocity: Vec2::ZERO,
                friendly: false,
                age: 0.0,
                fuse: Some(self.tune.rune_arm),
                damage: 30.0 * self.bodies[index].genes.sharpness(),
                blast: self.tune.rune_radius,
            });
            state.rune_casts = state.rune_casts.wrapping_add(1);
            self.cue(Cue::RuneTell { at: target });
            break;
        }
    }
    pub(super) fn update_rune_mines(&mut self, dt: f32) {
        self.cleanup_runes();
        let mut bursts = Vec::new();
        for mine in self.mines.iter_mut().filter(|m| m.sigil.is_some()) {
            if mine.sigil.as_ref().unwrap().fresh {
                mine.sigil.as_mut().unwrap().fresh = false;
                continue;
            }
            // A sigil sits still unless something throws it (a pushwhale's shove).
            mine.velocity *= (-1.4 * dt).exp();
            mine.position += mine.velocity * dt;
            mine.age += dt;
            mine.fuse =
                (mine.age < self.tune.rune_arm).then_some((self.tune.rune_arm - mine.age).max(0.0));
            if mine.age >= self.tune.rune_life {
                continue;
            }
            if mine.age >= self.tune.rune_arm
                && (mine.sigil.unwrap().shot
                    || self.bodies.iter().any(|b| {
                        b.active
                            && b.health > 0.0
                            && !b.phased
                            && !b.consumed
                            && matches!(b.kind, BodyKind::Player | BodyKind::Creature)
                            && b.position.distance(mine.position) < mine.blast + b.radius
                    }))
            {
                bursts.push(mine.clone());
                mine.age = self.tune.rune_life;
            }
        }
        self.mines
            .retain(|m| m.sigil.is_none() || m.age < self.tune.rune_life);
        for mine in bursts {
            let sigil = mine.sigil.unwrap();
            if !self.body(sigil.owner).is_some_and(Self::rune_owner_live) {
                continue;
            }
            self.cue(Cue::RuneFire { at: mine.position });
            if self.rune_fields.len() < self.tune.rune_global_cap {
                self.rune_fields.push(RuneField {
                    owner: sigil.owner,
                    position: mine.position,
                    payload: sigil.payload,
                    age: 0.0,
                    left: if sigil.payload == Payload::Slow {
                        self.tune.rune_slow_life
                    } else {
                        0.45
                    },
                });
            }
            match sigil.payload {
                Payload::Blast => {
                    self.rune_blast(mine.position, mine.blast, mine.damage, sigil.shot)
                }
                Payload::Slow => {}
                Payload::Push => {
                    let invulnerability = self.player_invulnerability;
                    for body in self
                        .bodies
                        .iter_mut()
                        .filter(|b| b.active && !b.phased && !is_fixed(b))
                    {
                        if body.position.distance(mine.position) >= mine.blast + body.radius
                            || (body.kind == BodyKind::Player && invulnerability > 0.0)
                        {
                            continue;
                        }
                        let out = (body.position - mine.position)
                            .try_normalize()
                            .unwrap_or(Vec2::X);
                        let ballast = if body.rig.ballast { 0.2 } else { 1.0 };
                        body.velocity = (body.velocity + out * self.tune.rune_push * ballast)
                            .clamp_length_max(300.0);
                        if body.kind != BodyKind::Player {
                            body.rune_pushed = self.tune.rune_slow_life;
                        }
                    }
                }
                Payload::Jam => {
                    if self
                        .player()
                        .is_some_and(|p| p.position.distance(mine.position) < mine.blast + p.radius)
                        && self.player_invulnerability <= 0.0
                        && !self.parry_active()
                    {
                        let candidates = self.jam_candidates();
                        let system = candidates[(sigil.owner as usize) % candidates.len()];
                        self.apply_jam(&[system], 0.8);
                    }
                }
            }
        }
        self.cleanup_runes();
    }
    /// Environmental blasts hit friend and foe, with deliberate shooting claiming credit.
    pub(super) fn rune_blast(&mut self, at: Vec2, radius: f32, amount: f32, credited: bool) {
        let invulnerability = self.player_invulnerability;
        let mut notes = Vec::new();
        for body in self
            .bodies
            .iter_mut()
            .filter(|b| b.active && !b.phased && b.health > 0.0)
        {
            if body.position.distance(at) >= radius + body.radius
                || body.kind == BodyKind::BlackHole
                || (body.kind == BodyKind::Asteroid
                    && matches!(body.rock, RockKind::Wall | RockKind::Planetoid))
                || body.kind == BodyKind::Base
            {
                continue;
            }
            let resist = if credited {
                self.apexes
                    .adapt
                    .get(&body.id)
                    .map_or(1.0, |r| r.scale(arsenal::Family::Explosive, &self.tune))
            } else {
                1.0
            };
            let taken = damage(
                body,
                armored(body, amount * resist, credited, &self.tune),
                if body.kind == BodyKind::Player {
                    invulnerability
                } else {
                    0.0
                },
                &self.tune,
            );
            if !credited && taken > 0.0 && body.kind == BodyKind::Asteroid {
                body.rune_pushed = body.rune_pushed.max(self.tune.rune_slow_life);
            }
            if body.health <= 0.0 && body.kind != BodyKind::Player {
                body.hostile_rock_kill = !credited;
            }
            if credited && body.kind == BodyKind::Creature {
                self.run.damage_dealt += taken;
                if diplomacy::civil_target(body) {
                    self.civs.hits.push((body.id, taken));
                }
                notes.push((body.id, taken, body.max_health + body.max_shield));
            }
        }
        for (id, dealt, pool) in notes {
            self.note_family_hit(id, arsenal::Family::Explosive, dealt, pool);
        }
    }
    pub(super) fn update_rune_fields(&mut self, dt: f32) {
        self.cleanup_runes();
        if self.rune_fields.is_empty() {
            return;
        }
        let rate = 1.0 - (-6.0 * dt).exp();
        for body in self
            .bodies
            .iter_mut()
            .filter(|b| b.active && !b.phased && !is_fixed(b))
        {
            if !self.rune_fields.iter().any(|f| {
                f.payload == Payload::Slow
                    && body.position.distance(f.position) < self.tune.rune_radius + body.radius
            }) {
                continue;
            }
            if body.kind == BodyKind::Player && self.player_invulnerability > 0.0 {
                continue;
            }
            let top = match body.kind {
                BodyKind::Player => self.stats.top_speed,
                BodyKind::Creature => body.genome.speed.max(body.genome.cruise),
                _ => 120.0,
            } * self.tune.rune_slow_floor;
            let speed = body.velocity.length();
            if speed > top {
                body.velocity *= 1.0 + (top / speed - 1.0) * rate;
            }
        }
        for field in &mut self.rune_fields {
            field.left -= dt;
            field.age += dt;
        }
        self.rune_fields.retain(|f| f.left > 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Species;
    use crate::simulation::tests::{DT, add, body, empty_game, set_player, spawn};
    fn arena(payload: Payload, at: Vec2) -> (Game, u64) {
        let mut game = empty_game();
        let id = spawn(
            &mut game,
            &Species::of(Genome::runekeeper()),
            Vec2::new(450.0, 0.0),
        );
        let owner = game.bodies.iter_mut().find(|b| b.id == id).unwrap();
        owner.pinned = true;
        owner.alert = true;
        game.mines.push(Mine {
            position: at,
            velocity: Vec2::ZERO,
            friendly: false,
            age: 0.0,
            fuse: Some(DEFAULT_TUNING.rune_arm),
            damage: 40.0,
            blast: DEFAULT_TUNING.rune_radius,
            sigil: Some(Sigil {
                owner: id,
                payload,
                shot: false,
                fresh: false,
            }),
        });
        (game, id)
    }
    fn tick(game: &mut Game, seconds: f32) {
        for _ in 0..(seconds / DT).ceil() as usize {
            game.update_mines(DT);
            game.update_rune_fields(DT);
        }
    }
    #[test]
    fn arming_warns_for_full_duration_then_fires_only_once() {
        let (mut g, _) = arena(Payload::Blast, Vec2::ZERO);
        let before = g.player().unwrap().shield;
        tick(&mut g, DEFAULT_TUNING.rune_arm - 2.0 * DT);
        assert_eq!(g.player().unwrap().shield, before);
        assert!(g.mines[0].fuse.unwrap() > 0.0);
        tick(&mut g, 3.0 * DT);
        assert!(g.mines.is_empty());
        assert_eq!(g.player().unwrap().shield, before - 40.0);
        tick(&mut g, 1.0);
        assert_eq!(g.player().unwrap().shield, before - 40.0);
        assert_eq!(
            g.drain_cues()
                .iter()
                .filter(|c| matches!(c, Cue::RuneFire { .. }))
                .count(),
            1
        );
    }
    #[test]
    fn an_armed_sigil_waits_and_expires_without_firing() {
        let (mut g, _) = arena(Payload::Blast, Vec2::new(0.0, 500.0));
        tick(&mut g, 2.0);
        assert_eq!(g.mines.len(), 1);
        assert_eq!(g.mines[0].fuse, None);
        tick(&mut g, DEFAULT_TUNING.rune_life);
        assert!(g.mines.is_empty());
        assert!(
            !g.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::RuneFire { .. }))
        );
    }
    #[test]
    fn leaving_the_circle_and_luring_a_pursuer_counters_it() {
        let (mut g, _) = arena(Payload::Blast, Vec2::ZERO);
        let before = g.player().unwrap().shield;
        tick(&mut g, 0.4);
        set_player(&mut g, Vec2::new(0.0, -400.0), Vec2::ZERO);
        tick(&mut g, 1.0);
        assert_eq!(g.player().unwrap().shield, before);
        assert_eq!(g.mines.len(), 1);
        let foe = spawn(&mut g, &Species::fatso(), Vec2::ZERO);
        let hull = body(&g, foe).health;
        tick(&mut g, DT);
        assert!(g.mines.is_empty());
        assert!(body(&g, foe).health < hull);
    }
    #[test]
    fn bare_thrust_can_escape_from_the_center_before_activation() {
        for payload in [Payload::Blast, Payload::Slow, Payload::Push, Payload::Jam] {
            let (mut g, owner) = arena(payload, Vec2::ZERO);
            let before = g.player().unwrap().shield;
            for _ in 0..80 {
                g.bodies
                    .retain(|b| b.kind == BodyKind::Player || b.id == owner);
                g.step(
                    DT,
                    Input {
                        thrust: 0.6,
                        move_direction: Some(Vec2::Y * 0.6),
                        ..Default::default()
                    },
                );
            }
            assert!(
                g.player().unwrap().position.length()
                    > DEFAULT_TUNING.rune_radius + g.player().unwrap().radius
            );
            assert_eq!(g.player().unwrap().shield, before);
            assert!(!g.jam_view().any());
        }
    }
    #[test]
    fn shooting_before_arming_queues_detonation_without_shortening_the_warning() {
        let (mut g, _) = arena(Payload::Blast, Vec2::new(0.0, 200.0));
        g.bullets.push(Bullet::friendly(
            Vec2::new(0.0, 150.0),
            Vec2::Y * 12000.0,
            1.0,
        ));
        g.move_bullets(DT);
        assert!(g.mines[0].sigil.unwrap().shot);
        tick(&mut g, DEFAULT_TUNING.rune_arm - 2.0 * DT);
        assert_eq!(g.mines.len(), 1);
        tick(&mut g, 3.0 * DT);
        assert!(g.mines.is_empty());
    }
    #[test]
    fn shooting_armed_sigils_detonates_immediately_and_still_hurts_the_ship() {
        let (mut g, _) = arena(Payload::Blast, Vec2::new(0.0, 200.0));
        tick(&mut g, 2.0);
        set_player(&mut g, Vec2::new(0.0, 180.0), Vec2::ZERO);
        let shield = g.player().unwrap().shield;
        g.bullets.push(Bullet::friendly(
            Vec2::new(0.0, 160.0),
            Vec2::Y * 12000.0,
            1.0,
        ));
        g.move_bullets(DT);
        tick(&mut g, DT);
        assert!(g.mines.is_empty());
        assert_eq!(g.player().unwrap().shield, shield - 40.0);
    }
    #[test]
    fn blast_friendly_fire_includes_caster_and_phase_spares_a_body() {
        let (mut g, owner) = arena(Payload::Blast, Vec2::ZERO);
        g.bodies
            .iter_mut()
            .find(|b| b.id == owner)
            .unwrap()
            .position = Vec2::new(40.0, 0.0);
        let phased = spawn(&mut g, &Species::fatso(), Vec2::new(-40.0, 0.0));
        g.bodies.iter_mut().find(|b| b.id == phased).unwrap().phased = true;
        let hull = body(&g, phased).health;
        tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
        assert!(body(&g, owner).health < body(&g, owner).max_health);
        assert_eq!(body(&g, phased).health, hull);
    }
    #[test]
    fn natural_kills_pay_no_credit_but_a_deliberate_shot_claims_the_blast() {
        for shot in [false, true] {
            let (mut g, _) = arena(Payload::Blast, Vec2::ZERO);
            let foe = spawn(&mut g, &Species::bogey(), Vec2::new(30.0, 0.0));
            g.bodies.iter_mut().find(|b| b.id == foe).unwrap().health = 1.0;
            g.mines[0].sigil.as_mut().unwrap().shot = shot;
            tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
            assert!(body(&g, foe).health <= 0.0);
            g.remove_destroyed();
            assert_eq!(g.run.kills > 0, shot);
            assert_eq!(g.score > 0, shot);
            if !shot {
                assert!(g.pickups.is_empty());
            }
        }
    }
    #[test]
    fn environmental_hit_shatters_crystal_without_a_cascade_or_player_credit() {
        let (mut g, _) = arena(Payload::Blast, Vec2::ZERO);
        let rock = add(&mut g, BodyKind::Asteroid, Vec2::new(40.0, 0.0));
        let crystal = g.bodies.iter_mut().find(|b| b.id == rock).unwrap();
        crystal.rock = RockKind::Crystal;
        crystal.health = 1.0;
        let foe = spawn(&mut g, &Species::bogey(), Vec2::new(140.0, 0.0));
        g.bodies.iter_mut().find(|b| b.id == foe).unwrap().health = 1.0;
        tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
        g.remove_destroyed();
        assert!(body(&g, foe).health > 0.0);
        g.remove_destroyed();
        assert_eq!(g.run.kills, 0);
        assert_eq!(g.score, 0);
        assert!(g.pickups.is_empty());
    }
    #[test]
    fn slow_patch_is_short_nonstacking_and_never_brakes_below_the_floor() {
        let (mut g, owner) = arena(Payload::Slow, Vec2::ZERO);
        tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
        g.bodies
            .iter_mut()
            .find(|b| b.kind == BodyKind::Player)
            .unwrap()
            .velocity = Vec2::X * g.stats.top_speed;
        let field = g.rune_fields[0];
        for _ in 0..5 {
            g.rune_fields.push(field);
        }
        for _ in 0..60 {
            g.update_rune_fields(DT);
            assert!(
                g.player().unwrap().velocity.length()
                    >= g.stats.top_speed * DEFAULT_TUNING.rune_slow_floor
            );
        }
        assert!(g.player().unwrap().velocity.length() < g.stats.top_speed * 0.7);
        tick(&mut g, 1.0);
        assert!(g.rune_fields.is_empty());
        assert!(g.body(owner).is_some());
    }
    #[test]
    fn push_is_outward_and_caps_final_speed() {
        let (mut g, _) = arena(Payload::Push, Vec2::ZERO);
        set_player(&mut g, Vec2::new(20.0, 0.0), Vec2::X * 600.0);
        let foe = spawn(&mut g, &Species::fatso(), Vec2::new(-30.0, 0.0));
        tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
        assert!(g.player().unwrap().velocity.x > 0.0);
        assert!(body(&g, foe).velocity.x < 0.0);
        assert!(g.player().unwrap().velocity.length() <= 300.0);
        assert_eq!(
            impact::kinetic_damage(300.0, 0.1, 0.1, &DEFAULT_TUNING),
            0.0
        );
    }
    #[test]
    fn jam_takes_one_owned_system_and_obeys_immunity() {
        let (mut g, _) = arena(Payload::Jam, Vec2::ZERO);
        tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
        assert!(g.jammed(JamSystem::Weapons));
        assert_eq!(g.jam_view().weapons, 0.8);
        assert_eq!(g.jam_view().dash, 0.0);
        assert_eq!(g.jam_view().parry, 0.0);
        assert!(!g.apply_jam(&[JamSystem::Dash], 0.8));
        g.update_jam(0.81, false);
        assert_eq!(g.jam_immunity(), crate::power::JAM_IMMUNITY);
        assert!(!g.apply_jam(&[JamSystem::Weapons], 0.8));
    }
    #[test]
    fn jam_respects_dash_parry_grace_and_faraday() {
        use super::super::organs::{Organ, Strain};
        use super::super::skills::Skill;
        for protection in 0..5 {
            let (mut g, _) = arena(Payload::Jam, Vec2::ZERO);
            match protection {
                0 => {
                    g.player_invulnerability = 0.2;
                }
                1 => {
                    g.loadout.skills.raise(Skill::Parry);
                    assert!(g.parry());
                }
                2 => {
                    g.player_invulnerability = 2.0;
                }
                3 => {
                    g.pad.landed = Some((SectorId::ORIGIN, 0));
                }
                _ => {
                    let strain = Strain::from_donor(Organ::Faraday, &Genome::stormcap());
                    for _ in 0..3 {
                        g.loadout.organs.acquire(strain, &DEFAULT_TUNING);
                    }
                    g.loadout.skills.raise(Skill::Symbiosis);
                    g.bond(strain);
                    g.cargo.volatiles = 100.0;
                }
            }
            tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
            assert!(!g.jam_view().any(), "protection {protection}");
        }
    }
    #[test]
    fn cleanup_covers_dead_consumed_frozen_unloaded_and_powerless_owners() {
        for reason in 0..5 {
            let (mut g, owner) = arena(Payload::Slow, Vec2::ZERO);
            tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
            g.mines.push(Mine {
                sigil: Some(Sigil {
                    owner,
                    payload: Payload::Blast,
                    shot: false,
                    fresh: false,
                }),
                position: Vec2::ZERO,
                velocity: Vec2::ZERO,
                friendly: false,
                age: 0.0,
                fuse: Some(DEFAULT_TUNING.rune_arm),
                damage: 40.0,
                blast: DEFAULT_TUNING.rune_radius,
            });
            let b = g.bodies.iter_mut().find(|b| b.id == owner).unwrap();
            match reason {
                0 => b.health = 0.0,
                1 => b.consumed = true,
                2 => b.active = false,
                3 => {
                    g.bodies.retain(|b| b.id != owner);
                }
                _ => b.genome.rune = 0.0,
            }
            g.cleanup_runes();
            assert!(
                g.mines.is_empty() && g.rune_fields.is_empty(),
                "reason {reason}"
            );
        }
    }
    #[test]
    fn unloading_or_freezing_the_sigil_sector_cleans_it_without_detonation() {
        let (mut g, _) = arena(Payload::Blast, Vec2::ZERO);
        g.active.clear();
        g.cleanup_runes();
        assert!(g.mines.is_empty());
        assert!(g.rune_fields.is_empty());
        assert!(
            !g.drain_cues()
                .iter()
                .any(|c| matches!(c, Cue::RuneFire { .. }))
        );
    }
    #[test]
    fn casts_place_one_per_period_inside_reach_with_stable_payload_and_no_rng_draws() {
        let trace = || {
            let (mut g, owner) = arena(Payload::Blast, Vec2::ZERO);
            g.mines.clear();
            let index = g.bodies.iter().position(|b| b.id == owner).unwrap();
            g.bodies[index].genome.volley = 2;
            let mut state = PowerState::default();
            let next = g.rng.clone().f32();
            let next_variation = g.variation.clone().f32();
            g.step_rune(index, &mut state, DT);
            assert_eq!(g.mines.len(), 1);
            assert!(
                g.mines[0].position.distance(g.bodies[index].position)
                    <= g.bodies[index].genome.power_params(Power::Rune).reach
            );
            assert!(
                g.drain_cues()
                    .iter()
                    .any(|c| matches!(c, Cue::RuneTell { .. }))
            );
            g.step_rune(index, &mut state, 1.0);
            assert_eq!(g.mines.len(), 1);
            assert_eq!(g.rng.f32(), next);
            assert_eq!(g.variation.f32(), next_variation);
            (g.mines[0].position, g.mines[0].sigil, state.rune_casts)
        };
        assert_eq!(trace(), trace());
    }
    #[test]
    fn owner_sector_global_and_shared_mine_budgets_refuse_new_casts() {
        for (reason, count) in [
            DEFAULT_TUNING.rune_owner_cap,
            DEFAULT_TUNING.rune_sector_cap,
            DEFAULT_TUNING.rune_global_cap,
            DEFAULT_TUNING.weapon_max_mines,
        ]
        .into_iter()
        .enumerate()
        {
            let (mut g, owner) = arena(Payload::Blast, Vec2::ZERO);
            g.mines.clear();
            for k in 0..count {
                let keeper = if reason == 0 || reason == 3 {
                    owner
                } else {
                    spawn(
                        &mut g,
                        &Species::of(Genome::runekeeper()),
                        Vec2::new(450.0, 0.0),
                    )
                };
                let position = if reason == 2 {
                    Vec2::new(6000.0 * (k + 1) as f32, 1000.0)
                } else {
                    Vec2::new(1000.0 + (k % 8) as f32 * 200.0, 1000.0)
                };
                g.active.push(SectorId::containing(position));
                g.mines.push(Mine {
                    sigil: (reason != 3).then_some(Sigil {
                        owner: keeper,
                        payload: Payload::Blast,
                        shot: false,
                        fresh: false,
                    }),
                    position,
                    velocity: Vec2::ZERO,
                    friendly: false,
                    age: 0.0,
                    fuse: None,
                    damage: 40.0,
                    blast: DEFAULT_TUNING.rune_radius,
                });
            }
            let index = g.bodies.iter().position(|b| b.id == owner).unwrap();
            g.step_rune(index, &mut PowerState::default(), DT);
            assert_eq!(g.mines.len(), count, "budget {reason}");
        }
    }
    #[test]
    fn a_pushed_body_hitting_a_fast_pursuer_does_not_pay_kill_credit() {
        let (mut g, _) = arena(Payload::Push, Vec2::ZERO);
        let pushed = spawn(&mut g, &Species::fatso(), Vec2::new(40.0, 0.0));
        tick(&mut g, DEFAULT_TUNING.rune_arm + DT);
        let victim = spawn(&mut g, &Species::bogey(), Vec2::new(250.0, 0.0));
        let ai = g.bodies.iter().position(|b| b.id == pushed).unwrap();
        let bi = g.bodies.iter().position(|b| b.id == victim).unwrap();
        g.bodies[bi].health = 1.0;
        g.bodies[bi].velocity = -Vec2::X * 600.0;
        let speed = (g.bodies[ai].velocity - g.bodies[bi].velocity).length();
        let raw = impact::kinetic_damage(
            speed,
            1.0 / g.bodies[ai].mass,
            1.0 / g.bodies[bi].mass,
            &DEFAULT_TUNING,
        );
        let (left, right) = g.bodies.split_at_mut(bi);
        impact::strike(&mut left[ai], &mut right[0], raw, 0.0, 1.0, &DEFAULT_TUNING);
        assert!(body(&g, victim).health <= 0.0);
        assert!(g.civs.hits.is_empty());
        g.remove_destroyed();
        assert_eq!(g.score, 0);
        assert_eq!(g.run.kills, 0);
        assert!(g.pickups.is_empty());
    }
    #[test]
    fn active_fields_share_the_owner_budget_and_two_builders_per_sector() {
        let (mut g, owner) = arena(Payload::Slow, Vec2::ZERO);
        g.mines.clear();
        for _ in 0..DEFAULT_TUNING.rune_owner_cap {
            g.rune_fields.push(RuneField {
                owner,
                position: Vec2::ZERO,
                payload: Payload::Slow,
                left: DEFAULT_TUNING.rune_slow_life,
                age: 0.0,
            });
        }
        let index = g.bodies.iter().position(|b| b.id == owner).unwrap();
        g.step_rune(index, &mut PowerState::default(), DT);
        assert!(g.mines.is_empty());
        g.rune_fields.clear();
        let _second = spawn(
            &mut g,
            &Species::of(Genome::runekeeper()),
            Vec2::new(-400.0, 0.0),
        );
        let third = spawn(
            &mut g,
            &Species::of(Genome::runekeeper()),
            Vec2::new(0.0, 400.0),
        );
        let index = g.bodies.iter().position(|b| b.id == third).unwrap();
        g.bodies[index].alert = true;
        g.step_rune(index, &mut PowerState::default(), DT);
        assert!(g.mines.is_empty());
    }
    #[test]
    fn real_dash_at_an_armed_jam_destination_protects_the_ability() {
        let (mut g, _) = arena(Payload::Jam, Vec2::ZERO);
        g.loadout.skills.raise(super::super::skills::Skill::Dash);
        set_player(&mut g, -Vec2::X * 240.0, Vec2::ZERO);
        g.mines[0].age = DEFAULT_TUNING.rune_arm;
        g.mines[0].fuse = None;
        assert!(g.dash(Some(Vec2::X)));
        assert!(g.player().unwrap().position.length() < 1.0);
        g.update_mines(DT);
        assert!(g.mines.is_empty());
        assert!(!g.jam_view().any());
    }
    #[test]
    fn salted_fallback_casts_are_spaced_clear_and_do_not_draw_from_existing_streams() {
        let trace = || {
            let (mut g, owner) = arena(Payload::Blast, Vec2::ZERO);
            let obstacle = add(&mut g, BodyKind::Asteroid, Vec2::new(40.0, 0.0));
            let solid = g.bodies.iter_mut().find(|b| b.id == obstacle).unwrap();
            solid.radius = 40.0;
            solid.pinned = true;
            let index = g.bodies.iter().position(|b| b.id == owner).unwrap();
            let mut state = PowerState::default();
            let expected = g.rng.clone().f32();
            for _ in 0..3 {
                state.rune_clock = 0.0;
                g.step_rune(index, &mut state, DT);
            }
            assert!(state.rune_casts > 0, "fallback found room");
            assert_eq!(g.rng.f32(), expected);
            for (i, mine) in g.mines.iter().enumerate().skip(1) {
                assert!(mine.position.length() > 0.0, "k0 was occupied");
                assert!(
                    mine.position.distance(body(&g, obstacle).position)
                        >= 40.0 + DEFAULT_TUNING.rune_radius + 24.0
                );
                assert!(
                    mine.position.distance(body(&g, owner).position)
                        <= body(&g, owner).genome.power_params(Power::Rune).reach
                );
                for prior in &g.mines[..i] {
                    assert!(
                        mine.position.distance(prior.position)
                            >= 2.0 * DEFAULT_TUNING.rune_radius + 50.0
                    );
                }
            }
            g.mines
                .iter()
                .map(|m| (m.position, m.sigil))
                .collect::<Vec<_>>()
        };
        assert_eq!(trace(), trace());
    }
    #[test]
    fn full_step_multicast_encounters_repeat_for_all_four_payloads() {
        for payload in [Payload::Blast, Payload::Slow, Payload::Push, Payload::Jam] {
            let trace = || {
                let (mut g, owner) = arena(payload, Vec2::new(0.0, 600.0));
                g.mines.clear();
                let i = [Payload::Blast, Payload::Slow, Payload::Push, Payload::Jam]
                    .iter()
                    .position(|p| *p == payload)
                    .unwrap();
                let caster = g.bodies.iter_mut().find(|b| b.id == owner).unwrap();
                caster.genome.rune = (91.125 + i as f32 * 0.25) / 131.0;
                caster.genome.power_params_mut(Power::Rune).period = 1.5;
                caster.genome.speed = 0.0;
                let mut out = Vec::new();
                let mut tells = 0;
                for k in 0..480 {
                    g.bodies
                        .retain(|b| b.kind == BodyKind::Player || b.id == owner);
                    g.step(
                        DT,
                        Input {
                            move_direction: Some(Vec2::Y * if k % 120 < 60 { 0.05 } else { -0.05 }),
                            ..Default::default()
                        },
                    );
                    tells += g
                        .drain_cues()
                        .iter()
                        .filter(|c| matches!(c, Cue::RuneTell { .. }))
                        .count();
                    out.push((
                        g.player().unwrap().position,
                        g.mines
                            .iter()
                            .map(|m| (m.position, m.age, m.sigil))
                            .collect::<Vec<_>>(),
                        g.rune_fields.clone(),
                        g.jam_view(),
                    ));
                }
                assert!(tells >= 2, "multiple casts {payload:?}");
                out
            };
            assert_eq!(trace(), trace());
        }
    }
    #[test]
    fn a_cast_keeps_the_full_warning_between_placement_and_activation() {
        let (mut g, owner) = arena(Payload::Blast, Vec2::ZERO);
        g.mines.clear();
        let index = g.bodies.iter().position(|b| b.id == owner).unwrap();
        g.step_rune(index, &mut PowerState::default(), DT);
        g.update_mines(DT);
        assert_eq!(g.mines[0].age, 0.0);
        tick(&mut g, DEFAULT_TUNING.rune_arm - DT);
        assert_eq!(g.mines.len(), 1);
        tick(&mut g, 2.0 * DT);
        assert!(g.mines.is_empty());
    }
    #[test]
    fn simultaneous_blasts_cancel_a_dead_owners_queued_activation() {
        let (mut g, _) = arena(Payload::Blast, Vec2::ZERO);
        let second = spawn(
            &mut g,
            &Species::of(Genome::runekeeper()),
            Vec2::new(35.0, 0.0),
        );
        g.bodies.iter_mut().find(|b| b.id == second).unwrap().health = 1.0;
        g.mines[0].age = DEFAULT_TUNING.rune_arm;
        let mut mine = g.mines[0].clone();
        mine.sigil.as_mut().unwrap().owner = second;
        mine.sigil.as_mut().unwrap().shot = true;
        mine.position = Vec2::new(0.0, 250.0);
        g.mines.push(mine);
        g.update_mines(DT);
        assert!(body(&g, second).health <= 0.0);
        assert!(g.mines.is_empty());
        assert_eq!(g.rune_fields.len(), 1);
        assert_eq!(
            g.drain_cues()
                .iter()
                .filter(|c| matches!(c, Cue::RuneFire { .. }))
                .count(),
            1
        );
    }
    #[test]
    fn payload_bands_and_specimen_are_readable_and_live() {
        for (i, payload) in [Payload::Blast, Payload::Slow, Payload::Push, Payload::Jam]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                Payload::from_gene((91.125 + i as f32 * 0.25) / 131.0),
                payload
            );
        }
        assert!(Power::Rune.built());
        let specimen = Genome::runekeeper();
        assert_eq!(specimen.weapon, crate::genome::Weapon::Mine);
        assert_eq!(specimen.volley, 1);
        assert_eq!(Power::Rune.first_ring(), 6);
    }
}
