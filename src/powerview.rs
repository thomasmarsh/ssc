//! Drawing of the rare powers: the tells that let the player recognise a carrier. Rendering
//! only; the rules are in `ssc::power` and `ssc::simulation`, and nothing here is read back.
//!
//! - Every live carrier wears a halo in its power's tint that pulses (a shimmer), plus a mark
//!   of its own: a blinker has twin chevrons at its nose, a phaser a dotted halo, a hullpick a
//!   long violet spine.
//! - A blink shows a ring that closes on the landing spot, a faint line to it, and for a
//!   moment a trail where it went.
//! - A phased body is drawn dim, with an afterimage; the last 0.4 s before it turns solid the
//!   outline brightens and a ring flares.
//! - A hullpick about to fire shows a charge ring closing on its muzzle.

use bevy::prelude::*;
use ssc::power::Power;
use ssc::simulation::{Body, Game};
use std::f32::consts::TAU;

fn tinted(power: Power) -> Color {
    let [r, g, b] = power.tint();
    Color::srgb(r, g, b)
}

/// The colour a creature's outline is drawn in: dim while phased, brightening through the
/// lead-in. Everything else is as given.
pub fn outline(game: &Game, body: &Body, color: Color) -> Color {
    if !body.phased {
        return color;
    }
    let lead = game.power_view(body).phase.map_or(0.0, |p| p.lead);
    color.with_alpha(0.22 + 0.5 * lead)
}

/// Whether to draw an afterimage behind the body (phased and not leading in).
pub fn afterimage(body: &Body) -> Option<Vec2> {
    body.phased
        .then(|| -body.velocity.normalize_or_zero() * body.radius * 1.2)
}

fn dotted_circle(
    gizmos: &mut Gizmos,
    center: Vec2,
    radius: f32,
    color: Color,
    dots: u32,
    spin: f32,
) {
    for k in 0..dots {
        let a = spin + k as f32 * TAU / dots as f32;
        gizmos.line_2d(
            center + Vec2::from_angle(a) * radius,
            center + Vec2::from_angle(a + TAU / dots as f32 * 0.4) * radius,
            color,
        );
    }
}

/// The dirge rings in flight: each a circle with its safe gap left open.
pub fn draw_song_rings(gizmos: &mut Gizmos, game: &Game) {
    let tint = tinted(Power::Song);
    for ring in game.song_rings() {
        let half = ring.gap_half();
        let steps = 64;
        let mut run: Vec<Vec2> = Vec::new();
        let flush = |gizmos: &mut Gizmos, run: &mut Vec<Vec2>| {
            if run.len() > 1 {
                gizmos.linestrip_2d(run.drain(..), tint.with_alpha(0.9));
            } else {
                run.clear();
            }
        };
        for k in 0..=steps {
            let a = k as f32 / steps as f32 * TAU;
            let apart =
                ((a - ring.gap_at) + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
            if apart.abs() <= half {
                flush(gizmos, &mut run);
            } else {
                run.push(ring.center + Vec2::from_angle(a) * ring.radius);
            }
        }
        flush(gizmos, &mut run);
        gizmos
            .circle_2d(ring.center, ring.radius + 6.0, tint.with_alpha(0.25))
            .resolution(64);
    }
}

/// A ring of small eyes that open in sequence as `open` rises to 1, then flash.
fn eyes(gizmos: &mut Gizmos, p: Vec2, r: f32, time: f32, open: f32, tint: Color) {
    let n = 10;
    for k in 0..n {
        let lit = (open * (n as f32 + 2.0) - k as f32).clamp(0.0, 1.0);
        let a = time * 0.5 + k as f32 * TAU / n as f32;
        gizmos
            .circle_2d(
                p + Vec2::from_angle(a) * (r * 1.7 + 6.0),
                1.5 + 2.0 * lit,
                tint.with_alpha(0.3 + 0.7 * lit),
            )
            .resolution(8);
    }
    if open >= 0.95 {
        gizmos
            .circle_2d(p, r * 1.7 + 14.0, Color::WHITE.with_alpha(0.8))
            .resolution(28);
    }
}

/// An Oozer: the soft skin as a closed curve through its nodes (the simulation owns the
/// shape), a darker nucleus that lags inside, what it has swallowed browning away, a second
/// inner membrane while it holds the ship, a long pseudopod when it reaches, and a body that
/// flattens (area kept) as it squeezes through a gap. Its fed reserve tints the skin.
pub fn draw_ooze(gizmos: &mut Gizmos, game: &Game, body: &Body, color: Color) {
    let Some(view) = game.power_view(body).ooze else {
        return;
    };
    let (p, r) = (body.position, body.radius);
    let tint = tinted(Power::Engulf);
    // Squeezed: the hit circle is `squeeze` of the free size, so the drawn blob keeps its area
    // by stretching along the passage (perpendicular to the gap line), within reason.
    let squeeze = view.squeeze.clamp(0.2, 1.0);
    let free = r / squeeze;
    let along_gap = Vec2::from_angle(view.squeeze_axis);
    let along_passage = Vec2::new(-along_gap.y, along_gap.x);
    let stretch = (1.0 / squeeze).min(1.7);
    let place = |local: Vec2| {
        p + along_gap * local.dot(along_gap) * squeeze
            + along_passage * local.dot(along_passage) * stretch
    };
    let skin = |scale: f32| {
        (0..=64).map(move |k| {
            let a = k as f32 / 64.0 * TAU;
            place(
                Vec2::from_angle(a) * (free * ssc::simulation::skin_radius(&view.skin, a) * scale),
            )
        })
    };
    // A well-fed one is deeper in colour.
    let rich = tint.mix(&Color::srgb(0.9, 0.95, 0.4), 0.5 * view.fed);
    let held = if view.held { 1.0 } else { 0.0 };
    gizmos.linestrip_2d(skin(1.0), color.mix(&rich, 0.5).with_alpha(0.9));
    gizmos.linestrip_2d(
        skin(0.9),
        rich.with_alpha(0.22 + 0.3 * held + 0.2 * view.fed),
    );
    if view.held {
        gizmos.linestrip_2d(skin(0.78), rich.with_alpha(0.5));
    }
    // Swallowed rocks, browning and shrinking as they digest.
    for &(angle, size, digested) in view.inside.iter().filter(|i| i.1 > 0.0) {
        let at = place(Vec2::from_angle(angle) * free * 0.45);
        let brown = Color::srgb(0.62 - 0.3 * digested, 0.5 - 0.1 * digested, 0.38);
        gizmos
            .circle_2d(
                at,
                free * size * (1.0 - 0.6 * digested),
                brown.with_alpha(0.7),
            )
            .resolution(10);
    }
    let nucleus = p + view.nucleus;
    gizmos
        .circle_2d(
            nucleus,
            r * 0.24,
            Color::srgb(0.15, 0.3, 0.1).with_alpha(0.95),
        )
        .resolution(18);
    gizmos
        .circle_2d(
            nucleus,
            r * 0.14,
            Color::srgb(0.2, 0.4, 0.12).with_alpha(0.9),
        )
        .resolution(14);
    // The pseudopod: a tapering finger that sways as it feels its way out, ending in a bright
    // tip. Its base sits on the skin where it points.
    if let Some((angle, len)) = view.reach.filter(|r| r.1 > 1.0) {
        let dir = Vec2::from_angle(angle);
        let side = Vec2::new(-dir.y, dir.x);
        let base = p + dir * (r * ssc::simulation::skin_radius(&view.skin, angle) * 0.92);
        let width = (r * 0.38).min(len * 0.5 + 4.0);
        let sway =
            |t: f32| side * (game.time * 3.0 + t * 4.0 + body.id as f32).sin() * len * 0.05 * t;
        let spine = |t: f32| base + dir * (len * t) + sway(t);
        let edge = |sign: f32| {
            (0..=10).map(move |k| {
                let t = k as f32 / 10.0;
                let w = width * (1.0 - t * t).max(0.0).sqrt() * (1.0 - 0.55 * t);
                spine(t) + side * w * sign
            })
        };
        gizmos.linestrip_2d(edge(1.0), color.mix(&rich, 0.5).with_alpha(0.9));
        gizmos.linestrip_2d(edge(-1.0), color.mix(&rich, 0.5).with_alpha(0.9));
        gizmos
            .circle_2d(spine(1.0), 3.0 + 0.05 * width, Color::WHITE.with_alpha(0.7))
            .resolution(8);
    }
}

/// The tells of one body's power: the halo, the mark, and any warning in progress.
pub fn draw(gizmos: &mut Gizmos, game: &Game, body: &Body) {
    let Some(carried) = body.genome.live_power() else {
        return;
    };
    if game.disguise(body).is_some() {
        return;
    }
    if body.follower {
        return;
    }
    let (p, r, time) = (body.position, body.radius, game.time);
    let tint = tinted(carried.power);
    let direction = Vec2::from_angle(body.angle);
    let side = Vec2::new(-direction.y, direction.x);
    let view = game.power_view(body);
    let shimmer = 0.5 + 0.5 * (time * 5.0 + body.id as f32 * 1.7).sin();
    if carried.power == Power::Engulf {
        // The blob's own soft outline is its tell (see `draw_ooze`), not a round halo.
        return;
    }
    // The halo: a thin ring that breathes, in the power's colour.
    gizmos
        .circle_2d(
            p,
            r * 1.45 + 4.0 + 2.0 * shimmer,
            tint.with_alpha(0.22 + 0.2 * shimmer),
        )
        .resolution(20);
    // A jam announced: the zone it will hit stays faintly drawn and a bright ring closes on the
    // source. Drawn whatever the strongest power is, so a stamped elder shows it too.
    if let Some(t) = view.jam {
        let jam_tint = tinted(if t.kind == ssc::simulation::JamKind::Emp {
            Power::Emp
        } else {
            Power::Confuse
        });
        let c = t.progress();
        gizmos
            .circle_2d(t.at, t.reach, jam_tint.with_alpha(0.12 + 0.12 * c))
            .resolution(48);
        gizmos
            .circle_2d(
                t.at,
                r * 1.4 + (t.reach - r * 1.4) * (1.0 - c),
                jam_tint.with_alpha(0.4 + 0.6 * c),
            )
            .resolution(48);
        for k in 0..8 {
            let a = time * 3.0 + k as f32 * TAU / 8.0;
            let d = Vec2::from_angle(a);
            let ring = r * 1.4 + (t.reach - r * 1.4) * (1.0 - c);
            gizmos.line_2d(
                t.at + d * (ring - 8.0),
                t.at + d * ring,
                jam_tint.with_alpha(0.8),
            );
        }
    }
    if view.glare > 0.0 {
        eyes(gizmos, p, r, time, view.glare, tint);
    }
    match carried.power {
        Power::Rift => {
            let forward = Vec2::from_angle(body.angle);
            gizmos.line_2d(p - forward * 30.0, p + forward * 40.0, tint);
            for k in 0..5 {
                let at = p - forward * (35.0 + k as f32 * 10.0);
                gizmos.line_2d(
                    at - forward.perp() * 4.0,
                    at + forward.perp() * 4.0,
                    tint.with_alpha(0.55),
                );
            }
        }
        Power::Sling => {
            dotted_circle(gizmos, p, r + 18.0, tint.with_alpha(0.45), 16, time * 0.9);
            for k in 0..4 {
                let angle = body.angle + k as f32 * TAU / 4.0 + 0.4;
                let radial = Vec2::from_angle(angle);
                let knee = p + radial * (r + 15.0);
                let tip = knee + Vec2::from_angle(angle + 0.8) * 18.0;
                gizmos.line_2d(p + radial * r, knee, tint);
                gizmos.line_2d(knee, tip, tint);
            }
            if let Some(tell) = view.sling
                && let Some(rock) = game.body(tell.rock)
            {
                let at = rock.position;
                let progress = (1.0 - tell.left / tell.total).clamp(0.0, 1.0);
                gizmos
                    .circle_2d(at, rock.radius + 7.0 + 12.0 * (1.0 - progress), tint)
                    .resolution(32);
                let end = at + tell.direction * 300.0;
                for i in 0..12 {
                    let u = i as f32 / 12.0;
                    gizmos.line_2d(
                        at.lerp(end, u),
                        at.lerp(end, u + 0.04),
                        tint.with_alpha(0.85),
                    );
                }
                let side = Vec2::new(-tell.direction.y, tell.direction.x);
                gizmos.line_2d(end, end - tell.direction * 18.0 + side * 10.0, tint);
                gizmos.line_2d(end, end - tell.direction * 18.0 - side * 10.0, tint);
            }
        }
        Power::Weave => {
            // Six spinnerets make the builder readable even before it has found a rock.
            let mut tips = Vec::with_capacity(7);
            for k in 0..6 {
                let angle = body.angle + k as f32 * TAU / 6.0;
                let radial = Vec2::from_angle(angle);
                let bend = Vec2::from_angle(angle + 0.3);
                let knee = p + radial * (r * 1.5 + 6.0);
                let tip = p + bend * (r * 2.0 + 12.0);
                gizmos.line_2d(p + radial * r * 0.7, knee, tint.with_alpha(0.8));
                gizmos.line_2d(knee, tip, tint.with_alpha(0.8));
                gizmos.circle_2d(tip, 2.0, tint).resolution(6);
                tips.push(knee);
            }
            tips.push(tips[0]);
            gizmos.linestrip_2d(tips, tint.with_alpha(0.2));
        }
        Power::Emp | Power::Confuse => {
            // Crackling fronds around the body.
            for k in 0..6 {
                let a = time * 2.0 + k as f32 * TAU / 6.0 + body.id as f32;
                let jag = 3.0 * (time * 23.0 + k as f32 * 1.9).sin();
                gizmos.line_2d(
                    p + Vec2::from_angle(a) * (r * 1.2),
                    p + Vec2::from_angle(a + 0.25) * (r * 1.9 + 6.0 + jag),
                    tint.with_alpha(0.8),
                );
            }
        }
        Power::Repel => {
            let reach = body.genome.power_reach;
            // Faint field rings drifting outward, spaced wider with distance.
            for k in 0..4 {
                let u = (time * 0.35 + k as f32 / 4.0).fract();
                let ring = r * 1.3 + (reach - r * 1.3) * u * u;
                gizmos
                    .circle_2d(p, ring, tint.with_alpha(0.28 * (1.0 - u)))
                    .resolution(40);
            }
            if view.inhale > 0.0 {
                // Breathing in: a pale ring closes from the rim onto the body.
                let ring = reach - (reach - r * 1.5) * view.inhale;
                gizmos
                    .circle_2d(p, ring, Color::WHITE.with_alpha(0.25 + 0.6 * view.inhale))
                    .resolution(48);
            }
            if view.shove_age < ssc::power::SHOVE_RING {
                let u = view.shove_age / ssc::power::SHOVE_RING;
                gizmos
                    .circle_2d(
                        p,
                        r + (reach - r) * u,
                        Color::WHITE.with_alpha(0.9 * (1.0 - u)),
                    )
                    .resolution(48);
                gizmos
                    .circle_2d(
                        p,
                        r + (reach - r) * u * 0.9,
                        tint.with_alpha(0.5 * (1.0 - u)),
                    )
                    .resolution(48);
            }
        }
        Power::Warp => {
            // A shimmering bubble: orange rim for haste, blue for slow, drifting dashes and a
            // slow swirl inside.
            let reach = body.genome.power_reach;
            let rim = if body.genome.warp < 0.0 {
                Color::srgb(0.35, 0.65, 1.0)
            } else {
                Color::srgb(1.0, 0.6, 0.2)
            };
            let spin = if body.genome.warp < 0.0 { 0.1 } else { 0.9 };
            gizmos
                .circle_2d(p, reach, rim.with_alpha(0.35 + 0.15 * shimmer))
                .resolution(64);
            dotted_circle(
                gizmos,
                p,
                reach - 6.0,
                rim.with_alpha(0.75),
                36,
                time * spin,
            );
            dotted_circle(
                gizmos,
                p,
                reach * 0.62,
                rim.with_alpha(0.25),
                24,
                -time * spin * 0.7,
            );
            for k in 0..3 {
                let a = time * spin * 0.6 + k as f32 * TAU / 3.0;
                gizmos.linestrip_2d(
                    (0..8).map(|j| {
                        let t = j as f32 / 7.0;
                        p + Vec2::from_angle(a + t * 1.4) * (reach * (0.25 + 0.3 * t))
                    }),
                    rim.with_alpha(0.22),
                );
            }
        }
        Power::Lens => {
            // A ring of stretched stars around the head, and the edge of the pull.
            let reach = body.genome.power_reach * ssc::power::LENS_REACH;
            dotted_circle(gizmos, p, reach, tint.with_alpha(0.2), 40, time * 0.1);
            for k in 0..10 {
                let a = time * 0.4 + k as f32 * TAU / 10.0;
                let d = Vec2::from_angle(a);
                gizmos.line_2d(
                    p + d * (r * 2.6 + 6.0),
                    p + d * (r * 2.6 + 6.0) + Vec2::new(-d.y, d.x) * (6.0 + 4.0 * shimmer),
                    tint.with_alpha(0.7),
                );
            }
            gizmos
                .circle_2d(p, r * 3.4 + 4.0 * shimmer, tint.with_alpha(0.3))
                .resolution(32);
        }
        Power::Devour => {
            // An open mouth ring with teeth; a green swirl inside once a well is held.
            gizmos
                .circle_2d(p, r * 0.8, tint.with_alpha(0.6))
                .resolution(24);
            for k in 0..12 {
                let a = k as f32 * TAU / 12.0 + time * 0.2;
                let d = Vec2::from_angle(a);
                gizmos.line_2d(p + d * (r * 0.8), p + d * (r * 0.8 - 5.0), tint);
            }
            if view.pocket > 0.0 {
                for k in 0..3 {
                    let a = time * 2.0 + k as f32 * TAU / 3.0;
                    gizmos.linestrip_2d(
                        (0..7).map(|j| {
                            let t = j as f32 / 6.0;
                            p + Vec2::from_angle(a + t * 2.0) * (r * 0.7 * t)
                        }),
                        Color::srgb(0.3, 1.0, 0.5).with_alpha(0.4 + 0.5 * view.pocket),
                    );
                }
                dotted_circle(
                    gizmos,
                    p,
                    ssc::power::POCKET_REACH * view.pocket,
                    Color::srgb(0.3, 1.0, 0.5).with_alpha(0.25),
                    30,
                    time * 0.2,
                );
            }
        }
        Power::Split => {
            // A seam down the middle that brightens as the body nears its end.
            let seam = game.seam(body);
            let across = Vec2::new(-direction.y, direction.x);
            gizmos.line_2d(
                p - across * r * 1.05,
                p + across * r * 1.05,
                Color::WHITE.with_alpha(0.25 + 0.7 * seam),
            );
            if seam > 0.6 {
                gizmos
                    .circle_2d(p, r * (1.15 + 0.1 * shimmer), tint.with_alpha(0.6))
                    .resolution(20);
            }
        }
        Power::Cloud => {
            // Motes in a slow swirl: ring, arrow or ball by the swarm's own beat; the edge
            // is a faint ring, the density shows in how many there are.
            let n = 14 + (ssc::power::cloud_density(&body.genome) * 40.0) as u32;
            let beat = (time * 0.25 + body.id as f32).sin();
            for k in 0..n {
                let h = ssc::world::hash2(body.id, k as i32, 7);
                let u = (h >> 40) as f32 / 16_777_216.0;
                let v = ((h >> 16) & 0xFFFF) as f32 / 65535.0;
                let ring = 0.45 + 0.55 * (u * 0.5 + 0.5 * beat.abs());
                let a = v * TAU + time * (0.4 + u) * if k % 2 == 0 { 1.0 } else { -0.7 };
                let at = p + Vec2::from_angle(a) * r * ring;
                gizmos.line_2d(
                    at,
                    at + Vec2::from_angle(a + 1.6) * (5.0 + 5.0 * u),
                    tint.with_alpha(0.55 + 0.4 * u),
                );
            }
            dotted_circle(gizmos, p, r, tint.with_alpha(0.18), 32, time * 0.1);
        }
        Power::Song => {
            if body.genome.song < 0.0 {
                // A chant: slow notes rising off the body, and the aura's edge.
                for k in 0..3 {
                    let u = (time * 0.5 + k as f32 / 3.0).fract();
                    gizmos
                        .circle_2d(p, r * 1.4 + 40.0 * u, tint.with_alpha(0.45 * (1.0 - u)))
                        .resolution(24);
                }
                dotted_circle(
                    gizmos,
                    p,
                    body.genome.power_reach,
                    tint.with_alpha(0.18),
                    40,
                    time * 0.1,
                );
            } else {
                // The mouth ring opens before each note.
                let open = view.song;
                gizmos
                    .circle_2d(p, r * (1.0 + 0.6 * open), tint.with_alpha(0.4 + 0.5 * open))
                    .resolution(28);
                if open > 0.0 {
                    gizmos
                        .circle_2d(
                            p,
                            r * 2.4 * (1.0 - open) + r,
                            Color::WHITE.with_alpha(0.6 * open),
                        )
                        .resolution(28);
                }
            }
        }
        Power::Glare => eyes(gizmos, p, r, time, view.glare.max(0.15), tint),
        Power::Dim => {
            // A bright thin rim, two bright eyes, and the edge of the dark.
            gizmos
                .circle_2d(p, r * 1.15, Color::WHITE.with_alpha(0.75))
                .resolution(20);
            for s in [-0.35_f32, 0.35] {
                gizmos
                    .circle_2d(p + direction * r * 0.35 + side * r * s, 1.8, Color::WHITE)
                    .resolution(6);
            }
            dotted_circle(
                gizmos,
                p,
                body.genome.power_reach * ssc::power::DIM_REACH,
                tint.with_alpha(0.28),
                48,
                time * 0.05,
            );
        }
        Power::Blink => {
            // Twin chevrons ahead of the nose: it goes places.
            for k in 0..2 {
                let at = p + direction * (r * 1.35 + 9.0 + 7.0 * k as f32);
                gizmos.linestrip_2d(
                    [
                        at - direction * 5.0 + side * 6.0,
                        at,
                        at - direction * 5.0 - side * 6.0,
                    ],
                    tint.with_alpha(0.9 - 0.3 * k as f32),
                );
            }
            if let Some(tell) = view.blink {
                // The ring that closes on the landing spot: shoot where it lands.
                let t = tell.progress();
                let ring = r * 1.6 + 70.0 * (1.0 - t);
                gizmos
                    .circle_2d(tell.to, ring, tint.with_alpha(0.35 + 0.55 * t))
                    .resolution(28);
                gizmos
                    .circle_2d(tell.to, r * 1.1, tint.with_alpha(0.25 + 0.5 * t))
                    .resolution(14);
                gizmos.line_2d(tell.from, tell.to, tint.with_alpha(0.12 + 0.15 * t));
            }
            if let Some((from, to, age)) = view.trail {
                let fade = 1.0 - age / 0.4;
                gizmos.line_2d(from, to, tint.with_alpha(0.35 * fade.max(0.0)));
                gizmos
                    .circle_2d(
                        to,
                        r * 2.0 + 30.0 * age / 0.4,
                        tint.with_alpha(0.5 * fade.max(0.0)),
                    )
                    .resolution(20);
            }
        }
        Power::Phase => {
            dotted_circle(
                gizmos,
                p,
                r * 1.75 + 6.0,
                tint.with_alpha(0.6),
                14,
                time * 0.8,
            );
            if let Some(phase) = view.phase {
                if phase.phased && phase.lead > 0.0 {
                    // About to turn solid: a flare that closes onto the body.
                    let ring = r * 1.2 + 40.0 * (1.0 - phase.lead);
                    gizmos
                        .circle_2d(p, ring, Color::WHITE.with_alpha(0.3 + 0.6 * phase.lead))
                        .resolution(24);
                } else if !phase.phased && phase.solid_left < 0.5 {
                    // The solid window is closing.
                    gizmos
                        .circle_2d(p, r * 1.5, tint.with_alpha(0.5 * phase.solid_left / 0.5))
                        .resolution(20);
                }
            }
        }
        Power::Bypass => {
            // A long violet dorsal spine, beyond the hull at both ends.
            gizmos.line_2d(
                p - direction * (r * 1.9 + 8.0),
                p + direction * (r * 2.2 + 10.0),
                tint,
            );
            gizmos.line_2d(
                p - direction * (r * 1.2),
                p - direction * (r * 1.2) + side * r * 0.9,
                tint.with_alpha(0.6),
            );
            gizmos.line_2d(
                p - direction * (r * 1.2),
                p - direction * (r * 1.2) - side * r * 0.9,
                tint.with_alpha(0.6),
            );
            if view.bypass_charge > 0.0 {
                let c = view.bypass_charge;
                let muzzle = p + direction * (r + 5.0);
                gizmos
                    .circle_2d(
                        muzzle,
                        4.0 + 26.0 * (1.0 - c),
                        tint.with_alpha(0.3 + 0.7 * c),
                    )
                    .resolution(16);
                gizmos
                    .circle_2d(muzzle, 3.0, tint.with_alpha(c))
                    .resolution(8);
            }
        }
        _ => {}
    }
}

/// Doorways use open centers and a faint broken connection, keeping shots readable.
/// Offscreen mouths have mandatory edge markers, independent of sensor settings or jams.
pub fn draw_rifts(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
    let cyan = Color::srgb(0.3, 0.95, 1.0);
    let gold = Color::srgb(1.0, 0.75, 0.35);
    for r in &game.rifts {
        let warning = r.warning > 0.0;
        let axis = (r.b - r.a).normalize_or_zero();
        let length = r.a.distance(r.b);
        let pieces = (length / 28.0) as u32;
        for k in 0..pieces {
            let d = 85.0 + k as f32 * (length - 170.0) / pieces as f32;
            gizmos.line_2d(
                r.a + axis * d,
                r.a + axis * (d + 7.0),
                cyan.with_alpha(0.16),
            );
        }
        for (at, color) in [(r.a, cyan), (r.b, gold)] {
            if warning {
                dotted_circle(gizmos, at, 70.0, color, 20, 0.0);
                let progress = (1.0 - r.warning / 1.2).clamp(0.0, 1.0);
                for k in 0..(progress * 24.0) as u32 {
                    let angle = k as f32 * TAU / 24.0;
                    gizmos.line_2d(
                        at + Vec2::from_angle(angle) * 83.0,
                        at + Vec2::from_angle(angle + TAU / 32.0) * 83.0,
                        color,
                    );
                }
            } else {
                gizmos.circle_2d(at, 70.0, color).resolution(64);
                dotted_circle(gizmos, at, 78.0, color.with_alpha(0.4), 24, 0.0);
            }
            // Inward brackets distinguish the portal from wells and sigils without a fill.
            for sign in [-1.0, 1.0] {
                let tip = at + Vec2::X * 60.0 * sign;
                gizmos.line_2d(tip, tip + Vec2::new(12.0 * sign, 10.0), color);
                gizmos.line_2d(tip, tip + Vec2::new(12.0 * sign, -10.0), color);
            }
            let delta = at - camera;
            if delta.x.abs() > half.x - 90.0 || delta.y.abs() > half.y - 90.0 {
                let bounds = (half - Vec2::splat(35.0)).max(Vec2::splat(1.0));
                let scale = (delta.x.abs() / bounds.x)
                    .max(delta.y.abs() / bounds.y)
                    .max(1.0);
                let edge = camera + delta / scale;
                dotted_circle(gizmos, edge, 12.0, color, 8, 0.0);
                let toward = delta.normalize_or_zero();
                gizmos.line_2d(edge, edge + toward * 18.0, color);
            }
        }
    }
    for t in &game.rift_traces {
        for at in [t.from, t.to] {
            gizmos
                .circle_2d(
                    at,
                    12.0 + 40.0 * (1.0 - t.left / 0.35),
                    cyan.with_alpha(t.left / 0.35),
                )
                .resolution(32);
        }
    }
}
