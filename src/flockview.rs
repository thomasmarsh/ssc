//! Drawing of flocks as marks. Rendering only: every number comes from `ssc::simulation::Flock`
//! and nothing here is read by the simulation. A near member is a small chevron pointing along
//! its velocity, beating with its phase; a middling flock draws dashes; a far one is dots.

use bevy::prelude::*;
use ssc::simulation::{FlockLod, Game, extent_in_view};

/// Draws every flock whose bounding circle is in view, and only the members in view.
pub fn draw(gizmos: &mut Gizmos, game: &Game, camera: Vec2, half: Vec2) {
    for flock in game.flocks() {
        if flock.is_empty() || !extent_in_view(flock.centroid, flock.radius, camera, half, 60.0) {
            continue;
        }
        let [r, g, b] = flock.genome.color();
        let mut color = Color::srgb(r, g, b);
        if flock.alarmed {
            let mix = |c: f32| c + (1.0 - c) * 0.6;
            color = Color::srgb(mix(r), mix(g), mix(b));
        }
        let size = flock.member_radius() * 0.85;
        let reach = half + Vec2::splat(40.0);
        for m in &flock.members {
            if !(m.position - camera).abs().cmplt(reach).all() {
                continue;
            }
            let p = m.position;
            match flock.lod {
                FlockLod::Near => {
                    let heading = if m.velocity.length_squared() > 1.0 {
                        m.velocity.normalize()
                    } else {
                        Vec2::X
                    };
                    let side = Vec2::new(-heading.y, heading.x);
                    // The wings open and close with the phase.
                    let beat = 0.55 + 0.25 * m.phase.sin();
                    let tip = p + heading * size;
                    let back = p - heading * size * 0.7;
                    gizmos.line_2d(tip, back + side * size * beat, color);
                    gizmos.line_2d(tip, back - side * size * beat, color);
                }
                FlockLod::Mid => {
                    let heading = m.velocity.normalize_or_zero();
                    gizmos.line_2d(p - heading * size * 0.6, p + heading * size, color);
                }
                FlockLod::Far => {
                    gizmos.line_2d(p, p + Vec2::splat(size * 0.5), color);
                }
            }
        }
    }
}
