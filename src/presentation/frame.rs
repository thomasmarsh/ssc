//! Per-frame gizmo draw system.
use super::*;

/// Everything the layer functions read from the frame setup.
#[derive(Clone, Copy)]
struct Frame<'a> {
    game: &'a Game,
    session: &'a Session,
    camera: Vec2,
    half: Vec2,
    jam: &'a ssc::simulation::JamView,
    screen: &'a crate::hud::Screen,
    window: Vec2,
}

pub fn draw(
    session: Res<Session>,
    view: Single<(&Transform, &Projection, &Camera), With<Camera2d>>,
    ui_scale: Res<UiScale>,
    mut gizmos: Gizmos,
    mut ship_view: Local<crate::shipview::ShipView>,
    mut plant_cache: Local<PlantCache>,
) {
    let game = &session.game;
    let camera = view.0.translation.truncate();
    let half = match view.1 {
        Projection::Orthographic(p) => p.area.half_size(),
        _ => Vec2::new(900.0, 450.0),
    };
    // The HUD is laid out in UI pixels (logical pixels over the UI scale): world units per
    // pixel follow from the view.
    let viewport = view
        .2
        .logical_viewport_size()
        .unwrap_or(Vec2::new(1200.0, 800.0));
    let screen = crate::hud::Screen::new(camera, half, viewport, ui_scale.0);
    let window = screen.size;
    let sky = if session.reduce_effects {
        ssc::backdrop::Backdrop::NEUTRAL
    } else {
        ssc::backdrop::backdrop_at(game.seed(), camera)
    };
    let dim = game.dim_sources();
    let dark = if dim.is_empty() {
        0.0
    } else {
        Game::dim_from(&dim, camera)
    };
    let jam = game.jam_view();
    draw_backdrop(&mut gizmos, camera, half, &sky, dark);
    let f = Frame {
        game,
        session: &session,
        camera,
        half,
        jam: &jam,
        screen: &screen,
        window,
    };
    draw_bodies(&mut gizmos, &f, &mut ship_view, &dim);
    draw_works(&mut gizmos, &f);
    draw_ground(&mut gizmos, &f, &mut plant_cache);
    draw_loot_and_rocks(&mut gizmos, &f);
    draw_tethers(&mut gizmos, &f);
    draw_shots_and_effects(&mut gizmos, &f);
    draw_ship_effects(&mut gizmos, &f);
    draw_screen_layer(&mut gizmos, &f);
}

/// Every body in view: the ship and its rig, creatures, rocks, drones.
fn draw_bodies(
    gizmos: &mut Gizmos,
    f: &Frame,
    ship_view: &mut crate::shipview::ShipView,
    dim: &[(Vec2, f32, f32)],
) {
    let Frame {
        game,
        session,
        camera,
        half,
        jam,
        ..
    } = *f;
    let pests = game.pest_targets();
    for body in game.bodies.iter().filter(|b| {
        // Cull on the body's full extent, not its center: a planetoid is hundreds of units
        // wide and must stay drawn while only its edge (or halo) is on screen.
        ssc::simulation::extent_in_view(b.position, body_draw_extent(b), camera, half, 120.0)
    }) {
        let p = body.position;
        let r = body.radius;
        if pests.contains(&body.id) {
            let mark = Color::srgb(1.0, 0.65, 0.15);
            let size = r + 14.0;
            for sign in [-1.0, 1.0] {
                let at = p + Vec2::new(sign * size, size);
                gizmos.line_2d(at, at - Vec2::Y * 10.0, mark);
                gizmos.line_2d(at, at - Vec2::X * sign * 10.0, mark);
                let at = p + Vec2::new(sign * size, -size);
                gizmos.line_2d(at, at + Vec2::Y * 10.0, mark);
                gizmos.line_2d(at, at - Vec2::X * sign * 10.0, mark);
            }
        }
        let direction = Vec2::from_angle(body.angle);
        let color = if body.kind == BodyKind::Asteroid && body.pinned {
            // A civilization's blocks wear its tint; other pinned stone is plain tan.
            match game.civ_tint(body) {
                Some(tint) => lifted(Some(tint)),
                None => Color::srgb(0.62, 0.5, 0.38),
            }
        } else {
            body_color(body)
        };
        match body.kind {
            BodyKind::Player => {
                ship_view.update(body, session.input, game.time);
                // Grace shows as a thin shell that thins out as it runs down; the ship itself
                // stays solid and readable instead of blinking away.
                if game.player_invulnerability > 0.0 {
                    let left = game.player_invulnerability.min(1.0);
                    let wobble = if session.reduce_effects {
                        1.0
                    } else {
                        0.8 + 0.2 * (game.time * 9.0).sin()
                    };
                    let shell = Color::srgba(0.55, 0.85, 1.0, 0.55 * left * wobble);
                    gizmos.circle_2d(p, r * 3.2, shell).resolution(40);
                    gizmos
                        .circle_2d(p, r * 3.6, shell.with_alpha(0.2 * left * wobble))
                        .resolution(40);
                }
                let active = !session.paused
                    && session.chart.is_none()
                    && session.settings.is_none()
                    && !game.game_over;
                let jets = ship_view.thrusters(session.input, body, game.stats.thrust, active);
                gizmos.linestrip_2d(crate::shipview::outline(p, r, ship_view.angle), color);
                draw_rig(gizmos, game, body, ship_view.angle, jets.main);
                crate::shipview::draw_thrusters(
                    gizmos,
                    body,
                    ship_view.angle,
                    &jets,
                    game.time,
                    session.reduce_effects,
                );
                if !session.reduce_effects {
                    crate::glitchview::ship_fringe(gizmos, jam, body, ship_view.angle);
                }
                crate::glitchview::confusion(gizmos, jam, body, game.time);
            }
            BodyKind::Creature if game.disguise(body).is_some() => {
                // A mimic: a plain rock, or a bright pickup hanging on a thin stalk. A crack
                // in the surface and a lit stalk give it away just before it shows itself.
                let crack = game.reveal_progress(body);
                match game.disguise(body) {
                    Some(ssc::simulation::Disguise::Lure) => {
                        let lure = Pickup {
                            relic: None,
                            position: p,
                            velocity: Vec2::ZERO,
                            item: Item::Material(ssc::simulation::Material::ALL[0], 8.0),
                            age: game.time,
                            remaining: 99.0,
                        };
                        draw_pickup(gizmos, &lure);
                        let stalk = Color::srgb(0.6, 0.7, 0.8).with_alpha(0.07 + 0.8 * crack);
                        gizmos.line_2d(p, p + direction * (r * 2.4 + 8.0), stalk);
                    }
                    _ => draw_rock(
                        gizmos,
                        game.time,
                        body,
                        Color::srgb(0.62, 0.5, 0.38),
                        &game.tune,
                    ),
                }
                if crack > 0.0 {
                    for k in 0..5 {
                        let a = k as f32 * 1.3 + body.id as f32;
                        gizmos.line_2d(
                            p,
                            p + Vec2::from_angle(a) * r * (0.6 + 0.6 * crack),
                            Color::WHITE.with_alpha(0.4 + 0.5 * crack),
                        );
                    }
                }
            }
            BodyKind::Creature => {
                // In a light eater's dark a creature draws fainter, never below 55 percent.
                let faint = if dim.is_empty() {
                    0.0
                } else {
                    Game::dim_from(dim, p) / (1.0 - ssc::power::DIM_FLOOR)
                };
                let shown = crate::powerview::outline(game, body, color);
                // A piece broken off a body fades as it drifts away.
                let shown = if body.adrift > 0.0 {
                    shown.with_alpha(
                        0.15 + 0.6 * (body.adrift / game.tune.pool_drift).clamp(0.0, 1.0),
                    )
                } else {
                    shown
                };
                let shown = if faint > 0.0 && !body.phased {
                    shown.with_alpha(1.0 - 0.45 * faint)
                } else {
                    shown
                };
                if body.genome.appearance.surface == ssc::development::Surface::Motes {
                    // A swarm is its motes (see `powerview`) around a small bright core.
                    gizmos
                        .circle_2d(p, r * ssc::power::CLOUD_CORE, shown)
                        .resolution(14);
                } else if body.genome.appearance.surface == ssc::development::Surface::Soft {
                    if game.power_view(body).ooze.is_some() {
                        crate::powerview::draw_ooze(gizmos, game, body, shown);
                    } else {
                        gizmos.lineloop_2d(
                            (0..48).map(|k| {
                                let a = k as f32 * std::f32::consts::TAU / 48.0;
                                p + Vec2::from_angle(a)
                                    * r
                                    * (1.0 + 0.04 * (a * 5.0 + game.time).sin())
                            }),
                            shown,
                        );
                    }
                } else {
                    draw_creature(gizmos, game.time, body, shown, &game.tune);
                }
                if let Some(back) = crate::powerview::afterimage(body) {
                    // A phased body trails a ghost of itself.
                    let mut ghost = body.clone();
                    ghost.position += back;
                    draw_creature(gizmos, game.time, &ghost, color.with_alpha(0.1), &game.tune);
                }
                crate::powerview::draw(gizmos, game, body);
                if !body.follower
                    && let Some([cr, cg, cb]) = game.civ_tint(body)
                {
                    // A faint banner ring: this one belongs to a civilization.
                    gizmos
                        .circle_2d(p, r * 1.3 + 9.0, Color::srgba(cr, cg, cb, 0.28))
                        .resolution(20);
                }
                if !body.follower
                    && let Some(archetype) = game.apex_archetype(body)
                {
                    // An apex elder: two slow golden crowns and spokes, unmistakable; the
                    // spoke count tells the archetype and the crown reddens once enraged.
                    let crown = if game.apex_enraged(body) {
                        APEX_ENRAGED
                    } else {
                        APEX_GOLD
                    };
                    let spin = game.time * 0.4;
                    for (k, grow) in [(0.0, 1.5), (1.0, 1.9)] {
                        let ring = r * grow + 14.0 + 4.0 * (game.time * 1.6 + k).sin();
                        gizmos
                            .circle_2d(p, ring, crown.with_alpha(0.5 - 0.15 * k))
                            .resolution(28);
                    }
                    let spokes = archetype.spokes();
                    for k in 0..spokes {
                        let a = spin + k as f32 * std::f32::consts::TAU / spokes as f32;
                        let d = Vec2::from_angle(a);
                        gizmos.line_2d(
                            p + d * (r * 1.9 + 18.0),
                            p + d * (r * 1.9 + 34.0),
                            crown.with_alpha(0.7),
                        );
                    }
                    if archetype == ssc::apex::Archetype::Bulwark && !game.apex_enraged(body) {
                        // The plated front arc, until it is shed.
                        let heading = body.angle;
                        for k in -6..=6 {
                            let a = heading + k as f32 * 0.17;
                            let d = Vec2::from_angle(a);
                            gizmos.line_2d(
                                p + d * (r + 3.0),
                                p + d * (r + 11.0),
                                Color::srgb(0.75, 0.78, 0.85),
                            );
                        }
                    }
                }
                if let Some(root) = body.root
                    && let Some(host) = game.body(root.host)
                {
                    draw_roots(gizmos, game.time, body, host, color);
                }
            }
            BodyKind::Base if body.fort.is_some() => {
                draw_turret(gizmos, game.time, body, lifted(game.civ_tint(body)));
            }
            BodyKind::Base => draw_station(gizmos, game.time, body, color, &game.tune),
            // Fortress walls are drawn together after the loop, with their joins.
            BodyKind::Asteroid if body.rock == RockKind::Wall => {}
            BodyKind::Asteroid => draw_rock(gizmos, game.time, body, color, &game.tune),
            BodyKind::BlackHole => crate::wellview::draw(gizmos, game, body),
        }
        // A hungry forager shows a faint amber ring that warms as its energy runs out.
        if body.kind == BodyKind::Creature && !body.follower && body.vigor(&game.tune) < 1.0 {
            let need = 1.0 - body.vigor(&game.tune);
            let pulse = if body.is_starving() {
                0.75 + 0.25 * (game.time * 4.0 + body.id as f32).sin()
            } else {
                1.0
            };
            gizmos
                .circle_2d(
                    p,
                    r * 1.35,
                    Color::srgba(1.0, 0.7, 0.25, (0.12 + 0.5 * need / 0.4) * pulse * 0.6),
                )
                .resolution(20);
        }
        if body.shield > 0.0 && body.max_shield > 0.0 && !body.follower {
            let fraction = body.shield / body.max_shield;
            gizmos
                .circle_2d(
                    p,
                    r * 1.7,
                    Color::srgba(0.3, 0.75, 1.0, 0.15 + fraction * 0.5),
                )
                .resolution(24);
        }
        if body.kind == BodyKind::Creature && !body.follower {
            draw_resistance(gizmos, game, body);
        }
    }
}

/// Walls, flocks, caches, miner beams, wrecks and mining drones.
fn draw_works(gizmos: &mut Gizmos, f: &Frame) {
    let Frame {
        game, camera, half, ..
    } = *f;
    draw_walls(gizmos, game, camera, half);
    crate::flockview::draw(gizmos, game, camera, half);
    for cache in game.caches() {
        if (cache.at - camera)
            .abs()
            .cmplt(half + Vec2::splat(120.0))
            .all()
        {
            draw_cache(gizmos, game.time, &cache);
        }
    }
    for (from, to, tint) in game.miner_beams() {
        let c = lifted(Some(tint));
        gizmos.line_2d(from, to, c.with_alpha(0.55));
        let t = (game.time * 2.5).fract();
        gizmos
            .circle_2d(from.lerp(to, t), 3.0, c.with_alpha(0.8))
            .resolution(6);
        gizmos.circle_2d(to, 7.0, c.with_alpha(0.4)).resolution(8);
    }
    for wreck in game.drone_wrecks() {
        if !ssc::simulation::extent_in_view(wreck.position, 24.0, camera, half, 0.0) {
            continue;
        }
        let p = wreck.position;
        let color = Color::srgb(1.0, 0.72, 0.25);
        gizmos.linestrip_2d(
            [
                p + Vec2::new(-12.0, 8.0),
                p + Vec2::new(0.0, 12.0),
                p + Vec2::new(7.0, 0.0),
            ],
            color,
        );
        gizmos.linestrip_2d(
            [
                p + Vec2::new(12.0, -8.0),
                p + Vec2::new(0.0, -12.0),
                p + Vec2::new(-7.0, 0.0),
            ],
            color,
        );
        gizmos
            .circle_2d(p, 18.0, color.with_alpha(0.35))
            .resolution(8);
    }
    for drone in game.mining_drone_views() {
        if !ssc::simulation::extent_in_view(drone.position, 24.0, camera, half, 0.0) {
            continue;
        }
        let p = drone.position;
        let forward = drone.heading;
        let side = forward.perp();
        let color = if drone.health < game.tune.fleet_drone_health * 0.5 {
            Color::srgb(1.0, 0.45, 0.15)
        } else if drone.powered {
            CYAN
        } else {
            MUTED
        };
        gizmos.lineloop_2d(
            [
                p + forward * 12.0,
                p - forward * 8.0 + side * 8.0,
                p - forward * 4.0,
                p - forward * 8.0 - side * 8.0,
            ],
            color,
        );
        if drone.cargo_pod {
            gizmos
                .circle_2d(p - forward * 3.0, 5.0, color)
                .resolution(6);
        }
        if drone.cargo > 0.0 {
            gizmos.line_2d(p - side * 4.0, p + side * 4.0, Color::srgb(1.0, 0.72, 0.25));
        }
        if drone.powered {
            use ssc::simulation::fleet::DronePhase;
            match drone.phase {
                DronePhase::Mining => {
                    gizmos.line_2d(
                        p + forward * 12.0,
                        drone.deposit,
                        color.with_alpha(if drone.mining_head { 0.9 } else { 0.45 }),
                    );
                }
                DronePhase::Launching | DronePhase::Returning => {
                    gizmos.line_2d(
                        p - forward * 8.0,
                        p - forward * 19.0,
                        Color::srgb(1.0, 0.45, 0.15),
                    );
                }
                DronePhase::Docked => {}
            }
        }
    }
}

/// Pads, landing, plankton, eggs, the mining beam, plants and symbiosis.
fn draw_ground(gizmos: &mut Gizmos, f: &Frame, plant_cache: &mut PlantCache) {
    let Frame {
        game, camera, half, ..
    } = *f;
    for pad in game.pads() {
        let at = game.pad_position(pad);
        if (at - camera).abs().cmplt(half + Vec2::splat(200.0)).all() {
            draw_pad(gizmos, game, pad, at);
        }
    }
    if game.is_landed()
        && let Some(ship) = game.player()
    {
        // Cover: a calm green shimmer while hidden, a restless amber flicker once exposed.
        let (shimmer, color) = if game.is_hidden() {
            (0.3 + 0.12 * (game.time * 2.0).sin(), PAD_GREEN)
        } else {
            (0.25 + 0.3 * (game.time * 16.0).sin().abs(), PAD_AMBER)
        };
        gizmos
            .circle_2d(ship.position, ship.radius * 2.3, color.with_alpha(shimmer))
            .resolution(28);
    }
    // Plankton: tiny pale-lime motes (distinct from the green gravity wells) that swell in when they bud and breathe gently.
    for food in game.food.iter().filter(|f| {
        (f.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(20.0))
            .all()
    }) {
        let phase = food.position.x * 0.013 + food.position.y * 0.007;
        let size = game.tune.food_radius
            * food.grown(&game.tune)
            * (1.0 + 0.12 * (game.time * 1.7 + phase).sin());
        let mote = Color::srgba(0.78, 0.95, 0.4, 0.65 * food.grown(&game.tune));
        gizmos.circle_2d(food.position, size, mote).resolution(8);
        gizmos.line_2d(
            food.position - Vec2::X * size * 1.6,
            food.position + Vec2::X * size * 1.6,
            Color::srgba(0.78, 0.95, 0.4, 0.18 * food.grown(&game.tune)),
        );
    }
    // Eggs: small speckled ovals in the parent's colors that wobble as they near hatching.
    for egg in game.eggs.iter().filter(|e| {
        (e.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(30.0))
            .all()
    }) {
        let [r, g, b] = egg.adult.color();
        let shell = Color::srgba(r, g, b, 0.85);
        let soon = ((egg.progress() - 0.8) / 0.2).clamp(0.0, 1.0);
        let wobble = soon * 0.35 * (game.time * 14.0 + egg.position.x).sin();
        let axis = Vec2::from_angle(wobble + 1.2);
        let side = Vec2::new(-axis.y, axis.x);
        let oval = (0..12).map(|i| {
            let t = i as f32 * std::f32::consts::TAU / 12.0;
            egg.position + axis * t.sin() * egg.radius * 1.25 + side * t.cos() * egg.radius * 0.9
        });
        gizmos.lineloop_2d(oval, shell);
        let core = 0.35 + 0.65 * egg.progress();
        gizmos
            .circle_2d(
                egg.position,
                egg.radius * 0.35 * core,
                shell.with_alpha(0.5),
            )
            .resolution(6);
        gizmos.line_2d(
            egg.position - side * egg.radius * 0.5,
            egg.position + side * egg.radius * 0.5,
            shell.with_alpha(0.35 * soon),
        );
    }
    if let (Some(beam), Some(ship)) = (&game.beam, game.player())
        && let Some(rock) = game.body(beam.target)
    {
        draw_beam(
            gizmos,
            game.time,
            ship,
            rock,
            beam,
            game.gripped() == Some(beam.target),
        );
    }
    draw_plants(gizmos, game, plant_cache);
    draw_symbiosis(gizmos, game);
}

/// Pickups, chains and their decoration, and loose rocks.
fn draw_loot_and_rocks(gizmos: &mut Gizmos, f: &Frame) {
    let Frame {
        game, camera, half, ..
    } = *f;
    for pickup in game.pickups.iter().filter(|p| {
        (p.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(40.0))
            .all()
    }) {
        draw_pickup(gizmos, pickup);
    }
    for chain in game.chains.values() {
        for part in &chain.parts {
            if let (Some(child), Some(parent)) =
                (game.body(part.id), part.parent.and_then(|id| game.body(id)))
            {
                gizmos.line_2d(
                    child.position,
                    parent.position,
                    body_color(child).with_alpha(0.6),
                );
            }
        }
    }
    // Marks of animal bodies (eyes, mounts, actuators, organs, fur, fins and so on); the
    // dressing takes the color of the body it hangs on.
    for deco in game.chain_decorations() {
        let Some(host) = game.body(deco.host) else {
            continue;
        };
        crate::bestiaryview::draw_mark(
            gizmos,
            deco.kind,
            deco.position,
            deco.angle,
            deco.length,
            deco.radius,
            body_color(host),
        );
    }
    for rock in game
        .bodies
        .iter()
        .filter(|b| b.active && b.sling_thrown > 0.0)
    {
        let direction = rock.velocity.normalize_or_zero();
        let color = Color::srgb(1.0, 0.65, 0.25);
        gizmos.line_2d(
            rock.position - direction * (rock.radius + 60.0),
            rock.position - direction * rock.radius,
            color,
        );
        gizmos
            .circle_2d(rock.position, rock.radius + 4.0, color.with_alpha(0.8))
            .resolution(24);
    }
}

/// Tethers between bodies.
fn draw_tethers(gizmos: &mut Gizmos, f: &Frame) {
    let Frame { game, .. } = *f;
    for tether in &game.tethers {
        if tether.health <= 0.0 {
            continue;
        }
        let Some((from, to)) = game.tether_ends(tether) else {
            continue;
        };
        if tether.kind == TetherKind::Sling {
            let tint = Color::srgb(1.0, 0.65, 0.25);
            let fray = (tether.health / tether.max_health).clamp(0.0, 1.0);
            gizmos.line_2d(from, to, tint.with_alpha(0.25 + 0.35 * fray));
            gizmos.circle_2d(to, 5.0, tint).resolution(12);
            continue;
        }
        if tether.kind == TetherKind::Web {
            let warning = tether.warning > 0.0;
            let [r, g, b] = ssc::power::Power::Weave.tint();
            let color = Color::srgb(r, g, b).with_alpha(if warning {
                0.55 + 0.2 * (game.time * 12.0).sin().abs()
            } else {
                let fray = (tether.health / tether.max_health).clamp(0.0, 1.0);
                (0.5 + 0.5 * fray) * (tether.remaining / 3.0).clamp(0.2, 1.0)
            });
            let along = to - from;
            if warning {
                for i in 0..16 {
                    let u = i as f32 / 16.0;
                    gizmos.line_2d(from + along * u, from + along * (u + 0.035), color);
                }
            } else {
                gizmos.line_2d(from, to, color);
            }
            gizmos.circle_2d(to, 7.0, color).resolution(12);
            gizmos.circle_2d(from, 4.0, color).resolution(8);
            continue;
        }
        let latched = tether.kind == TetherKind::Latch && tether.attached();
        // How near the ship is to snapping it, or how hard it pulls, whichever is more.
        let strain = if latched {
            tether.strain.max(tether.tension)
        } else {
            0.0
        };
        // Strong cords read as heavier: more strands, hotter and brighter, trembling under load.
        let power = if tether.kind == TetherKind::Latch {
            ((tether.cord.strength - 1.0) / 7.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Strong cords shift from violet to hot magenta; the glow styles bloom the extra heat.
        let heat = 1.0 + 0.7 * power;
        let color = Color::srgb(
            (0.75 + 0.25 * strain + 0.25 * power).min(1.0) * heat,
            (0.4 - 0.2 * strain - 0.2 * power).max(0.05) * heat,
            (1.0 - 0.7 * strain - 0.45 * power).max(0.1) * heat,
        );
        let color = if latched && tether.health < tether.max_health * 0.5 {
            // A frayed cord flickers.
            let flicker = 0.55 + 0.45 * (game.time * 40.0).sin().abs();
            color.with_alpha(flicker)
        } else {
            color
        };
        // A rippling cord, taut and straight as it nears breaking.
        let along = to - from;
        let side = Vec2::new(-along.y, along.x).normalize_or_zero();
        let ripple = 7.0 * (1.0 - strain);
        let tremble = 3.5 * tether.tension;
        let strands = if tether.cord.strength >= 6.0 && tether.kind == TetherKind::Latch {
            3
        } else if tether.cord.strength >= game.tune.tether_strong_cord
            && tether.kind == TetherKind::Latch
        {
            2
        } else {
            1
        };
        for strand in 0..strands {
            let lane = (strand as f32 - (strands - 1) as f32 * 0.5) * 3.0;
            let points = (0..=16).map(|i| {
                let t = i as f32 / 16.0;
                let envelope = t * (1.0 - t) * 4.0;
                let wobble = (t * 9.0 - game.time * 14.0).sin() * ripple * envelope
                    + (t * 37.0 + game.time * 70.0 + strand as f32 * 2.0).sin()
                        * tremble
                        * envelope;
                from + along * t + side * (wobble + lane)
            });
            gizmos.linestrip_2d(points, color);
        }
        if tether.tip.is_some() {
            gizmos.circle_2d(to, 5.0 + 3.0 * power, color).resolution(8);
        } else if latched {
            // A clamp where it holds the ship, bigger the stronger the cord, and a tension
            // ring at the middle that swells as the cord loads up.
            gizmos
                .circle_2d(to, 7.0 + 6.0 * power, color)
                .resolution(10);
            if tether.tension > 0.05 {
                gizmos
                    .circle_2d(from + along * 0.5, 3.0 + 9.0 * tether.tension, color)
                    .resolution(12);
            }
        }
    }
}

/// Bullets, casters, mines, rune fields and timed effects.
fn draw_shots_and_effects(gizmos: &mut Gizmos, f: &Frame) {
    let Frame {
        game, camera, half, ..
    } = *f;
    for bullet in &game.bullets {
        // Fitted shots wear their modification: bursting, piercing or seeking.
        let color = if bullet.friendly {
            if bullet.blast > 0 {
                Color::srgb(1.0, 0.62, 0.2)
            } else if bullet.pierce > 0 {
                Color::srgb(0.95, 0.98, 1.0)
            } else if bullet.homing > 0 {
                Color::srgb(0.75, 1.0, 0.35)
            } else {
                CYAN
            }
        } else if bullet.pith > 0.0 {
            // A hullpick's bolt: violet, the colour of the spine that fired it.
            Color::srgb(0.85, 0.35, 1.0)
        } else {
            Color::srgb(1.0, 0.3, 0.37)
        };
        let direction = bullet.velocity.normalize_or_zero();
        let side = Vec2::new(-direction.y, direction.x);
        let p = bullet.position;
        match bullet.shape {
            Shape::Pellet => {
                gizmos.line_2d(p - direction * 11.0, p, color);
                gizmos.circle_2d(p, bullet.radius, color).resolution(6);
            }
            Shape::Needle => {
                gizmos.line_2d(p - direction * 20.0, p + direction * 3.0, color);
            }
            Shape::Missile => {
                gizmos.lineloop_2d(
                    [
                        p + direction * 10.0,
                        p - direction * 6.0 + side * 5.0,
                        p - direction * 3.0,
                        p - direction * 6.0 - side * 5.0,
                    ],
                    color,
                );
                gizmos.line_2d(
                    p - direction * 6.0,
                    p - direction * 22.0,
                    Color::srgb(1.0, 0.7, 0.2),
                );
            }
            Shape::Orb => {
                gizmos.circle_2d(p, bullet.radius, color).resolution(12);
                gizmos
                    .circle_2d(p, bullet.radius + 3.0, color.with_alpha(0.25))
                    .resolution(12);
            }
        }
    }
    for caster in game
        .bodies
        .iter()
        .filter(|b| b.active && !b.follower && ssc::power::Power::Rune.active(&b.genome))
    {
        let p = caster.position;
        let rgb = ssc::simulation::Payload::from_gene(caster.genome.rune).color();
        let color = Color::srgb(rgb[0], rgb[1], rgb[2]);
        let staff = p + Vec2::new(caster.radius + 12.0, 0.0);
        gizmos.line_2d(staff - Vec2::Y * 26.0, staff + Vec2::Y * 26.0, color);
        gizmos
            .circle_2d(staff + Vec2::Y * 26.0, 7.0, color)
            .resolution(5);
        gizmos
            .circle_2d(p, caster.radius + 7.0, color.with_alpha(0.55))
            .resolution(32);
        for k in 0..4 {
            let eye = p + Vec2::from_angle(k as f32 * std::f32::consts::FRAC_PI_2)
                * (caster.radius + 7.0);
            gizmos.circle_2d(eye, 2.5, color).resolution(6);
        }
    }
    for mine in game.mines.iter().filter(|m| {
        (m.position - camera)
            .abs()
            .cmplt(half + Vec2::splat(150.0))
            .all()
    }) {
        if let Some(sigil) = mine.sigil {
            draw_sigil(
                gizmos,
                mine.position,
                mine.blast,
                sigil.payload,
                mine.fuse,
                1.0,
            );
            continue;
        }
        let p = mine.position;
        let color = if mine.friendly {
            CYAN
        } else {
            Color::srgb(1.0, 0.55, 0.18)
        };
        gizmos.circle_2d(p, 8.0, color).resolution(8);
        for k in 0..6 {
            let d = Vec2::from_angle(k as f32 * std::f32::consts::TAU / 6.0 + mine.age * 0.3);
            gizmos.line_2d(p + d * 8.0, p + d * 14.0, color);
        }
        if let Some(fuse) = mine.fuse {
            let flash = 0.35 + 0.55 * (game.time * 24.0).sin().abs();
            gizmos
                .circle_2d(p, mine.blast, color.with_alpha(flash * 0.4))
                .resolution(40);
            gizmos
                .circle_2d(p, 16.0 + fuse.max(0.0) * 18.0, color.with_alpha(flash))
                .resolution(20);
        }
    }
    for field in &game.rune_fields {
        let alpha = (field.left / 0.45).min(1.0);
        draw_sigil(gizmos, field.position, 90.0, field.payload, None, alpha);
        let rgb = field.payload.color();
        let tint = Color::srgb(rgb[0], rgb[1], rgb[2]);
        let pulse = (1.0 - field.age / 0.45).max(0.0);
        let radius = 90.0 + 55.0 * (1.0 - pulse);
        gizmos
            .circle_2d(field.position, radius, tint.with_alpha(pulse * 0.8))
            .resolution(48);
        for k in 0..8 {
            let d = Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0);
            gizmos.line_2d(
                field.position + d * (radius + 7.0),
                field.position + d * (radius + 22.0),
                tint.with_alpha(pulse),
            );
        }
    }
    for effect in &game.effects {
        let fade = (effect.remaining / effect.lifetime).clamp(0.0, 1.0);
        let r = effect.radius * (1.0 + (1.0 - fade) * 1.5);
        let (ring, spark) = match effect.kind {
            // A birth is a soft lime bloom; coming of age is a bright, clean pulse.
            EffectKind::Birth => (
                Color::srgba(0.78, 0.95, 0.4, fade),
                Color::srgba(0.9, 1.0, 0.7, fade),
            ),
            EffectKind::Pair => (
                Color::srgba(0.85, 0.7, 1.0, fade * 0.45),
                Color::srgba(0.95, 0.85, 1.0, fade * 0.3),
            ),
            // The ship's shot found something: a crisp white tick, no orange spark.
            EffectKind::Hit => (
                Color::srgba(1.0, 1.0, 1.0, fade * 0.7),
                Color::srgba(1.0, 1.0, 1.0, fade),
            ),
            EffectKind::Mature => (
                Color::srgba(0.7, 0.9, 1.0, fade),
                Color::srgba(1.0, 1.0, 1.0, fade),
            ),
            _ => (
                Color::srgba(1.0, 0.66, 0.3, fade),
                Color::srgba(1.0, 0.85, 0.5, fade),
            ),
        };
        gizmos.circle_2d(effect.position, r, ring).resolution(24);
        for i in 0..8 {
            let direction = Vec2::from_angle(i as f32 * std::f32::consts::TAU / 8.0);
            gizmos.line_2d(
                effect.position + direction * r,
                effect.position + direction * (r + 8.0 * fade),
                spark,
            );
        }
    }
}

/// Ship-centered effects, markers and guide arrows over the world.
fn draw_ship_effects(gizmos: &mut Gizmos, f: &Frame) {
    let Frame {
        game,
        session,
        camera,
        half,
        jam,
        ..
    } = *f;
    crate::powerview::draw_rifts(gizmos, game, camera, half);
    crate::powerview::draw_song_rings(gizmos, game);
    draw_parry(gizmos, game);
    draw_dash(gizmos, game);
    draw_boost(gizmos, game);
    draw_echoes(gizmos, game, camera, half);
    draw_beacons(gizmos, game, camera, half);
    draw_wrecks(gizmos, game, camera, half);
    draw_guides(
        gizmos,
        game,
        camera,
        half,
        session.arrows,
        jam,
        session.reduce_effects,
    );
}

/// The HUD rings and corners, screen glitch and the radar.
fn draw_screen_layer(gizmos: &mut Gizmos, f: &Frame) {
    let Frame {
        game,
        session,
        camera,
        half,
        jam,
        screen,
        window,
    } = *f;
    if !game.game_over {
        let hud = game.hud();
        if jam.hud > 0.0 {
            // The display is jammed: static where the rings and corners were.
            let cluster = screen.v(screen.cluster());
            crate::glitchview::static_box(
                gizmos,
                (cluster, Vec2::new(screen.px(150.0), screen.px(50.0))),
                (40, 7, 0.5),
                (game.time, session.reduce_effects),
            );
            crate::glitchview::static_box(
                gizmos,
                (
                    screen.at(window.x / 2.0, 40.0),
                    Vec2::new(screen.px(window.x * 0.45), screen.px(30.0)),
                ),
                (40, 11, 0.35),
                (game.time, session.reduce_effects),
            );
        } else {
            if let Some(ship) = game.player() {
                crate::hud::draw_ship_rings(gizmos, &hud, ship, screen, game.time);
            }
            crate::hud::draw_hud(gizmos, game, &hud, screen, game.time);
        }
        if !session.reduce_effects {
            crate::hud::draw_juice(gizmos, &session.juice, screen);
            crate::hud::draw_vignette(gizmos, &hud, screen, game.time);
        }
    }
    if !session.reduce_effects {
        crate::glitchview::screen(gizmos, jam, camera, half, game.time);
    }
    // The radar: always on if the setting says so, else while the details are open and there
    // is room beside them. Mid-right, clear of the corners and the bottom cluster.
    if session.radar || (session.details_open() && window.x >= 1180.0) {
        draw_radar(
            gizmos,
            game,
            screen.at(window.x - 24.0 - RADAR_RADIUS, window.y / 2.0),
            screen.scale,
            jam,
            session.reduce_effects,
        );
    }
}
