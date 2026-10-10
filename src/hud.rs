//! The geometric HUD: rings, arcs, pips and bars drawn with gizmos in screen space, and the few
//! small texts that carry totals. Everything shown comes from `ssc::simulation::hud` (the
//! headless view-model); nothing here owns a rule.
//!
//! Gizmo coordinates are world units, so every shape is laid out in logical pixels from the
//! window's top left (`Screen`) and converted once. That keeps the HUD the same size at every
//! camera zoom and lets the texts, which are UI nodes in the same pixels, line up with it.

use crate::Session;
use crate::presentation::{
    AMBER, APEX_GOLD, CYAN, DRY_RED, MUTED, PAD_AMBER, PAD_GREEN, material_color,
};
use crate::ui::controls::{self, ActiveDevice};
use crate::ui::glyphs::Device;
use bevy::prelude::*;
use ssc::simulation::arsenal::Profile;
use ssc::simulation::hud::{
    Ability, AbilityRing, CargoPip, HULL_SEGMENTS, Health, HudModel, RingState, THREAT_PIPS,
    segment_fill,
};
use ssc::simulation::{Game, Tier};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Reference pixel metrics of the bottom cluster.
const RING_R: f32 = 21.0;
const RING_GAP: f32 = 58.0;
const WEAPON_R: f32 = 25.0;
const WEAPON_X: f32 = -152.0;
const CARGO_X: f32 = 112.0;
const CARGO_W: f32 = 40.0;
const CARGO_H: f32 = 4.0;
const CARGO_STEP: f32 = 48.0;
/// Distance from the window bottom to the cluster's center line.
const CLUSTER_UP: f32 = 88.0;

const DIM: Color = Color::srgba(0.36, 0.49, 0.62, 0.3);
const WHITE: Color = Color::srgb(0.9, 0.95, 1.0);
const GOOD: Color = PAD_GREEN;
const JAM_GREY: Color = Color::srgba(0.6, 0.65, 0.72, 0.75);

/// The window in logical pixels, and where the camera looks, for placing HUD shapes.
#[derive(Clone, Copy)]
pub struct Screen {
    /// World position under the middle of the window.
    pub center: Vec2,
    /// World units per logical pixel.
    pub scale: f32,
    /// Window size in logical pixels.
    pub size: Vec2,
}

impl Screen {
    /// The layout for a camera at `center` seeing `half` world units each way, in a window of
    /// `viewport` logical pixels drawn at `ui_scale`.
    pub fn new(center: Vec2, half: Vec2, viewport: Vec2, ui_scale: f32) -> Self {
        let ui_scale = ui_scale.max(0.1);
        Self {
            center,
            scale: half.y * 2.0 / viewport.y.max(1.0) * ui_scale,
            size: viewport / ui_scale,
        }
    }

    /// The world position of the pixel (x from the left, y from the top).
    pub fn at(&self, x: f32, y: f32) -> Vec2 {
        self.center + Vec2::new(x - self.size.x / 2.0, self.size.y / 2.0 - y) * self.scale
    }

    pub fn v(&self, p: Vec2) -> Vec2 {
        self.at(p.x, p.y)
    }

    /// The pixel a world position falls on.
    pub fn px_of(&self, world: Vec2) -> Vec2 {
        let d = (world - self.center) / self.scale;
        Vec2::new(self.size.x / 2.0 + d.x, self.size.y / 2.0 - d.y)
    }

    /// A length in pixels as world units.
    pub fn px(&self, length: f32) -> f32 {
        length * self.scale
    }

    /// The bottom cluster's center line: x is the window's middle.
    pub fn cluster(&self) -> Vec2 {
        Vec2::new(self.size.x / 2.0, self.size.y - CLUSTER_UP)
    }
}

/// Points along an arc of the pixel circle at `c` (angles in screen sense: zero to the right,
/// positive counter-clockwise, so `FRAC_PI_2` is the top).
fn arc_points(s: &Screen, c: Vec2, r: f32, a0: f32, a1: f32) -> Vec<Vec2> {
    let steps = (((a1 - a0).abs() * r / 5.0).ceil() as usize).clamp(2, 64);
    (0..=steps)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / steps as f32;
            s.at(c.x + a.cos() * r, c.y - a.sin() * r)
        })
        .collect()
}

fn arc(g: &mut Gizmos, s: &Screen, c: Vec2, r: f32, a0: f32, a1: f32, color: Color) {
    if (a1 - a0).abs() > 1e-3 {
        g.linestrip_2d(arc_points(s, c, r, a0, a1), color);
    }
}

fn circle(g: &mut Gizmos, s: &Screen, c: Vec2, r: f32, color: Color) {
    arc(g, s, c, r, 0.0, TAU, color);
}

/// A clockwise sweep from the top covering `fraction` of the circle.
fn gauge(g: &mut Gizmos, s: &Screen, c: Vec2, r: f32, fraction: f32, color: Color) {
    let f = fraction.clamp(0.0, 1.0);
    if f >= 0.999 {
        circle(g, s, c, r, color);
    } else {
        arc(g, s, c, r, FRAC_PI_2, FRAC_PI_2 - f * TAU, color);
    }
}

fn line(g: &mut Gizmos, s: &Screen, a: Vec2, b: Vec2, color: Color) {
    g.line_2d(s.v(a), s.v(b), color);
}

/// A solid disc: concentric rings a pixel and a half apart (lines are two pixels wide).
fn disc(g: &mut Gizmos, s: &Screen, c: Vec2, r: f32, color: Color) {
    let mut ring = r;
    while ring > 0.6 {
        circle(g, s, c, ring, color);
        ring -= 1.5;
    }
    circle(g, s, c, 0.6, color);
}

/// A solid axis-aligned box (x, y from the top left), filled with horizontal lines.
fn solid_box(g: &mut Gizmos, s: &Screen, x: f32, y: f32, w: f32, h: f32, color: Color) {
    let mut row = y + 1.0;
    while row < y + h {
        line(g, s, Vec2::new(x, row), Vec2::new(x + w, row), color);
        row += 1.5;
    }
}

fn outline_box(g: &mut Gizmos, s: &Screen, x: f32, y: f32, w: f32, h: f32, color: Color) {
    g.linestrip_2d(
        [
            s.at(x, y),
            s.at(x + w, y),
            s.at(x + w, y + h),
            s.at(x, y + h),
            s.at(x, y),
        ],
        color,
    );
}

/// A gentle pulse in 0..=1.
fn pulse(time: f32, rate: f32) -> f32 {
    0.5 + 0.5 * (time * rate).sin()
}

fn health_color(health: Health, time: f32) -> Color {
    match health {
        Health::Good => GOOD,
        Health::Hurt => AMBER,
        Health::Low => DRY_RED.with_alpha(0.6 + 0.4 * pulse(time, 9.0)),
    }
}

/// The tier's color: red hostile, amber wary, white ignores, green friendly.
pub fn tier_color(tier: Tier) -> Color {
    match tier {
        Tier::Hostile => DRY_RED,
        Tier::Wary => PAD_AMBER,
        Tier::Ignores => WHITE,
        Tier::Friendly => GOOD,
    }
}

// ---- on the ship ------------------------------------------------------------------------

/// The hull ring (ten segments, green to amber to red) inside the shield ring (cyan, an arc
/// that shrinks) around the ship, in world space and sized so it clears the rig at any zoom.
/// Both dim when the ship is whole and quiet, and wake up when anything is hurt.
pub fn draw_ship_rings(
    g: &mut Gizmos,
    hud: &HudModel,
    ship: &ssc::simulation::Body,
    s: &Screen,
    time: f32,
) {
    let hull_r = (ship.radius * 2.6).max(s.px(24.0));
    let shield_r = hull_r + s.px(5.0);
    let whole = hud.hull_fraction >= 0.999 && hud.shield_fraction >= 0.999;
    let wake = if whole && hud.calm > 3.0 { 0.38 } else { 1.0 };
    let health = Health::of(hud.hull_fraction);
    let tint = health_color(health, time).with_alpha(wake);
    let p = ship.position;
    let seg = TAU / f32::from(HULL_SEGMENTS);
    let gap = 0.13;
    let on_ring = |a: f32, r: f32| p + Vec2::from_angle(a) * r;
    for i in 0..HULL_SEGMENTS {
        let a0 = FRAC_PI_2 - f32::from(i) * seg - gap * 0.5;
        let a1 = a0 - (seg - gap);
        let lit = segment_fill(hud.hull_fraction, i);
        // The dim track, then the lit part of it.
        g.linestrip_2d(
            (0..=4).map(|k| on_ring(a0 + (a1 - a0) * k as f32 / 4.0, hull_r)),
            DIM.with_alpha(0.22 * wake.max(0.6)),
        );
        if lit > 0.0 {
            let end = a0 + (a1 - a0) * lit;
            g.linestrip_2d(
                (0..=4).map(|k| on_ring(a0 + (end - a0) * k as f32 / 4.0, hull_r)),
                tint,
            );
        }
    }
    // Shield: a thin ring that is a full circle when charged and an arc as it drains.
    let shield_tint = if hud.shield_fraction < 0.3 && hud.shield_max > 0.0 {
        CYAN.with_alpha((0.35 + 0.65 * pulse(time, 10.0)) * wake)
    } else {
        CYAN.with_alpha(0.85 * wake)
    };
    if hud.shield_max > 0.0 {
        g.circle_2d(p, shield_r, DIM.with_alpha(0.18))
            .resolution(48);
        let f = hud.shield_fraction;
        if f >= 0.999 {
            g.circle_2d(p, shield_r, shield_tint).resolution(48);
        } else if f > 0.0 {
            let end = FRAC_PI_2 - f * TAU;
            let steps = ((f * 40.0).ceil() as usize).max(2);
            g.linestrip_2d(
                (0..=steps).map(|k| {
                    on_ring(
                        FRAC_PI_2 + (end - FRAC_PI_2) * k as f32 / steps as f32,
                        shield_r,
                    )
                }),
                shield_tint,
            );
        }
    }
    // Where the last hits came from: a red arc on the shield ring that fades.
    for &(angle, left) in &hud.hurts {
        for (grow, alpha) in [(0.0, 0.95), (s.px(2.5), 0.45)] {
            let r = shield_r + grow;
            g.linestrip_2d(
                (0..=8).map(|k| on_ring(angle - 0.45 + 0.9 * k as f32 / 8.0, r)),
                DRY_RED.with_alpha(alpha * left),
            );
        }
        // A tick pointing in from the arc's middle.
        g.line_2d(
            on_ring(angle, shield_r + s.px(6.0)),
            on_ring(angle, shield_r - s.px(4.0)),
            DRY_RED.with_alpha(left),
        );
    }
    // A mending ship wears a slow green halo.
    if hud.repairing {
        let r = shield_r + s.px(5.0 + 2.0 * pulse(time, 4.0));
        g.circle_2d(p, r, GOOD.with_alpha(0.35 + 0.3 * pulse(time, 4.0)))
            .resolution(40);
    }
}

// ---- the corners ------------------------------------------------------------------------

/// Five threat pips and a warning mark, top left.
pub fn draw_threat(g: &mut Gizmos, hud: &HudModel, s: &Screen, time: f32) {
    let origin = Vec2::new(28.0, 30.0);
    let lit = hud.threat;
    let color = match lit {
        0 => DIM,
        1 | 2 => GOOD,
        3 => PAD_AMBER,
        _ => DRY_RED,
    };
    // A warning triangle with a bang: the legend of the pips.
    let t = origin + Vec2::new(8.0, 0.0);
    g.linestrip_2d(
        [
            s.v(t + Vec2::new(0.0, -9.0)),
            s.v(t + Vec2::new(9.0, 7.0)),
            s.v(t + Vec2::new(-9.0, 7.0)),
            s.v(t + Vec2::new(0.0, -9.0)),
        ],
        if lit == 0 { DIM } else { color },
    );
    line(
        g,
        s,
        t + Vec2::new(0.0, -3.0),
        t + Vec2::new(0.0, 1.5),
        if lit == 0 { DIM } else { color },
    );
    for i in 0..THREAT_PIPS {
        let c = origin + Vec2::new(36.0 + f32::from(i) * 17.0, 0.0);
        if i < lit {
            let flash = if lit >= 4 {
                0.75 + 0.25 * pulse(time, 8.0)
            } else {
                1.0
            };
            disc(g, s, c, 5.5, color.with_alpha(flash));
        } else {
            circle(g, s, c, 5.5, DIM);
        }
    }
}

/// Lives as small ship glyphs, and the chain bar, top right (the score is text).
pub fn draw_score(g: &mut Gizmos, hud: &HudModel, s: &Screen) {
    let right = s.size.x - 28.0;
    for i in 0..hud.lives.min(6) {
        let c = Vec2::new(right - 8.0 - i as f32 * 20.0, 78.0);
        g.linestrip_2d(
            [
                s.v(c + Vec2::new(0.0, -8.0)),
                s.v(c + Vec2::new(7.0, 7.0)),
                s.v(c + Vec2::new(0.0, 3.0)),
                s.v(c + Vec2::new(-7.0, 7.0)),
                s.v(c + Vec2::new(0.0, -8.0)),
            ],
            CYAN,
        );
    }
    if let Some((mult, left)) = hud.streak {
        // The chain: a bar that drains as its window closes, hotter the bigger the multiplier.
        let width = 112.0;
        let (x, y) = (right - width, 56.0);
        let heat = ((mult - 1.0) / 2.0).clamp(0.0, 1.0);
        let color = Color::srgb(1.0, 0.85 - 0.45 * heat, 0.3 - 0.1 * heat);
        outline_box(g, s, x - 1.0, y - 1.0, width + 2.0, 8.0, DIM);
        solid_box(g, s, x, y, width * left.clamp(0.0, 1.0), 6.0, color);
    }
}

/// The standing meter near a civilization: a tier icon and a bar from hostile to friendly with
/// ticks where the tiers change.
pub fn draw_standing(g: &mut Gizmos, hud: &HudModel, s: &Screen, time: f32) {
    let Some(meter) = &hud.standing else {
        return;
    };
    let cx = s.size.x / 2.0;
    let y = 82.0;
    if meter.fallen {
        let c = Vec2::new(cx, y);
        line(
            g,
            s,
            c + Vec2::new(-5.0, -5.0),
            c + Vec2::new(5.0, 5.0),
            DIM,
        );
        line(
            g,
            s,
            c + Vec2::new(-5.0, 5.0),
            c + Vec2::new(5.0, -5.0),
            DIM,
        );
        return;
    }
    let color = tier_color(meter.tier);
    let (width, x0) = (132.0, cx - 66.0 + 8.0);
    // The tier icon.
    let ic = Vec2::new(x0 - 18.0, y);
    match meter.tier {
        Tier::Hostile => {
            let flash = 0.7 + 0.3 * pulse(time, 7.0);
            g.linestrip_2d(
                [
                    s.v(ic + Vec2::new(0.0, -8.0)),
                    s.v(ic + Vec2::new(8.0, 0.0)),
                    s.v(ic + Vec2::new(0.0, 8.0)),
                    s.v(ic + Vec2::new(-8.0, 0.0)),
                    s.v(ic + Vec2::new(0.0, -8.0)),
                ],
                color.with_alpha(flash),
            );
            line(
                g,
                s,
                ic + Vec2::new(-3.0, -3.0),
                ic + Vec2::new(3.0, 3.0),
                color,
            );
            line(
                g,
                s,
                ic + Vec2::new(-3.0, 3.0),
                ic + Vec2::new(3.0, -3.0),
                color,
            );
        }
        Tier::Wary => {
            g.linestrip_2d(
                [
                    s.v(ic + Vec2::new(0.0, -8.0)),
                    s.v(ic + Vec2::new(8.0, 6.0)),
                    s.v(ic + Vec2::new(-8.0, 6.0)),
                    s.v(ic + Vec2::new(0.0, -8.0)),
                ],
                color,
            );
        }
        Tier::Ignores => {
            circle(g, s, ic, 7.0, color);
            circle(g, s, ic, 2.0, color);
        }
        Tier::Friendly => {
            circle(g, s, ic, 7.0, color);
            line(
                g,
                s,
                ic + Vec2::new(-3.5, 0.0),
                ic + Vec2::new(-1.0, 3.0),
                color,
            );
            line(
                g,
                s,
                ic + Vec2::new(-1.0, 3.0),
                ic + Vec2::new(4.0, -3.5),
                color,
            );
        }
    }
    // The bar: filled to the regard, ticked at the tier lines (-40, -12, 40 on a -100..100 scale).
    outline_box(g, s, x0 - 1.0, y - 4.0, width + 2.0, 8.0, DIM);
    solid_box(
        g,
        s,
        x0,
        y - 3.0,
        width * meter.fraction,
        6.0,
        color.with_alpha(0.85),
    );
    for line_at in [-40.0_f32, -12.0, 40.0] {
        let f = (line_at + 100.0) / 200.0;
        line(
            g,
            s,
            Vec2::new(x0 + width * f, y - 7.0),
            Vec2::new(x0 + width * f, y + 7.0),
            DIM.with_alpha(0.7),
        );
    }
    let marker = x0 + width * meter.fraction;
    line(
        g,
        s,
        Vec2::new(marker, y - 7.0),
        Vec2::new(marker, y + 7.0),
        WHITE,
    );
}

/// The nearest-civilization line: name, distance in sectors and compass point.
fn nearest_text(game: &Game) -> Option<(String, ssc::simulation::NearestReport)> {
    const POINTS: [&str; 8] = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];
    let near = game.nearest_civilization()?;
    let octant = ((near.direction.y.atan2(near.direction.x) / std::f32::consts::FRAC_PI_4).round()
        as i32)
        .rem_euclid(8) as usize;
    let text = format!(
        "{}  {:.1} SECTORS {}",
        near.name, near.sectors, POINTS[octant]
    );
    Some((text, near))
}

/// A small arrow toward the nearest civilization (the echo's bearing), drawn beside its text.
pub fn draw_nearest(g: &mut Gizmos, game: &Game, s: &Screen, y: f32) {
    let Some((text, near)) = nearest_text(game) else {
        return;
    };
    // Inside a civilization's land the nearest one is the one you are in.
    if game.territory_report().is_some() {
        return;
    }
    let text_half = text.chars().count() as f32 * 3.4;
    let color =
        Color::srgb(near.tint[0], near.tint[1], near.tint[2]).with_alpha(0.4 + 0.6 * near.fade);
    let c = Vec2::new(s.size.x / 2.0 - text_half - 14.0, y);
    // Screen y is down, so a world-up bearing points up the screen.
    let d = Vec2::new(near.direction.x, -near.direction.y);
    let side = Vec2::new(-d.y, d.x);
    g.linestrip_2d(
        [
            s.v(c - d * 6.0 + side * 5.0),
            s.v(c + d * 7.0),
            s.v(c - d * 6.0 - side * 5.0),
        ],
        color,
    );
    line(g, s, c - d * 6.0, c + d * 7.0, color);
}

/// The apex elder's compact bar: a crown and a hull bar in gold (red once enraged).
pub fn draw_apex(g: &mut Gizmos, game: &Game, s: &Screen, y: f32) {
    let Some(apex) = game.apex_report() else {
        return;
    };
    let color = if apex.enraged {
        crate::presentation::APEX_ENRAGED
    } else {
        APEX_GOLD
    };
    let cx = s.size.x / 2.0;
    let (width, x0) = (132.0, cx - 66.0 + 8.0);
    let c = Vec2::new(x0 - 18.0, y);
    g.linestrip_2d(
        [
            s.v(c + Vec2::new(-8.0, 5.0)),
            s.v(c + Vec2::new(-8.0, -4.0)),
            s.v(c + Vec2::new(-4.0, 1.0)),
            s.v(c + Vec2::new(0.0, -6.0)),
            s.v(c + Vec2::new(4.0, 1.0)),
            s.v(c + Vec2::new(8.0, -4.0)),
            s.v(c + Vec2::new(8.0, 5.0)),
            s.v(c + Vec2::new(-8.0, 5.0)),
        ],
        color,
    );
    outline_box(g, s, x0 - 1.0, y - 4.0, width + 2.0, 8.0, DIM);
    solid_box(
        g,
        s,
        x0,
        y - 3.0,
        width * apex.health.clamp(0.0, 1.0),
        6.0,
        color.with_alpha(0.9),
    );
    // Resistance: a pip per damage family under the bar, in the family's tint, bigger the more
    // the elder has hardened against it (switch guns to relieve it); a bubble: a thin bar
    // under them that drains as close shots wear it.
    for (k, family) in ssc::simulation::arsenal::Family::ALL
        .into_iter()
        .enumerate()
    {
        let meter = apex.resist[k];
        if meter < game.tune.adapt_shown {
            continue;
        }
        let [r, gr, b] = family.tint();
        disc(
            g,
            s,
            Vec2::new(x0 + 8.0 + k as f32 * 18.0, y + 11.0),
            2.0 + 2.5 * meter,
            Color::srgb(r, gr, b).with_alpha(0.4 + 0.6 * meter),
        );
    }
    if let Some(integrity) = apex.bubble {
        let bubble = Color::srgb(0.75, 0.9, 1.0);
        outline_box(g, s, x0 + width - 41.0, y + 7.0, 42.0, 5.0, DIM);
        solid_box(
            g,
            s,
            x0 + width - 40.0,
            y + 8.0,
            40.0 * integrity.clamp(0.0, 1.0),
            3.0,
            bubble.with_alpha(0.8),
        );
    }
}

// ---- the next lure -----------------------------------------------------------------------

const LURE_GOLD: Color = Color::srgb(1.0, 0.84, 0.28);

/// Where the lure marker is: its pixel, whether that is inside the window (the diamond sits on
/// the target) or on the edge (an arrow points the way), and the direction from the middle of
/// the window.
fn lure_anchor(s: &Screen, target: Vec2) -> (Vec2, bool, Vec2) {
    let at = s.px_of(target);
    let middle = s.size / 2.0;
    let inset = 34.0;
    let inside = at.x > inset && at.x < s.size.x - inset && at.y > inset && at.y < s.size.y - inset;
    let d = (at - middle).normalize_or_zero();
    if inside {
        return (at, true, d);
    }
    // Along the ray from the middle to the target, stopped at the inset rectangle (and kept off
    // the bottom cluster's row).
    let reach = Vec2::new(middle.x - inset, middle.y - inset - 36.0).max(Vec2::splat(1.0));
    let t = (reach.x / d.x.abs().max(1e-4)).min(reach.y / d.y.abs().max(1e-4));
    (middle + d * t, false, d)
}

/// The next-lure marker: a gold diamond on the target when it is in view, else a gold diamond
/// arrow on the screen edge. There is only ever one.
pub fn draw_lure(g: &mut Gizmos, hud: &HudModel, s: &Screen, time: f32) {
    let Some(lure) = hud.lure else {
        return;
    };
    let (at, inside, d) = lure_anchor(s, lure.position);
    let beat = 0.75 + 0.25 * pulse(time, 5.0);
    let color = LURE_GOLD.with_alpha(beat);
    let size = 9.0;
    let discovery_kind = match lure.kind {
        ssc::simulation::lure::LureKind::Relic => Some(ssc::simulation::EchoKind::Relic),
        ssc::simulation::lure::LureKind::Well => Some(ssc::simulation::EchoKind::Well),
        // The mandatory Rift marker already draws its brackets.
        ssc::simulation::lure::LureKind::Rift => return,
        _ => None,
    };
    if let Some(kind) = discovery_kind {
        crate::presentation::draw_discovery_glyph(g, s.v(at), s.px(size), kind, color);
        return;
    }
    let diamond = |c: Vec2, r: f32| {
        [
            s.v(c + Vec2::new(0.0, -r)),
            s.v(c + Vec2::new(r, 0.0)),
            s.v(c + Vec2::new(0.0, r)),
            s.v(c + Vec2::new(-r, 0.0)),
            s.v(c + Vec2::new(0.0, -r)),
        ]
    };
    if inside {
        g.linestrip_2d(diamond(at, size + 5.0), color);
        g.linestrip_2d(diamond(at, size + 1.0), color.with_alpha(0.5));
        circle(g, s, at, 2.0, color);
    } else {
        // A diamond with a chevron leading out toward the target.
        let side = Vec2::new(-d.y, d.x);
        g.linestrip_2d(diamond(at - d * 4.0, size), color);
        g.linestrip_2d(
            [
                s.v(at + d * 10.0 + side * 6.0),
                s.v(at + d * 18.0),
                s.v(at + d * 10.0 - side * 6.0),
            ],
            color,
        );
    }
}

/// Floating kills and bench rings: short expanding rings in world space.
pub fn draw_juice(g: &mut Gizmos, juice: &crate::juice::Juice, s: &Screen) {
    use crate::juice::RingKind;
    for ring in &juice.rings {
        let t = ring.progress();
        let ease = 1.0 - (1.0 - t) * (1.0 - t);
        let fade = 1.0 - t;
        match ring.kind {
            RingKind::Kill | RingKind::BigKill => {
                let big = ring.kind == RingKind::BigKill;
                let reach = if big { 90.0 } else { 46.0 };
                let color = if big { LURE_GOLD } else { WHITE };
                g.circle_2d(ring.at, s.px(8.0 + reach * ease), color.with_alpha(fade))
                    .resolution(32);
                if big {
                    g.circle_2d(
                        ring.at,
                        s.px(4.0 + reach * 0.6 * ease),
                        color.with_alpha(fade * 0.5),
                    )
                    .resolution(24);
                }
            }
            RingKind::Purchase(rarity) => {
                let [r, gr, b] =
                    ssc::simulation::upgrades::Rarity::ALL[usize::from(rarity).min(3)].color();
                let color = Color::srgb(r, gr, b);
                for (delay, grow) in [(0.0, 1.0), (0.18, 0.7)] {
                    let local = ((t - delay) / (1.0 - delay)).clamp(0.0, 1.0);
                    if local <= 0.0 {
                        continue;
                    }
                    let e = 1.0 - (1.0 - local) * (1.0 - local);
                    g.circle_2d(
                        ring.at,
                        s.px(22.0 + 80.0 * grow * e),
                        color.with_alpha((1.0 - local) * 0.9),
                    )
                    .resolution(40);
                }
            }
        }
    }
}

/// A red wash along the window's edge while the hull is low: nested outlines that pulse, at
/// most a thin frame (well under a fifth of the screen).
pub fn draw_vignette(g: &mut Gizmos, hud: &HudModel, s: &Screen, time: f32) {
    if hud.hull_fraction >= ssc::simulation::hud::LOW || hud.hull <= 0.0 {
        return;
    }
    let need = 1.0 - hud.hull_fraction / ssc::simulation::hud::LOW;
    let beat = 0.55 + 0.45 * pulse(time, 3.2);
    for k in 0..4 {
        let inset = 2.0 + 5.0 * k as f32;
        let alpha = 0.4 * need * beat * (1.0 - k as f32 / 4.0);
        outline_box(
            g,
            s,
            inset,
            inset,
            s.size.x - 2.0 * inset,
            s.size.y - 2.0 * inset,
            DRY_RED.with_alpha(alpha),
        );
    }
}

// ---- the interact prompt -----------------------------------------------------------------

/// Where the prompt over the ship sits: the keycap's center, the label's center and its
/// half-width, all in pixels. It hovers above the ship, clear of the rings, and never rides up
/// into the top row.
fn prompt_layout(s: &Screen, ship: Vec2, label: &str) -> (Vec2, Vec2, f32) {
    let at = s.px_of(ship);
    let half = label.chars().count() as f32 * 3.9;
    let center = Vec2::new(
        at.x.clamp(half + 40.0, (s.size.x - half - 40.0).max(half + 40.0)),
        (at.y - 66.0).clamp(150.0, (s.size.y - 190.0).max(150.0)),
    );
    let key = center + Vec2::new(-half - 4.0, 0.0);
    let label_c = center + Vec2::new(16.0, 0.0);
    (key, label_c, half)
}

/// The keycap of the interact prompt: a rounded box that glows while a press would work and
/// dims with a reason when it would not.
pub fn draw_prompt(g: &mut Gizmos, game: &Game, s: &Screen, time: f32) {
    let (Some(prompt), Some(ship)) = (game.interact_prompt(), game.player()) else {
        return;
    };
    // The bench panel says how to close it.
    if game.bench_open() {
        return;
    }
    let text = prompt_text(&prompt);
    let (key, _, _) = prompt_layout(s, ship.position, &text);
    let blocked = prompt.blocked.is_some();
    let color = if blocked { PAD_AMBER } else { CYAN };
    let (x, y) = (key.x - 12.0, key.y - 12.0);
    if !blocked {
        let glow = 0.2 + 0.25 * pulse(time, 4.0);
        outline_box(g, s, x - 3.0, y - 3.0, 30.0, 30.0, color.with_alpha(glow));
    }
    outline_box(g, s, x, y, 24.0, 24.0, color);
}

fn prompt_text(prompt: &ssc::simulation::interact::Prompt) -> String {
    match prompt.blocked {
        Some(reason) => format!("{}  {reason}", prompt.label),
        None => prompt.label.clone(),
    }
}

// ---- the bottom cluster -----------------------------------------------------------------

fn ability_icon(g: &mut Gizmos, s: &Screen, ability: Ability, c: Vec2, color: Color) {
    match ability {
        Ability::Parry => {
            // A shield: flat top, pointed foot, and a forward arc over it.
            g.linestrip_2d(
                [
                    s.v(c + Vec2::new(-6.5, -6.0)),
                    s.v(c + Vec2::new(6.5, -6.0)),
                    s.v(c + Vec2::new(6.5, 1.5)),
                    s.v(c + Vec2::new(0.0, 8.5)),
                    s.v(c + Vec2::new(-6.5, 1.5)),
                    s.v(c + Vec2::new(-6.5, -6.0)),
                ],
                color,
            );
            line(
                g,
                s,
                c + Vec2::new(0.0, -6.0),
                c + Vec2::new(0.0, 5.5),
                color.with_alpha(0.55),
            );
        }
        Ability::Dash => {
            for k in [-4.0_f32, 3.0] {
                g.linestrip_2d(
                    [
                        s.v(c + Vec2::new(-6.0, k + 4.0)),
                        s.v(c + Vec2::new(0.0, k - 2.0)),
                        s.v(c + Vec2::new(6.0, k + 4.0)),
                    ],
                    color,
                );
            }
        }
        Ability::Ping => {
            disc(g, s, c + Vec2::new(0.0, 6.0), 1.6, color);
            for r in [5.0_f32, 9.0] {
                arc(
                    g,
                    s,
                    c + Vec2::new(0.0, 6.0),
                    r,
                    FRAC_PI_2 - 0.9,
                    FRAC_PI_2 + 0.9,
                    color.with_alpha(0.8),
                );
            }
        }
    }
}

fn padlock(g: &mut Gizmos, s: &Screen, c: Vec2, color: Color) {
    outline_box(g, s, c.x - 5.0, c.y - 1.0, 10.0, 8.0, color);
    arc(g, s, Vec2::new(c.x, c.y - 1.0), 3.8, 0.0, PI, color);
}

/// Static inside a ring: short bright dashes that flicker.
fn jam_static(g: &mut Gizmos, s: &Screen, c: Vec2, r: f32, time: f32, salt: u32) {
    let step = crate::glitchview::tick(time, false);
    for k in 0..7 {
        let u = |n: i32| crate::glitchview::unit(salt, k + n, step);
        let a = u(0) * TAU;
        let d = r * 0.8 * u(40).sqrt();
        let p = c + Vec2::from_angle(a) * d;
        let w = r * 0.6 * u(80);
        line(
            g,
            s,
            p - Vec2::new(w, 0.0),
            p + Vec2::new(w, 0.0),
            WHITE.with_alpha(0.55),
        );
    }
}

/// One ability ring: a lock, a recovering arc, a ready glow, an active flare or a red
/// no-energy warning, with its icon inside.
pub fn draw_ability(g: &mut Gizmos, ring: &AbilityRing, c: Vec2, s: &Screen, time: f32) {
    match ring.state {
        RingState::Locked => {
            circle(g, s, c, RING_R, DIM);
            padlock(g, s, c + Vec2::new(0.0, -1.0), DIM.with_alpha(0.7));
        }
        RingState::Cooling => {
            circle(g, s, c, RING_R, DIM);
            gauge(g, s, c, RING_R, ring.fill, CYAN.with_alpha(0.75));
            gauge(g, s, c, RING_R - 1.5, ring.fill, CYAN.with_alpha(0.4));
            ability_icon(g, s, ring.ability, c, CYAN.with_alpha(0.35));
        }
        RingState::Ready => {
            let glow = 0.25 + 0.2 * pulse(time, 3.0);
            circle(g, s, c, RING_R + 3.5, CYAN.with_alpha(glow));
            circle(g, s, c, RING_R, CYAN);
            circle(g, s, c, RING_R - 1.5, CYAN.with_alpha(0.55));
            ability_icon(g, s, ring.ability, c, WHITE);
        }
        RingState::Active => {
            circle(g, s, c, RING_R + 4.0, AMBER.with_alpha(0.5));
            circle(g, s, c, RING_R, AMBER);
            circle(g, s, c, RING_R - 1.5, AMBER);
            ability_icon(g, s, ring.ability, c, AMBER);
        }
        RingState::Jammed => {
            // Greyed out and full of static, with the seconds left below.
            circle(g, s, c, RING_R, DIM.with_alpha(0.5));
            ability_icon(g, s, ring.ability, c, DIM.with_alpha(0.5));
            jam_static(g, s, c, RING_R, time, ring.ability as u32 + 1);
        }
        RingState::NoEnergy => {
            // Dashed red: it would fire, but there is no shield to pay for it.
            for k in 0..12 {
                let a = TAU * k as f32 / 12.0;
                arc(g, s, c, RING_R, a, a + 0.34, DRY_RED);
            }
            ability_icon(g, s, ring.ability, c, DRY_RED.with_alpha(0.8));
        }
    }
}

/// A small glyph for a weapon profile, centered in the weapon ring.
fn weapon_glyph(g: &mut Gizmos, s: &Screen, profile: Profile, c: Vec2, color: Color) {
    let up = |dx: f32, dy: f32| c + Vec2::new(dx, -dy);
    match profile {
        Profile::Stock => {
            line(g, s, up(0.0, -7.0), up(0.0, 8.0), color);
            circle(g, s, up(0.0, 8.0), 2.0, color);
        }
        Profile::Spread => {
            for dx in [-7.0_f32, 0.0, 7.0] {
                line(g, s, up(dx * 0.25, -6.0), up(dx, 8.0), color);
            }
        }
        Profile::Needles => {
            for dx in [-5.0_f32, 0.0, 5.0] {
                line(g, s, up(dx, -3.0), up(dx, 8.0), color);
            }
        }
        Profile::Missiles => {
            g.linestrip_2d(
                [
                    s.v(up(0.0, 9.0)),
                    s.v(up(4.0, 1.0)),
                    s.v(up(4.0, -6.0)),
                    s.v(up(-4.0, -6.0)),
                    s.v(up(-4.0, 1.0)),
                    s.v(up(0.0, 9.0)),
                ],
                color,
            );
        }
        Profile::Mines => {
            circle(g, s, c, 4.0, color);
            for k in 0..6 {
                let d = Vec2::from_angle(k as f32 * TAU / 6.0);
                line(g, s, c + d * 4.0, c + d * 8.0, color);
            }
        }
        Profile::Nova => {
            circle(g, s, c, 4.0, color);
            circle(g, s, c, 8.0, color.with_alpha(0.55));
        }
        Profile::Broadside => {
            line(g, s, up(-8.0, 0.0), up(8.0, 0.0), color);
            line(g, s, up(-8.0, 0.0), up(-8.0, 5.0), color);
            line(g, s, up(8.0, 0.0), up(8.0, 5.0), color);
        }
        Profile::Tail => {
            line(g, s, up(0.0, 7.0), up(0.0, -8.0), color);
            circle(g, s, up(0.0, -8.0), 2.0, color);
        }
        Profile::Homing => {
            g.linestrip_2d(
                [
                    s.v(up(-6.0, -6.0)),
                    s.v(up(-6.0, 2.0)),
                    s.v(up(0.0, 7.0)),
                    s.v(up(6.0, 2.0)),
                    s.v(up(6.0, 8.0)),
                ],
                color,
            );
        }
        Profile::Pierce => {
            line(g, s, up(0.0, -8.0), up(0.0, 8.0), color);
            circle(g, s, c, 4.5, color.with_alpha(0.6));
        }
        Profile::Blast => {
            for k in 0..8 {
                let d = Vec2::from_angle(k as f32 * TAU / 8.0);
                line(g, s, c + d * 3.0, c + d * 8.0, color);
            }
        }
    }
}

/// The weapon: an icon in a ring whose outer arc is the fuel, level pips under it and a tick
/// for every owned profile over it (the active one lit).
pub fn draw_weapon(g: &mut Gizmos, hud: &HudModel, c: Vec2, s: &Screen, time: f32) {
    let w = &hud.weapon;
    let fuel_color = w.material.map_or(CYAN, material_color);
    let base = if w.jammed {
        DIM.with_alpha(0.55)
    } else if w.dry {
        DRY_RED.with_alpha(0.55 + 0.45 * pulse(time, 8.0))
    } else {
        WHITE
    };
    circle(g, s, c, WEAPON_R - 4.0, DIM);
    weapon_glyph(g, s, w.profile, c, base);
    // The fuel arc.
    circle(g, s, c, WEAPON_R + 1.5, DIM);
    if w.material.is_none() {
        circle(g, s, c, WEAPON_R + 1.5, CYAN.with_alpha(0.55));
    } else if w.dry {
        circle(
            g,
            s,
            c,
            WEAPON_R + 1.5,
            DRY_RED.with_alpha(0.6 + 0.4 * pulse(time, 8.0)),
        );
    } else {
        gauge(g, s, c, WEAPON_R + 1.5, w.fuel, fuel_color);
        gauge(g, s, c, WEAPON_R + 3.0, w.fuel, fuel_color.with_alpha(0.35));
    }
    if w.jammed {
        jam_static(g, s, c, WEAPON_R, time, 9);
    }
    // Level pips below: lit up to the level, dim up to the profile's maximum.
    let max = w.profile.max_level().max(1);
    if max > 1 {
        let width = f32::from(max - 1) * 8.0;
        for k in 0..max {
            let p = Vec2::new(
                c.x - width / 2.0 + f32::from(k) * 8.0,
                c.y + WEAPON_R + 12.0,
            );
            if k < w.level {
                disc(g, s, p, 2.4, fuel_color);
            } else {
                circle(g, s, p, 2.4, DIM);
            }
        }
    }
    // One tick per owned profile above the ring.
    if w.owned > 1 {
        let width = f32::from(w.owned - 1) * 6.0;
        for k in 0..w.owned {
            let x = c.x - width / 2.0 + f32::from(k) * 6.0;
            let lit = k == w.index;
            line(
                g,
                s,
                Vec2::new(x, c.y - WEAPON_R - 9.0),
                Vec2::new(x, c.y - WEAPON_R - if lit { 17.0 } else { 13.0 }),
                if lit { CYAN } else { DIM.with_alpha(0.8) },
            );
        }
    }
}

/// Six fixed counters with capacity bars, arranged in two rows of three.
pub fn draw_cargo(g: &mut Gizmos, pips: &[CargoPip; 6], origin: Vec2, s: &Screen, time: f32) {
    for (i, pip) in pips.iter().enumerate() {
        let x = origin.x + (i % 3) as f32 * CARGO_STEP;
        let top = origin.y + (i / 3) as f32 * 29.0 - CARGO_H / 2.0;
        let color = material_color(pip.material);
        let edge = if pip.full {
            color.with_alpha(0.6 + 0.4 * pulse(time, 7.0))
        } else {
            DIM.with_alpha(0.8)
        };
        outline_box(g, s, x - 1.0, top - 1.0, CARGO_W + 2.0, CARGO_H + 2.0, edge);
        let h = CARGO_W * pip.fill.clamp(0.0, 1.0);
        if h > 0.5 {
            solid_box(g, s, x, top, h, CARGO_H, color.with_alpha(0.9));
        }
    }
}

/// Where each element of the bottom cluster sits, in pixels (also used to place its texts).
pub struct ClusterLayout {
    pub weapon: Vec2,
    pub abilities: [Vec2; 3],
    pub cargo: Vec2,
}

impl ClusterLayout {
    pub fn of(s: &Screen) -> Self {
        let c = s.cluster();
        Self {
            weapon: c + Vec2::new(WEAPON_X, 0.0),
            abilities: [-1.0, 0.0, 1.0].map(|k| c + Vec2::new(k * RING_GAP, 0.0)),
            cargo: c + Vec2::new(CARGO_X, 0.0),
        }
    }

    /// The center of cargo pip `i`.
    pub fn cargo_pip(&self, i: usize) -> Vec2 {
        Vec2::new(
            self.cargo.x + (i % 3) as f32 * CARGO_STEP + CARGO_W / 2.0,
            self.cargo.y + (i / 3) as f32 * 29.0,
        )
    }
}

/// Small gland icons right of the cargo pips: a hexagon in the organ's tint, solid when it
/// works, hollow and grey when asleep, with a ring that drains while a bond runs on loan, and a
/// dot a level.
pub fn draw_organs(g: &mut Gizmos, hud: &HudModel, layout: &ClusterLayout, s: &Screen, time: f32) {
    use ssc::simulation::hud::OrganState;
    for (k, icon) in hud.organs.iter().enumerate() {
        let c = layout.cargo + Vec2::new(3.0 * CARGO_STEP + 14.0, -14.0 + k as f32 * 20.0);
        let [r, gr, b] = icon.organ.tint();
        let tint = Color::srgb(r, gr, b);
        let hex = |radius: f32| -> Vec<Vec2> {
            (0..=6)
                .map(|i| {
                    let a = i as f32 * TAU / 6.0 + FRAC_PI_2;
                    s.at(c.x + a.cos() * radius, c.y - a.sin() * radius)
                })
                .collect()
        };
        match icon.state {
            OrganState::Active => {
                g.linestrip_2d(hex(8.0), tint);
                g.linestrip_2d(hex(4.5), tint.with_alpha(0.8));
                g.linestrip_2d(hex(2.0), tint);
            }
            OrganState::Asleep => {
                g.linestrip_2d(hex(8.0), JAM_GREY);
            }
            OrganState::Bond(left) => {
                g.linestrip_2d(hex(8.0), tint.with_alpha(0.55 + 0.3 * pulse(time, 3.0)));
                g.linestrip_2d(hex(3.5), tint.with_alpha(0.7));
                gauge(g, s, c, 11.5, left, tint.with_alpha(0.7));
            }
        }
        for n in 0..icon.level {
            disc(
                g,
                s,
                Vec2::new(c.x + 13.0 + f32::from(n) * 4.0, c.y),
                1.0,
                tint,
            );
        }
    }
}

/// The realm tag's stress icon, top left under the threat pips: a diamond in the realm's tint
/// around the glyph of the first axis it tests (a calm ring where it tests nothing), and a dot
/// for each further axis, hollow for the mild one.
pub fn draw_realm(g: &mut Gizmos, hud: &HudModel, s: &Screen) {
    use ssc::realm::Axis;
    let Some(tag) = &hud.realm else {
        return;
    };
    let c = Vec2::new(36.0, 66.0);
    let [r, gr, b] = tag.tint;
    let tint = Color::srgb(r, gr, b).with_alpha(tag.alpha.max(0.45));
    let p = |dx: f32, dy: f32| s.v(c + Vec2::new(dx, dy));
    g.linestrip_2d(
        [
            p(0.0, -10.0),
            p(10.0, 0.0),
            p(0.0, 10.0),
            p(-10.0, 0.0),
            p(0.0, -10.0),
        ],
        tint,
    );
    if tag.gentle {
        circle(g, s, c, 3.5, tint);
        return;
    }
    match tag.axes.first().copied().unwrap_or(Axis::Damage) {
        Axis::Damage => {
            g.line_2d(p(-4.0, -4.0), p(4.0, 4.0), tint);
            g.line_2d(p(-4.0, 4.0), p(4.0, -4.0), tint);
        }
        Axis::Range => {
            g.line_2d(p(-5.0, 0.0), p(5.0, 0.0), tint);
            g.line_2d(p(5.0, 0.0), p(2.0, -3.0), tint);
            g.line_2d(p(5.0, 0.0), p(2.0, 3.0), tint);
        }
        Axis::Defense => {
            g.linestrip_2d(
                [
                    p(-3.5, -4.0),
                    p(3.5, -4.0),
                    p(3.5, 1.0),
                    p(0.0, 5.0),
                    p(-3.5, 1.0),
                    p(-3.5, -4.0),
                ],
                tint,
            );
        }
        Axis::Mobility => {
            g.linestrip_2d([p(-4.0, -3.5), p(0.0, 0.0), p(-4.0, 3.5)], tint);
            g.linestrip_2d([p(0.0, -3.5), p(4.0, 0.0), p(0.0, 3.5)], tint);
        }
        Axis::Mining => {
            g.linestrip_2d(
                [p(-4.0, 3.5), p(0.0, -4.0), p(4.0, 3.5), p(-4.0, 3.5)],
                tint,
            );
        }
        Axis::Sensors => {
            circle(g, s, c, 4.0, tint);
            disc(g, s, c, 1.2, tint);
        }
        Axis::Symbiosis => {
            circle(g, s, c + Vec2::new(-2.0, 0.0), 2.8, tint);
            circle(g, s, c + Vec2::new(2.0, 0.0), 2.8, tint);
        }
        Axis::Utility => {
            outline_box(g, s, c.x - 3.5, c.y - 3.5, 7.0, 7.0, tint);
            disc(g, s, c, 1.0, tint);
        }
    }
    for (k, _) in tag.axes.iter().enumerate().skip(1) {
        let at = c + Vec2::new(-4.0 + (k - 1) as f32 * 8.0, 18.0);
        if k < tag.primary {
            disc(g, s, at, 2.0, tint);
        } else {
            circle(g, s, at, 2.0, tint);
        }
    }
}

/// The whole HUD apart from the ship's own rings.
pub fn draw_hud(g: &mut Gizmos, game: &Game, hud: &HudModel, s: &Screen, time: f32) {
    draw_threat(g, hud, s, time);
    draw_score(g, hud, s);
    draw_standing(g, hud, s, time);
    draw_nearest(g, game, s, 108.0);
    draw_apex(g, game, s, 130.0);
    draw_prompt(g, game, s, time);
    draw_lure(g, hud, s, time);
    let layout = ClusterLayout::of(s);
    draw_weapon(g, hud, layout.weapon, s, time);
    for (ring, c) in hud.abilities.iter().zip(layout.abilities) {
        draw_ability(g, ring, c, s, time);
    }
    draw_cargo(g, &hud.cargo, layout.cargo, s, time);
    draw_organs(g, hud, &layout, s, time);
    draw_realm(g, hud, s);
}

// ---- texts ------------------------------------------------------------------------------

/// The few small texts of the always-visible HUD. Each is an absolute UI node placed from the
/// same pixel layout as the shapes.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tag {
    Discovery(usize),
    Region,
    Realm,
    Sector,
    /// What the sector asks of this build: level, rating and verdict (slice K3).
    Area,
    /// The plain lines under it: what is missing, the volley warning, the way around.
    AreaDetail,
    Score,
    Mult,
    Standing,
    Nearest,
    Apex,
    Vitals,
    Hints,
    /// The next lure's kind and distance, by its marker.
    Lure,
    /// A floating score (by slot).
    Floater(usize),
    /// The active weapon's name, briefly after a switch.
    Weapon,
    /// The interact prompt's label, and the key in its cap.
    Prompt,
    PromptKey,
    Key(usize),
    Cargo(usize),
    /// Biomass and seeds, under the cargo pips, once the farm has anything.
    Farm,
    /// The developer tag, while any dev toggle is on.
    Dev,
}

impl Tag {
    const ALL: [Tag; 41] = [
        Tag::Discovery(0),
        Tag::Discovery(1),
        Tag::Discovery(2),
        Tag::Discovery(3),
        Tag::Discovery(4),
        Tag::Discovery(5),
        Tag::Discovery(6),
        Tag::Discovery(7),
        Tag::Region,
        Tag::Realm,
        Tag::Sector,
        Tag::Area,
        Tag::AreaDetail,
        Tag::Score,
        Tag::Mult,
        Tag::Standing,
        Tag::Nearest,
        Tag::Apex,
        Tag::Vitals,
        Tag::Hints,
        Tag::Lure,
        Tag::Floater(0),
        Tag::Floater(1),
        Tag::Floater(2),
        Tag::Floater(3),
        Tag::Floater(4),
        Tag::Floater(5),
        Tag::Weapon,
        Tag::Prompt,
        Tag::PromptKey,
        Tag::Key(0),
        Tag::Key(1),
        Tag::Key(2),
        Tag::Cargo(0),
        Tag::Cargo(1),
        Tag::Cargo(2),
        Tag::Cargo(3),
        Tag::Cargo(4),
        Tag::Cargo(5),
        Tag::Farm,
        Tag::Dev,
    ];

    fn size(self) -> f32 {
        match self {
            Tag::Discovery(_) => 11.0,
            Tag::Region => 19.0,
            Tag::Realm | Tag::AreaDetail => 11.0,
            Tag::Score => 26.0,
            Tag::Weapon => 17.0,
            Tag::Prompt => 14.0,
            Tag::Lure => 12.0,
            Tag::Floater(_) => 15.0,
            Tag::PromptKey => 15.0,
            Tag::Sector
            | Tag::Dev
            | Tag::Mult
            | Tag::Hints
            | Tag::Standing
            | Tag::Nearest
            | Tag::Area
            | Tag::Apex => 12.0,
            Tag::Vitals | Tag::Key(_) => 11.0,
            Tag::Cargo(_) | Tag::Farm => 10.0,
        }
    }
}

pub fn setup(mut commands: Commands) {
    for tag in Tag::ALL {
        commands.spawn((
            tag,
            Text::new(""),
            TextFont::from_font_size(tag.size()),
            TextColor(MUTED),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
        ));
    }
}

// Labels use a separate inset lane from mandatory attack warnings and the nearest bearing.
fn discovery_labels(game: &Game, s: &Screen) -> Vec<(String, Vec2, f32, [f32; 3])> {
    let mut out: Vec<(String, Vec2, f32, [f32; 3])> = Vec::new();
    let ship = game.player().map(|p| s.px_of(p.position));
    for (echo, fade) in game.echoes().filter(|(e, _)| e.discovery.is_some()).take(8) {
        let info = echo.discovery.unwrap();
        let mut text = info.label();
        if let ssc::simulation::DiscoveryInfo::Rift { partner, .. } = info {
            let sector = ssc::world::SectorId::containing(partner);
            text.push_str(&format!(" >{},{}", sector.x, sector.y));
        }
        let raw = s.px_of(echo.position);
        let half = text.chars().count() as f32 * 3.0 + 5.0;
        let x = raw
            .x
            .clamp(half + 70.0, (s.size.x - half - 70.0).max(half + 70.0));
        let start_y = (raw.y + (echo.radius / s.scale + 18.0).max(28.0))
            .clamp(145.0, (s.size.y - 160.0).max(145.0));
        for offset in [0.0, 22.0, -22.0, 44.0, -44.0, 66.0, -66.0] {
            let at = Vec2::new(x, start_y + offset);
            if at.y < 140.0 || at.y > s.size.y - 150.0 {
                continue;
            }
            if ship.is_some_and(|p| (p.x - x).abs() < half + 50.0 && (p.y - at.y).abs() < 65.0) {
                continue;
            }
            if out.iter().any(|(t, p, _, _)| {
                (p.y - at.y).abs() < 19.0 && (p.x - x).abs() < half + t.len() as f32 * 3.0
            }) {
                continue;
            }
            out.push((text, at, 0.35 + 0.55 * fade, info.tint()));
            break;
        }
    }
    out
}

/// The color an area readout of this mood is drawn in (the star map uses the same ones).
fn mood_color(mood: ssc::readout::Mood) -> Color {
    let [r, g, b] = crate::ui::screens::chart::mood_rgb(mood);
    Color::srgb(r, g, b)
}

/// Text, color, anchor (x, y of the text's middle) and alignment for a tag; None hides it.
fn describe(
    tag: Tag,
    session: &Session,
    hud: &HudModel,
    s: &Screen,
    device: Device,
) -> Option<(String, Color, Vec2, Align)> {
    let game = &session.game;
    let layout = ClusterLayout::of(s);
    let cx = s.size.x / 2.0;
    let right = s.size.x - 28.0;
    let dim = Color::srgba(0.55, 0.66, 0.78, 0.9);
    Some(match tag {
        Tag::Discovery(index) => {
            let labels = discovery_labels(game, s);
            let (text, at, fade, [r, g, b]) = labels.get(index)?;
            (
                text.clone(),
                Color::srgb(*r, *g, *b).with_alpha(*fade),
                *at,
                Align::Center,
            )
        }
        Tag::Region => (
            hud.region.clone(),
            Color::srgb(0.82, 0.9, 0.98).with_alpha(hud.region_alpha),
            Vec2::new(cx, 18.0),
            Align::Center,
        ),
        Tag::Realm => {
            let realm = hud.realm.as_ref()?;
            let [r, g, b] = realm.tint;
            (
                format!("{}\n{}", realm.name, realm.title),
                Color::srgb(r, g, b).with_alpha(0.55 + 0.4 * realm.alpha),
                Vec2::new(54.0, 61.0),
                Align::Left,
            )
        }
        Tag::Sector => (
            format!("SECTOR {}, {}", hud.sector.0, hud.sector.1),
            dim.with_alpha(0.75 * hud.region_alpha),
            Vec2::new(cx, 40.0),
            Align::Center,
        ),
        Tag::Area => {
            let area = &hud.area;
            if area.head.is_empty() {
                return None;
            }
            let calm = area.mood == ssc::readout::Mood::Calm;
            (
                area.head.clone(),
                mood_color(area.mood).with_alpha(if calm {
                    0.55
                } else {
                    0.7 + 0.3 * hud.region_alpha
                }),
                Vec2::new(cx, 56.0),
                Align::Center,
            )
        }
        Tag::AreaDetail => {
            let area = &hud.area;
            if area.detail.is_empty() || area.mood < ssc::readout::Mood::Warn {
                return None;
            }
            (
                area.detail
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n"),
                mood_color(area.mood).with_alpha(0.85),
                Vec2::new(cx, 84.0),
                Align::Center,
            )
        }
        Tag::Dev => {
            if !game.dev.active() && !game.culture_modified() && !game.tuning_modified() {
                return None;
            }
            (
                "DEV".to_string(),
                crate::presentation::AMBER,
                Vec2::new(right, 104.0),
                Align::Right,
            )
        }
        Tag::Score => (
            format!("{:06}", hud.score),
            Color::srgb(0.9, 0.96, 1.0),
            Vec2::new(right, 26.0),
            Align::Right,
        ),
        Tag::Mult => {
            let (mult, _) = hud.streak?;
            (
                format!("x{mult:.2}"),
                Color::srgb(1.0, 0.8, 0.35),
                Vec2::new(right - 120.0, 59.0),
                Align::Right,
            )
        }
        Tag::Standing => {
            let meter = hud.standing.as_ref()?;
            let tag = if meter.fallen {
                "FALLEN".to_string()
            } else {
                match meter.stance.note {
                    Some(note) => format!("{} - {}", meter.stance.label, note.to_uppercase()),
                    None => meter.stance.label.to_string(),
                }
            };
            // The region line above already names the place; this is how it feels about the ship.
            (
                tag,
                tier_color(meter.tier),
                Vec2::new(cx, 62.0),
                Align::Center,
            )
        }
        Tag::Nearest => {
            let (text, near) = nearest_text(game)?;
            if hud.standing.is_some() {
                return None;
            }
            (
                text,
                Color::srgb(near.tint[0], near.tint[1], near.tint[2])
                    .with_alpha(0.5 + 0.5 * near.fade),
                Vec2::new(cx + 10.0, 108.0),
                Align::Center,
            )
        }
        Tag::Apex => {
            let apex = game.apex_report()?;
            (
                format!(
                    "{}{}   {:.1}K",
                    apex.name,
                    if apex.alert { "   HUNTING" } else { "" },
                    apex.distance / 1000.0
                ),
                if apex.enraged {
                    crate::presentation::APEX_ENRAGED
                } else {
                    APEX_GOLD
                },
                Vec2::new(cx, 148.0),
                Align::Center,
            )
        }
        Tag::Vitals => (
            format!("HULL {:.0}   SHIELD {:.0}", hud.hull, hud.shield),
            dim,
            Vec2::new(cx, s.cluster().y + RING_R + 25.0),
            Align::Center,
        ),
        Tag::Hints => {
            let hints = game.context_hints();
            if hints.is_empty() {
                return None;
            }
            // As many as fit: the standing keys give way before the window does.
            let mut items: Vec<String> = hints
                .iter()
                .map(|h| format!("{} {}", controls::hint_key(h.key, device), h.action))
                .collect();
            items.extend(controls::standing_hints(device));
            let room = (s.size.x - 24.0) / 7.4;
            let mut text = String::new();
            for item in items {
                let next = if text.is_empty() {
                    item.clone()
                } else {
                    format!("{text}     {item}")
                };
                if next.chars().count() as f32 > room {
                    break;
                }
                text = next;
            }
            (
                text,
                Color::srgba(0.36, 0.49, 0.62, 0.9),
                Vec2::new(cx, s.size.y - 12.0),
                Align::Center,
            )
        }
        Tag::Weapon => {
            if session.details_open() || session.help {
                return None;
            }
            let (text, color) = crate::ui::screens::toast::arsenal_banner(game);
            if text.is_empty() {
                return None;
            }
            (
                text,
                color,
                layout.weapon + Vec2::new(0.0, -WEAPON_R - 34.0),
                Align::Center,
            )
        }
        Tag::Lure => {
            let lure = hud.lure?;
            if matches!(
                lure.kind,
                ssc::simulation::lure::LureKind::Relic
                    | ssc::simulation::lure::LureKind::Well
                    | ssc::simulation::lure::LureKind::Rift
            ) {
                return None;
            }
            let (at, inside, d) = lure_anchor(s, lure.position);
            let sectors = lure.position.distance(game.player()?.position) / ssc::world::SECTOR_SIZE;
            let text = format!("{}  {sectors:.1}", lure.kind.label());
            // Under the diamond when it sits on the target, inward of the arrow at the edge.
            let half = text.chars().count() as f32 * 3.6;
            let place = if inside {
                at + Vec2::new(0.0, 24.0)
            } else {
                // Inward of the arrow, clear of it: along the way back, shifted by the text's
                // own half-width where the arrow is on a side edge.
                at - d * 26.0 - Vec2::new(d.x * (half + 6.0) * d.x.abs().sqrt(), 0.0)
            };
            (text, LURE_GOLD.with_alpha(0.9), place, Align::Center)
        }
        Tag::Floater(i) => {
            let floater = session.juice.floaters.get(i)?;
            let rise = floater.age * 34.0;
            let fade = (1.0 - floater.age).clamp(0.0, 1.0);
            let at = s.px_of(floater.at) + Vec2::new(0.0, -14.0 - rise);
            (
                floater.text.clone(),
                if floater.big { LURE_GOLD } else { WHITE }.with_alpha(fade),
                at,
                Align::Center,
            )
        }
        Tag::Prompt | Tag::PromptKey => {
            let prompt = game.interact_prompt()?;
            let ship = game.player()?;
            if game.bench_open() {
                return None;
            }
            let text = prompt_text(&prompt);
            let (key, label, _) = prompt_layout(s, ship.position, &text);
            let color = if prompt.blocked.is_some() {
                PAD_AMBER
            } else {
                WHITE
            };
            if tag == Tag::Prompt {
                (text, color, label, Align::Center)
            } else {
                (controls::hint_key("E", device), color, key, Align::Center)
            }
        }
        Tag::Key(i) => {
            let ability = Ability::ALL[i];
            let ring = hud.abilities[i];
            let label = controls::ring_key(ability.key(), device);
            let color = match ring.state {
                RingState::Locked => DIM.with_alpha(0.7),
                RingState::Cooling => CYAN.with_alpha(0.6),
                RingState::Ready => WHITE,
                RingState::Active => AMBER,
                RingState::NoEnergy => DRY_RED,
                RingState::Jammed => JAM_GREY,
            };
            let c = layout.abilities[i];
            let text = if matches!(ring.state, RingState::Jammed | RingState::Cooling)
                && ring.left >= 0.1
            {
                format!("{:.1}", ring.left)
            } else if ring.fresh && ring.state == RingState::Ready {
                "NEW".to_string()
            } else {
                label
            };
            let color = if ring.fresh && ring.state == RingState::Ready {
                LURE_GOLD
            } else {
                color
            };
            (
                text,
                color,
                c + Vec2::new(0.0, RING_R + 10.0),
                Align::Center,
            )
        }
        Tag::Farm => {
            let farm = game.farm();
            if farm.seed_count() == 0 {
                return None;
            }
            let mut text = String::new();
            if let Some(kind) = farm.selected_seed() {
                let label = farm.seed_label(kind);
                text.push_str(&format!("   SEEDS {} {label}", farm.seed_count()));
            }
            (
                text,
                Color::srgb(0.55, 0.95, 0.6).with_alpha(0.9),
                layout.cargo + Vec2::new(0.0, 45.0),
                Align::Left,
            )
        }
        Tag::Cargo(i) => {
            if i >= 6 {
                return None;
            }
            let pip = hud.cargo[i];
            let c = layout.cargo_pip(i);
            (
                format!("{} {:.0}", pip.material.letter(), pip.amount),
                material_color(pip.material).with_alpha(0.9),
                c + Vec2::new(0.0, -9.0),
                Align::Center,
            )
        }
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Places and fills the HUD texts for this frame.
pub fn update_texts(
    session: Res<Session>,
    active: Res<ActiveDevice>,
    view: Single<(&Camera, &Transform, &Projection), With<Camera2d>>,
    ui_scale: Res<UiScale>,
    mut texts: Query<(&Tag, &mut Text, &mut TextColor, &mut Node, &mut TextLayout)>,
) {
    let Some(size) = view.0.logical_viewport_size() else {
        return;
    };
    let half = match view.2 {
        Projection::Orthographic(p) => p.area.half_size(),
        _ => Vec2::new(900.0, 450.0),
    };
    let s = Screen::new(view.1.translation.truncate(), half, size, ui_scale.0);
    let hud = session.game.hud();
    // A jammed display shows no text at all.
    let hidden = session.game.game_over || hud.jam.hud > 0.0;
    for (tag, mut text, mut color, mut node, mut layout) in &mut texts {
        let described = if hidden {
            None
        } else {
            describe(*tag, &session, &hud, &s, active.device)
        };
        let Some((string, tint, at, align)) = described else {
            if !text.0.is_empty() {
                text.0.clear();
            }
            continue;
        };
        if text.0 != string {
            text.0 = string;
        }
        color.0 = tint;
        let width = if *tag == Tag::Hints {
            (s.size.x - 16.0).max(200.0)
        } else {
            460.0
        };
        let left = match align {
            Align::Left => at.x,
            Align::Center => at.x - width / 2.0,
            Align::Right => at.x - width,
        };
        let justify = match align {
            Align::Left => Justify::Left,
            Align::Center => Justify::Center,
            Align::Right => Justify::Right,
        };
        if layout.justify != justify {
            layout.justify = justify;
        }
        node.left = px(left);
        node.top = px(at.y - tag.size() * 0.62);
        node.width = px(width);
    }
}

/// The UI scale for a window of this height: the HUD is drawn for about 760 pixels of height
/// and grows in quarter steps on bigger windows, never shrinking below one.
pub fn ui_scale_for(height: f32) -> f32 {
    ((height / 760.0).clamp(1.0, 1.75) * 4.0).round() / 4.0
}

/// Keeps the UI scale in step with the window.
pub fn apply_ui_scale(camera: Single<&Camera, With<Camera2d>>, mut scale: ResMut<UiScale>) {
    if let Some(size) = camera.logical_viewport_size() {
        let wanted = ui_scale_for(size.y);
        if (scale.0 - wanted).abs() > 1e-3 {
            scale.0 = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ui_scale_grows_in_steps_and_never_shrinks() {
        assert_eq!(ui_scale_for(500.0), 1.0);
        assert_eq!(ui_scale_for(720.0), 1.0);
        assert_eq!(ui_scale_for(1080.0), 1.5);
        assert_eq!(ui_scale_for(4000.0), 1.75);
    }

    #[test]
    fn pixels_map_to_world_units_from_the_top_left() {
        let s = Screen::new(
            Vec2::new(100.0, 50.0),
            Vec2::new(800.0, 450.0),
            Vec2::new(1600.0, 900.0),
            1.0,
        );
        assert_eq!(s.scale, 1.0);
        assert_eq!(s.at(800.0, 450.0), Vec2::new(100.0, 50.0));
        assert_eq!(s.at(0.0, 0.0), Vec2::new(-700.0, 500.0));
        let big = Screen::new(
            Vec2::ZERO,
            Vec2::new(800.0, 450.0),
            Vec2::new(1600.0, 900.0),
            2.0,
        );
        assert_eq!(big.size, Vec2::new(800.0, 450.0));
        assert_eq!(big.scale, 2.0);
    }
}
