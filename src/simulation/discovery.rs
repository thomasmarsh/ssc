//! Bounded sonar curiosity. Live handles never become persistent travel destinations.
use super::lure::{Lure, LureKind};
use super::ping::{ECHO_LIFE, Echo, EchoKind, Ring};
use super::*;
use crate::well::Mode;

/// Hard budget independent of target upgrades: two pairs, two wells, two relics.
pub const BUDGET: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Target {
    Rift {
        owner: u64,
        a: Vec2,
        b: Vec2,
        expires: f32,
        mouth: u8,
    },
    Well(u64),
    Relic {
        sector: SectorId,
        live: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Info {
    Rift {
        pair: u64,
        mouth: u8,
        warning: bool,
        seconds: f32,
        partner: Vec2,
    },
    Well {
        mode: Mode,
        holding: bool,
        carried: bool,
        fading: bool,
    },
    Relic {
        organ: organs::Organ,
    },
}
impl Info {
    pub fn tint(self) -> [f32; 3] {
        match self {
            Self::Rift { mouth: 0, .. } => [0.3, 0.95, 1.0],
            Self::Rift { .. } | Self::Relic { .. } => [1.0, 0.75, 0.35],
            Self::Well { mode, .. } => mode.tint(),
        }
    }
    pub fn label(self) -> String {
        match self {
            Self::Rift {
                pair,
                mouth,
                warning,
                seconds,
                ..
            } => format!(
                "R{}{} {} {:.1}s",
                pair,
                if mouth == 0 { "A" } else { "B" },
                if warning { "FORMING" } else { "OPEN" },
                seconds
            ),
            Self::Well {
                mode,
                holding,
                carried,
                fading,
            } => format!(
                "{} WELL{}",
                if carried {
                    "CARRIED"
                } else if fading {
                    "FADING"
                } else {
                    mode.label()
                }
                .to_uppercase(),
                if holding { " WAITING" } else { "" }
            ),
            Self::Relic { organ } => format!("SEALED {}", organ.label()),
        }
    }
}
impl Game {
    pub(super) fn resolve_discovery(&self, target: Target) -> Option<(Vec2, Info)> {
        match target {
            Target::Rift {
                owner,
                a,
                b,
                expires,
                mouth,
            } => {
                let r = self
                    .rifts
                    .iter()
                    .find(|r| r.owner == owner && r.a == a && r.b == b)?;
                let body = self.bodies.iter().find(|p| p.id == owner)?;
                if (self.time + r.warning + r.left - expires).abs() > 0.05
                    || !a.is_finite()
                    || !b.is_finite()
                    || !body.position.is_finite()
                    || r.left <= 0.0
                    || !body.active
                    || body.health <= 0.0
                    || body.consumed
                    || body.follower
                    || !crate::power::Power::Rift.active(&body.genome)
                    || !self.active.contains(&SectorId::containing(a))
                    || !self.active.contains(&SectorId::containing(b))
                {
                    return None;
                }
                Some((
                    if mouth == 0 { a } else { b },
                    Info::Rift {
                        pair: owner,
                        mouth,
                        warning: r.warning > 0.0,
                        seconds: if r.warning > 0.0 { r.warning } else { r.left },
                        partner: if mouth == 0 { b } else { a },
                    },
                ))
            }
            Target::Well(id) => {
                let body = self.bodies.iter().find(|b| b.id == id)?;
                if !body.active || body.health <= 0.0 || body.consumed || !body.position.is_finite()
                {
                    return None;
                }
                if body.kind == BodyKind::Creature
                    && crate::power::Power::Devour.active(&body.genome)
                    && self.apexes.power.get(&id).is_some_and(|s| s.pocket > 0.0)
                    && !body.follower
                {
                    return Some((
                        body.position,
                        Info::Well {
                            mode: Mode::Drift,
                            holding: false,
                            carried: true,
                            fading: false,
                        },
                    ));
                }
                let run = body.well.as_ref()?;
                if body.kind != BodyKind::BlackHole
                    || run.eaten >= 1.0
                    || (matches!(run.genome.mode, Mode::Static | Mode::Maw) && run.decay <= 0.0)
                {
                    return None;
                }
                Some((
                    body.position,
                    Info::Well {
                        mode: run.genome.mode,
                        holding: run.holding,
                        carried: false,
                        fading: run.decay > 0.0,
                    },
                ))
            }
            Target::Relic { sector: id, live } => {
                if live && (!self.loaded.contains(&id) || !self.active.contains(&id)) {
                    return None;
                }
                if self.relics_taken.contains(&id) {
                    return None;
                }
                let (strain, generated) = organs::relic_of(
                    self.seed,
                    id,
                    world::latent(self.seed, id).depth,
                    &self.tune,
                )?;
                let position = if self.loaded.contains(&id) {
                    self.pickups
                        .iter()
                        .find(|p| p.relic == Some(id) && p.remaining > 0.0)?
                        .position
                } else {
                    generated
                };
                Some((
                    position,
                    Info::Relic {
                        organ: strain.organ,
                    },
                ))
            }
        }
        .filter(|(p, _)| p.is_finite())
    }

    fn discovery_radius(&self, target: Target) -> f32 {
        match target {
            Target::Rift { .. } => rift::RADIUS + 14.0,
            Target::Well(id) => self.bodies.iter().find(|b| b.id == id).map_or(0.0, |b| {
                b.radius + b.well.as_ref().map_or(0.0, |w| w.pose.core) + 12.0
            }),
            Target::Relic { .. } => 12.0,
        }
    }

    pub(super) fn discovery_valid(&self, echo: &Echo) -> bool {
        echo.target
            .is_none_or(|t| self.resolve_discovery(t).is_some())
    }

    pub(super) fn refresh_discovery(&self, mut echo: Echo) -> Option<Echo> {
        let Some(target) = echo.target else {
            return Some(echo);
        };
        let (position, info) = self.resolve_discovery(target)?;
        let scan = echo.scan?;
        // In-flight echoes follow the actual target. No prediction of a held hop or rift.
        if !echo.sounded {
            if self.time > scan.started + scan.range / scan.speed + 0.05 {
                return None;
            }
            let distance = position.distance(scan.origin);
            if distance > scan.range {
                return None;
            }
            echo.born = scan.started + distance / scan.speed;
        }
        if self.time >= echo.born + ECHO_LIFE {
            return None;
        }
        echo.position = position;
        echo.radius = self.discovery_radius(target);
        echo.discovery = Some(info);
        echo.tint = Some(info.tint());
        Some(echo)
    }

    pub(super) fn discovery_candidates(&self, scan: Ring) -> Vec<Echo> {
        let mut targets = Vec::new();
        let mut pairs: Vec<_> = self.rifts.iter().collect();
        pairs.sort_by(|a, b| {
            a.a.distance_squared(scan.origin)
                .min(a.b.distance_squared(scan.origin))
                .total_cmp(
                    &b.a.distance_squared(scan.origin)
                        .min(b.b.distance_squared(scan.origin)),
                )
                .then(a.owner.cmp(&b.owner))
        });
        let mut accepted = 0;
        for r in pairs {
            if accepted == 2 {
                break;
            }
            // Reveal the pair together only if both mouths are reachable and available.
            if [r.a, r.b]
                .iter()
                .any(|p| p.distance(scan.origin) > scan.range)
            {
                continue;
            }
            let pair: Vec<_> = (0..2)
                .map(|mouth| Target::Rift {
                    owner: r.owner,
                    a: r.a,
                    b: r.b,
                    expires: self.time + r.warning + r.left,
                    mouth,
                })
                .collect();
            if pair.iter().all(|&t| self.resolve_discovery(t).is_some()) {
                targets.extend(pair);
                accepted += 1;
            }
        }
        let mut wells: Vec<_> = self
            .bodies
            .iter()
            .filter_map(|b| {
                let target = Target::Well(b.id);
                self.resolve_discovery(target)
                    .filter(|(p, _)| p.distance(scan.origin) <= scan.range)
                    .map(|(p, _)| (p.distance_squared(scan.origin), b.id, target))
            })
            .collect();
        wells.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        targets.extend(wells.into_iter().take(2).map(|(_, _, t)| t));
        if self.loadout.skills.level(skills::Skill::EchoLodes) > 0 {
            let home = SectorId::containing(scan.origin);
            let search = (scan.range / world::SECTOR_SIZE) as i32;
            let mut relics = Vec::new();
            for dx in -search..=search {
                for dy in -search..=search {
                    let id = SectorId {
                        x: home.x + dx,
                        y: home.y + dy,
                    };
                    if let Some((p, _)) = self.resolve_discovery(Target::Relic {
                        sector: id,
                        live: self.loaded.contains(&id),
                    }) && p.distance(scan.origin) <= scan.range
                    {
                        relics.push((p.distance_squared(scan.origin), id));
                    }
                }
            }
            relics.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            targets.extend(relics.into_iter().take(2).map(|(_, id)| Target::Relic {
                sector: id,
                live: self.loaded.contains(&id),
            }));
        }
        let mut echoes = Vec::new();
        for target in targets {
            let Some((position, info)) = self.resolve_discovery(target) else {
                continue;
            };
            let kind = match info {
                Info::Rift { .. } => EchoKind::Rift,
                Info::Well { .. } => EchoKind::Well,
                Info::Relic { .. } => EchoKind::Relic,
            };
            if echoes.iter().any(|e: &Echo| e.target == Some(target)) {
                continue;
            }
            echoes.push(Echo {
                kind,
                position,
                radius: self.discovery_radius(target),
                tint: Some(info.tint()),
                weight: 0.0,
                renewable: false,
                born: scan.started + position.distance(scan.origin) / scan.speed,
                sounded: false,
                target: Some(target),
                scan: Some(scan),
                discovery: Some(info),
            });
        }
        debug_assert!(echoes.len() <= BUDGET);
        echoes
    }

    pub(super) fn curiosity_lure(&self) -> Option<Lure> {
        let ship = self.player()?.position;
        self.echoes()
            .filter_map(|(e, _)| {
                let kind = match e.discovery? {
                    Info::Relic { .. } => LureKind::Relic,
                    Info::Rift { warning: false, .. } => LureKind::Rift,
                    Info::Well { .. } => LureKind::Well,
                    _ => return None,
                };
                Some(Lure {
                    kind,
                    position: e.position,
                })
            })
            .filter(|l| l.position.distance(ship) > lure::ARRIVED)
            .min_by(|a, b| {
                lure::lure_score(a.kind, a.position.distance(ship))
                    .total_cmp(&lure::lure_score(b.kind, b.position.distance(ship)))
            })
    }
}

#[cfg(feature = "desktop")]
impl Game {
    /// Bounded renderer gallery only. Called once by SSC_DISCOVERY under SSC_SMOKE_FRAMES.
    pub fn stage_discovery_smoke(&mut self, mode: &str) -> bool {
        if !matches!(mode, "warning" | "active" | "well" | "relic" | "crowded") {
            return false;
        }
        let id = (10..20)
            .flat_map(|x| (0..10).map(move |y| SectorId { x, y }))
            .find(|&id| {
                organs::relic_of(
                    self.seed,
                    id,
                    world::latent(self.seed, id).depth,
                    &self.tune,
                )
                .is_some()
            })
            .unwrap();
        let (_, relic) = organs::relic_of(
            self.seed,
            id,
            world::latent(self.seed, id).depth,
            &self.tune,
        )
        .unwrap();
        let ship = relic - Vec2::new(450.0, 170.0);
        self.teleport(ship);
        self.set_auto_ping(false);
        self.player_invulnerability = 1e9;
        self.step(0.02, Input::default());
        self.bodies.retain(|b| b.kind == BodyKind::Player);
        self.bullets.clear();
        self.mines.clear();
        self.tethers.clear();
        self.rifts.clear();
        self.pickups
            .retain(|p| p.relic == Some(id) && matches!(mode, "relic" | "crowded"));
        self.lure.lure = None;
        if matches!(mode, "warning" | "active" | "crowded") {
            let owner = self.place_creature(
                &crate::genome::Species::of(Genome::seamer()),
                ship + Vec2::Y * 200.0,
            );
            self.bodies
                .iter_mut()
                .find(|b| b.id == owner)
                .unwrap()
                .pinned = true;
            self.rifts.push(Rift {
                owner,
                a: ship - Vec2::X * 475.0,
                b: ship + Vec2::X * if mode == "crowded" { 1200.0 } else { 475.0 },
                warning: rift::WARNING,
                left: rift::LIFE,
            });
        }
        if matches!(mode, "well" | "crowded") {
            let anchor = ship + Vec2::new(if mode == "crowded" { -1200.0 } else { -380.0 }, -210.0);
            let mut body = self.make_body(BodyKind::BlackHole, anchor);
            body.well = Some(WellRun::new(
                &crate::well::SectorWell {
                    index: 0,
                    anchor,
                    genome: crate::well::WellGenome {
                        mode: Mode::Drift,
                        swing: 150.0,
                        period: 60.0,
                        ..crate::well::WellGenome::PLAIN
                    },
                },
                self.time,
            ));
            self.bodies.push(body);
        }
        if matches!(mode, "relic" | "crowded") {
            self.loadout.skills.raise(skills::Skill::EchoLodes);
        }
        self.ping.cooldown = 0.0;
        self.ping();
        // Gallery isolates the new glyphs but retains the guaranteed civilization bearing.
        self.ping
            .echoes
            .retain(|e| e.discovery.is_some() || e.kind == EchoKind::Nearest);
        for _ in 0..if mode == "warning" { 36 } else { 90 } {
            self.step(1.0 / 60.0, Input::default());
        }
        for k in 0..8 {
            self.bullets.push(Bullet::hostile(
                ship + Vec2::new(-240.0 + k as f32 * 65.0, -35.0),
                Vec2::Y * 300.0,
                2.0,
                10.0,
            ));
        }
        if mode == "crowded" {
            self.mines.push(Mine {
                sigil: None,
                position: ship + Vec2::new(220.0, -200.0),
                velocity: Vec2::ZERO,
                friendly: false,
                age: 1.0,
                fuse: Some(0.7),
                damage: 40.0,
                blast: 90.0,
            });
        }
        self.player_invulnerability = 0.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::skills::Skill;
    use super::super::tests::{add, empty_game};
    use super::*;
    use crate::genome::{Genome, Species};
    use crate::well::{SectorWell, WellGenome};

    fn pair(g: &mut Game, x: f32) -> u64 {
        let owner = g.place_creature(&Species::of(Genome::seamer()), Vec2::new(x, 250.0));
        g.rifts.push(Rift {
            owner,
            a: Vec2::new(x, 0.0),
            b: Vec2::new(x + 1000.0, 0.0),
            warning: rift::WARNING,
            left: rift::LIFE,
        });
        owner
    }
    fn well(g: &mut Game, mode: Mode, x: f32) -> u64 {
        let anchor = Vec2::new(x, -500.0);
        let id = add(g, BodyKind::BlackHole, anchor);
        g.bodies.iter_mut().find(|b| b.id == id).unwrap().well = Some(WellRun::new(
            &SectorWell {
                index: 0,
                anchor,
                genome: WellGenome {
                    mode,
                    swing: 200.0,
                    period: 60.0,
                    ..WellGenome::PLAIN
                },
            },
            g.time,
        ));
        id
    }
    fn tick(g: &mut Game, seconds: f32) {
        let mut left = seconds;
        while left > 0.000001 {
            let dt = left.min(0.02);
            g.time += dt;
            g.update_rifts(dt);
            g.update_wells(dt);
            g.update_ping(dt);
            left -= dt;
        }
    }
    fn discovered(g: &Game, kind: EchoKind) -> Vec<Echo> {
        g.echoes()
            .filter(|(e, _)| e.kind == kind)
            .map(|(e, _)| *e)
            .collect()
    }
    fn relic_sector(g: &Game) -> SectorId {
        (2..15)
            .flat_map(|x| (-10..10).map(move |y| SectorId { x, y }))
            .find(|&id| {
                organs::relic_of(g.seed, id, world::latent(g.seed, id).depth, &DEFAULT_TUNING)
                    .is_some()
            })
            .unwrap()
    }
    fn local_relic(g: &mut Game) -> SectorId {
        let id = relic_sector(g);
        let (_, at) =
            organs::relic_of(g.seed, id, world::latent(g.seed, id).depth, &DEFAULT_TUNING).unwrap();
        g.bodies[0].position = at - Vec2::new(1500.0, 0.0);
        g.active.push(id);
        g.loaded.insert(id);
        g.place_relic(id);
        g.loadout.skills.raise(Skill::EchoLodes);
        id
    }

    #[test]
    fn base_ping_reveals_live_pairs_and_dynamic_modes_but_not_static_or_maw() {
        let mut g = empty_game();
        pair(&mut g, 500.0);
        for (i, mode) in [Mode::Static, Mode::Maw, Mode::Pulse, Mode::Reverse]
            .into_iter()
            .enumerate()
        {
            well(&mut g, mode, 600.0 + i as f32 * 200.0);
        }
        assert!(g.ping());
        assert!(g.echoes().all(|(e, _)| e.discovery.is_none()));
        tick(&mut g, 0.4);
        assert_eq!(discovered(&g, EchoKind::Rift).len(), 2);
        let wells = discovered(&g, EchoKind::Well);
        assert_eq!(wells.len(), 2);
        assert!(wells.iter().all(|e| matches!(
            e.discovery,
            Some(Info::Well {
                mode: Mode::Pulse | Mode::Reverse,
                ..
            })
        )));
    }
    #[test]
    fn relics_require_the_existing_lodes_upgrade() {
        let mut g = empty_game();
        let id = relic_sector(&g);
        g.bodies[0].position = id.center();
        g.ping();
        assert!(!g.ping.echoes.iter().any(|e| e.kind == EchoKind::Relic));
        g.loadout.skills.raise(Skill::EchoNests);
        g.ping.cooldown = 0.0;
        g.ping();
        assert!(!g.ping.echoes.iter().any(|e| e.kind == EchoKind::Relic));
        g.loadout.skills.raise(Skill::EchoLodes);
        g.ping.cooldown = 0.0;
        g.ping();
        assert!(g.ping.echoes.iter().any(|e| e.kind == EchoKind::Relic));
    }
    #[test]
    fn rift_echo_arrives_at_the_ring_and_has_paired_identity() {
        let mut g = empty_game();
        let owner = pair(&mut g, 700.0);
        g.ping();
        tick(&mut g, 0.09);
        assert!(discovered(&g, EchoKind::Rift).is_empty());
        tick(&mut g, 0.02);
        let echo = discovered(&g, EchoKind::Rift)[0];
        assert!(
            matches!(echo.discovery, Some(Info::Rift { pair, mouth: 0, warning: true, partner, .. }) if pair == owner && partner == Vec2::new(1700.0, 0.0))
        );
        tick(&mut g, 0.2);
        assert_eq!(discovered(&g, EchoKind::Rift).len(), 2);
        let n = g
            .drain_cues()
            .into_iter()
            .filter(|c| matches!(c, Cue::Echo { .. }))
            .count();
        tick(&mut g, 0.2);
        assert_eq!(
            g.drain_cues()
                .into_iter()
                .filter(|c| matches!(c, Cue::Echo { .. }))
                .count(),
            0
        );
        assert!(n >= 2);
    }
    #[test]
    fn warning_becomes_active_without_a_second_ping_or_duplicate() {
        let mut g = empty_game();
        pair(&mut g, 700.0);
        g.ping();
        tick(&mut g, 0.4);
        let before = discovered(&g, EchoKind::Rift);
        tick(&mut g, 0.9);
        let after = discovered(&g, EchoKind::Rift);
        assert_eq!(before.len(), after.len());
        assert_eq!(before[0].born, after[0].born);
        assert!(
            after
                .iter()
                .all(|e| matches!(e.discovery, Some(Info::Rift { warning: false, .. })))
        );
    }
    #[test]
    fn moving_well_arrival_uses_live_distance_and_then_follows_motion() {
        let mut g = empty_game();
        let id = well(&mut g, Mode::Drift, 2000.0);
        g.ping();
        let initial = g
            .ping
            .echoes
            .iter()
            .find(|e| e.kind == EchoKind::Well)
            .unwrap()
            .born;
        g.bodies.iter_mut().find(|b| b.id == id).unwrap().position = Vec2::new(3500.0, 0.0);
        g.time = 0.35;
        g.update_ping(0.35);
        assert!(discovered(&g, EchoKind::Well).is_empty());
        assert!(initial < 0.35);
        g.time = 0.51;
        g.update_ping(0.16);
        let arrived = discovered(&g, EchoKind::Well)[0];
        assert_eq!(arrived.position, Vec2::new(3500.0, 0.0));
        g.bodies.iter_mut().find(|b| b.id == id).unwrap().position = Vec2::new(4000.0, 0.0);
        g.time = 0.7;
        g.update_ping(0.19);
        let moved = discovered(&g, EchoKind::Well)[0];
        assert_eq!(moved.born, arrived.born);
        assert_eq!(moved.position, Vec2::new(4000.0, 0.0));
    }
    #[test]
    fn pending_and_sounded_targets_die_freeze_or_unload_silently() {
        for sounded in [false, true] {
            for reason in 0..4 {
                let mut g = empty_game();
                let owner = pair(&mut g, 700.0);
                let hole = well(&mut g, Mode::Drift, 1500.0);
                g.ping();
                if sounded {
                    tick(&mut g, 0.4);
                }
                g.drain_cues();
                match reason {
                    0 => {
                        for b in g
                            .bodies
                            .iter_mut()
                            .filter(|b| b.id == owner || b.id == hole)
                        {
                            b.health = 0.0;
                        }
                    }
                    1 => {
                        for b in g
                            .bodies
                            .iter_mut()
                            .filter(|b| b.id == owner || b.id == hole)
                        {
                            b.active = false;
                        }
                    }
                    2 => g.bodies.retain(|b| b.id != owner && b.id != hole),
                    _ => {
                        for b in g
                            .bodies
                            .iter_mut()
                            .filter(|b| b.id == owner || b.id == hole)
                        {
                            b.consumed = true;
                        }
                    }
                }
                g.time += 0.4;
                g.update_ping(0.4);
                assert!(discovered(&g, EchoKind::Rift).is_empty());
                assert!(discovered(&g, EchoKind::Well).is_empty());
                assert!(
                    !g.ping
                        .echoes
                        .iter()
                        .any(|e| matches!(e.kind, EchoKind::Rift | EchoKind::Well))
                );
            }
        }
    }
    #[test]
    fn either_mouth_sector_unavailability_hides_the_whole_pair() {
        let mut g = empty_game();
        g.active.push(SectorId { x: 1, y: 0 });
        pair(&mut g, 2600.0);
        g.ping();
        assert_eq!(
            g.ping
                .echoes
                .iter()
                .filter(|e| e.kind == EchoKind::Rift)
                .count(),
            2
        );
        g.active.retain(|id| *id != SectorId { x: 1, y: 0 });
        tick(&mut g, 0.7);
        assert!(discovered(&g, EchoKind::Rift).is_empty());
    }
    #[test]
    fn expiry_and_replacement_cannot_restore_an_old_destination() {
        let mut g = empty_game();
        let owner = pair(&mut g, 700.0);
        g.ping();
        tick(&mut g, 0.4);
        let mut replacement = g.rifts[0];
        replacement.left = 8.0;
        replacement.warning = 1.2;
        g.rifts[0] = replacement;
        g.update_ping(0.0);
        assert!(discovered(&g, EchoKind::Rift).is_empty());
        g.ping.cooldown = 0.0;
        g.ping();
        tick(&mut g, 1.2);
        tick(&mut g, 8.1);
        assert!(discovered(&g, EchoKind::Rift).is_empty());
        assert!(g.rifts.iter().all(|r| r.owner != owner));
    }
    #[test]
    fn relic_collection_clears_echo_chart_and_reload_without_changing_harvest() {
        let mut g = empty_game();
        let id = local_relic(&mut g);
        g.ping();
        tick(&mut g, 0.3);
        assert!(
            discovered(&g, EchoKind::Relic)
                .iter()
                .any(|e| matches!(e.target, Some(Target::Relic { sector, .. }) if sector == id))
        );
        assert_eq!(g.chart_entry(id).unwrap().relics, 1);
        let p = g
            .pickups
            .iter()
            .find(|p| p.relic == Some(id))
            .unwrap()
            .position;
        g.bodies[0].position = p;
        g.update_pickups(0.02);
        g.update_ping(0.0);
        assert!(g.relics_taken.contains(&id));
        assert_eq!(g.chart_entry(id).unwrap().relics, 0);
        g.place_relic(id);
        assert!(!g.pickups.iter().any(|p| p.relic == Some(id)));
        assert!(
            !discovered(&g, EchoKind::Relic)
                .iter()
                .any(|e| matches!(e.target, Some(Target::Relic { sector, .. }) if sector == id))
        );
    }
    #[test]
    fn magnet_motion_uses_provenance_and_never_places_a_duplicate_relic() {
        let mut g = empty_game();
        let id = local_relic(&mut g);
        g.ping();
        let pickup = g.pickups.iter_mut().find(|p| p.relic == Some(id)).unwrap();
        pickup.position += Vec2::Y * 250.0;
        let position = pickup.position;
        g.place_relic(id);
        tick(&mut g, 0.5);
        assert_eq!(g.pickups.iter().filter(|p| p.relic == Some(id)).count(), 1);
        assert!(
            discovered(&g, EchoKind::Relic)
                .iter()
                .any(|e| e.position == position)
        );
    }
    #[test]
    fn a_loaded_relic_that_expires_or_unloads_before_arrival_is_silent() {
        for unload in [false, true] {
            let mut g = empty_game();
            let id = local_relic(&mut g);
            g.ping();
            if unload {
                g.loaded.remove(&id);
            } else {
                g.pickups.retain(|p| p.relic != Some(id));
            }
            tick(&mut g, 0.5);
            assert!(
                !discovered(&g, EchoKind::Relic).iter().any(
                    |e| matches!(e.target, Some(Target::Relic { sector, .. }) if sector == id)
                )
            );
        }
    }
    #[test]
    fn far_relics_are_generated_facts_and_never_load_a_sector() {
        let mut g = empty_game();
        let id = relic_sector(&g);
        g.bodies[0].position = id.center();
        g.loadout.skills.raise(Skill::EchoLodes);
        let loaded = g.loaded.clone();
        g.ping();
        tick(&mut g, 0.5);
        assert_eq!(g.loaded, loaded);
        assert!(discovered(&g, EchoKind::Relic).iter().any(
            |e| matches!(e.target, Some(Target::Relic { sector, live: false }) if sector == id)
        ));
        assert_eq!(g.chart_entry(id).unwrap().relics, 1);
    }
    #[test]
    fn curiosity_budgets_do_not_scale_with_target_upgrades_or_duplicate_pings() {
        let mut g = empty_game();
        local_relic(&mut g);
        // The pairs live in the active arena, separate from the far relic search.
        g.bodies[0].position = Vec2::ZERO;
        for i in 0..5 {
            pair(&mut g, 500.0 + i as f32 * 100.0);
            well(&mut g, Mode::Pulse, 800.0 + i as f32 * 100.0);
        }
        for _ in 0..4 {
            g.loadout.skills.raise(Skill::PingTargets);
        }
        for _ in 0..3 {
            g.ping.cooldown = 0.0;
            g.ping();
            let echoes: Vec<_> = g
                .ping
                .echoes
                .iter()
                .filter(|e| e.discovery.is_some())
                .collect();
            assert!(echoes.len() <= BUDGET);
            assert_eq!(
                echoes.iter().filter(|e| e.kind == EchoKind::Rift).count(),
                4
            );
            assert_eq!(
                echoes.iter().filter(|e| e.kind == EchoKind::Well).count(),
                2
            );
            for (i, a) in echoes.iter().enumerate() {
                assert!(!echoes[i + 1..].iter().any(|b| b.target == a.target));
            }
            assert_eq!(
                g.ping
                    .echoes
                    .iter()
                    .filter(|e| e.kind == EchoKind::Nearest)
                    .count(),
                1
            );
        }
    }
    #[test]
    fn curiosity_is_fallback_and_warning_is_never_a_travel_lure() {
        let mut g = empty_game();
        pair(&mut g, 700.0);
        g.ping();
        g.ping.echoes.retain(|e| e.discovery.is_some());
        tick(&mut g, 0.4);
        assert!(g.next_lure().is_none());
        tick(&mut g, 0.9);
        assert_eq!(g.next_lure().unwrap().kind, LureKind::Rift);
        let starter = Lure {
            kind: LureKind::Planetoid,
            position: Vec2::new(1800.0, 0.0),
        };
        g.consider_lure(starter);
        assert_eq!(g.next_lure(), Some(starter));
        g.lure.lure = None;
        g.rifts.clear();
        g.update_ping(0.0);
        assert!(g.next_lure().is_none());
    }
    #[test]
    fn sounded_echoes_expire_without_refreshing_lifetimes_as_wells_move() {
        let mut g = empty_game();
        well(&mut g, Mode::Drift, 1500.0);
        g.ping();
        tick(&mut g, 0.5);
        assert_eq!(discovered(&g, EchoKind::Well).len(), 1);
        tick(&mut g, ECHO_LIFE);
        assert!(discovered(&g, EchoKind::Well).is_empty());
    }
    #[test]
    fn generated_wells_chart_the_anchor_not_the_live_pose_and_eaten_marks_clear() {
        let mut g = empty_game();
        let id = well(&mut g, Mode::Drift, 1500.0);
        let b = g.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.origin = Some((SectorId::ORIGIN, 99));
        g.ping();
        tick(&mut g, 0.5);
        assert_eq!(g.chart_entry(SectorId::ORIGIN).unwrap().dynamic_wells, 1);
        tick(&mut g, 1.0);
        assert_eq!(g.chart_entry(SectorId::ORIGIN).unwrap().dynamic_wells, 1);
        g.fallen.entry(SectorId::ORIGIN).or_default().insert(99);
        assert_eq!(g.chart_entry(SectorId::ORIGIN).unwrap().dynamic_wells, 0);
        assert!(!g.chart_entries().iter().any(|e| e.relics > 0));
    }
    #[test]
    fn rifts_never_teach_persistent_map_destinations() {
        let mut g = empty_game();
        pair(&mut g, 700.0);
        g.ping();
        g.ping.echoes.retain(|e| e.kind == EchoKind::Rift);
        tick(&mut g, 0.4);
        assert!(g.chart_entries().is_empty());
    }
    #[test]
    fn repeated_full_step_traces_are_identical_and_scanning_draws_no_rng() {
        fn setup() -> Game {
            let mut g = empty_game();
            pair(&mut g, 700.0);
            well(&mut g, Mode::Drift, 1700.0);
            g
        }
        let (mut a, mut b) = (setup(), setup());
        for frame in 0..720 {
            if frame % 300 == 0 {
                a.ping();
                b.ping();
            }
            a.step(1.0 / 60.0, Input::default());
            b.step(1.0 / 60.0, Input::default());
            assert_eq!(a.ping.echoes, b.ping.echoes);
            assert_eq!(a.next_lure(), b.next_lure());
            assert_eq!(a.chart_entries(), b.chart_entries());
            assert_eq!(a.drain_cues(), b.drain_cues());
        }
        let (mut a, mut b) = (setup(), setup());
        a.ping();
        assert_eq!(a.rng.next_u64(), b.rng.next_u64());
    }
    #[test]
    fn hop_echo_reports_the_held_live_position_without_revealing_the_ghost() {
        let mut g = empty_game();
        let id = well(&mut g, Mode::Hop, 1500.0);
        let b = g.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.well.as_mut().unwrap().holding = true;
        b.well.as_mut().unwrap().pose.ghost = Some(Vec2::new(2600.0, 800.0));
        let live = b.position;
        g.ping();
        g.time = 0.4;
        g.update_ping(0.4);
        let echo = discovered(&g, EchoKind::Well)[0];
        assert_eq!(echo.position, live);
        assert!(matches!(
            echo.discovery,
            Some(Info::Well { holding: true, .. })
        ));
    }
    #[test]
    fn carried_and_released_wells_are_live_but_never_persistent_destinations() {
        let mut g = empty_game();
        let carrier = g.place_creature(&Species::of(Genome::tidegorger()), Vec2::new(1600.0, 0.0));
        g.apexes.power.entry(carrier).or_default().pocket = 0.2;
        g.ping();
        g.time = 0.4;
        g.update_ping(0.4);
        assert!(
            discovered(&g, EchoKind::Well)
                .iter()
                .any(|e| matches!(e.discovery, Some(Info::Well { carried: true, .. })))
        );
        assert!(g.chart_entries().iter().all(|e| e.dynamic_wells == 0));
        g.bodies
            .iter_mut()
            .find(|b| b.id == carrier)
            .unwrap()
            .health = 0.0;
        g.release_pocket(Vec2::new(1700.0, 0.0), 0.4);
        g.ping.cooldown = 0.0;
        g.ping();
        g.time = 0.8;
        g.update_ping(0.4);
        assert!(
            discovered(&g, EchoKind::Well)
                .iter()
                .any(|e| matches!(e.discovery, Some(Info::Well { fading: true, .. })))
        );
        assert!(g.chart_entries().iter().all(|e| e.dynamic_wells == 0));
    }
    #[test]
    fn leaving_scan_range_before_arrival_cancels_and_never_sounds_later() {
        let mut g = empty_game();
        let id = well(&mut g, Mode::Drift, 2000.0);
        g.ping();
        g.bodies.iter_mut().find(|b| b.id == id).unwrap().position = Vec2::new(21000.0, 0.0);
        g.time = 0.2;
        g.update_ping(0.2);
        assert!(!g.ping.echoes.iter().any(|e| e.kind == EchoKind::Well));
        g.bodies.iter_mut().find(|b| b.id == id).unwrap().position = Vec2::new(1000.0, 0.0);
        g.time = 0.4;
        g.update_ping(0.2);
        assert!(discovered(&g, EchoKind::Well).is_empty());
    }
    #[test]
    fn crowded_curiosity_keeps_nearest_civilization_and_arrow_budget() {
        let mut g = empty_game();
        pair(&mut g, 700.0);
        well(&mut g, Mode::Drift, 1500.0);
        g.loadout.skills.raise(Skill::EchoLodes);
        g.ping();
        tick(&mut g, 3.0);
        let near = g.nearest_civilization().unwrap();
        let truth = crate::territory::nearest_civilization(
            g.seed,
            Vec2::ZERO,
            &mut Default::default(),
            &|_| false,
        )
        .unwrap();
        assert!((near.sectors - truth.sector.center().length() / world::SECTOR_SIZE).abs() < 0.001);
        let arrows = g.echo_bearings(Vec2::ZERO, Vec2::new(300.0, 200.0));
        assert!(arrows.len() <= 4);
        assert!(matches!(
            arrows.first().unwrap().kind,
            GuideKind::Echo(EchoKind::Nearest, _)
        ));
        assert!(
            arrows
                .iter()
                .all(|b| !matches!(b.kind, GuideKind::Echo(EchoKind::Rift, _)))
        );
    }
    #[test]
    fn presentation_handles_invalid_positions_without_disclosing_a_pair() {
        let mut g = empty_game();
        let owner = pair(&mut g, 700.0);
        g.ping();
        g.bodies
            .iter_mut()
            .find(|b| b.id == owner)
            .unwrap()
            .position
            .x = f32::NAN;
        tick(&mut g, 0.4);
        assert!(discovered(&g, EchoKind::Rift).is_empty());
    }
    #[test]
    fn relic_fallback_beats_active_rifts_and_wells_but_preserves_normal_lures() {
        let mut g = empty_game();
        local_relic(&mut g);
        let at = g.player().unwrap().position;
        let owner = g.place_creature(&Species::of(Genome::seamer()), at + Vec2::Y * 200.0);
        g.rifts.push(Rift {
            owner,
            a: at + Vec2::X * 600.0,
            b: at + Vec2::X * 1000.0,
            warning: 0.0,
            left: 8.0,
        });
        g.ping();
        g.ping.echoes.retain(|e| e.discovery.is_some());
        tick(&mut g, 0.4);
        assert_eq!(g.next_lure().unwrap().kind, LureKind::Relic);
        let normal = Lure {
            kind: LureKind::Civilization,
            position: at + Vec2::X * 5000.0,
        };
        g.consider_lure(normal);
        assert_eq!(g.next_lure(), Some(normal));
    }
    #[test]
    fn rift_expiry_before_the_ring_arrives_never_teaches_or_sounds() {
        let mut g = empty_game();
        pair(&mut g, 1800.0);
        g.rifts[0].warning = 0.0;
        g.rifts[0].left = 0.1;
        g.ping();
        g.ping.echoes.retain(|e| e.kind == EchoKind::Rift);
        g.drain_cues();
        tick(&mut g, 0.5);
        assert!(g.echoes().next().is_none());
        assert!(g.chart_entries().is_empty());
        assert!(!g.drain_cues().iter().any(|c| matches!(c, Cue::Echo { .. })));
    }
    #[test]
    fn collection_before_arrival_cancels_but_an_ordinary_specimen_does_not_spend_a_relic() {
        let mut g = empty_game();
        let id = local_relic(&mut g);
        let p = g
            .pickups
            .iter()
            .find(|p| p.relic == Some(id))
            .unwrap()
            .clone();
        let ship = g.player().unwrap().position;
        g.drop_item(ship, Vec2::ZERO, p.item.clone());
        g.update_pickups(0.02);
        assert!(!g.relics_taken.contains(&id));
        g.ping();
        g.ping
            .echoes
            .retain(|e| matches!(e.target, Some(Target::Relic { sector, .. }) if sector == id));
        assert_eq!(g.ping.echoes.len(), 1);
        g.drain_cues();
        g.bodies[0].position = p.position;
        g.update_pickups(0.02);
        tick(&mut g, 0.5);
        assert!(g.relics_taken.contains(&id));
        assert!(g.ping.echoes.is_empty());
        assert!(!g.drain_cues().iter().any(|c| matches!(c, Cue::Echo { .. })));
    }
}
