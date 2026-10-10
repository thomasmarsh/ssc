//! Procedural vector icons: each icon is a few bars, discs and rings in a unit box, the same
//! look as the game's gizmo-drawn HUD and nothing to commit as an atlas. The shapes are pure
//! data (tested); `widgets::icon` turns them into rotated `bevy_ui` nodes.

use std::f32::consts::FRAC_PI_4;

/// One primitive of an icon.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    /// A filled rectangle (`w` by `h`, rotated by `angle`).
    Bar,
    /// A filled ellipse.
    Disc,
    /// An outline ellipse, the stroke being `thickness` of the box.
    Ring,
}

/// A primitive in unit-box coordinates (0,0 top left, 1,1 bottom right); `x`, `y` is its center.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Part {
    pub shape: Shape,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Clockwise radians.
    pub angle: f32,
}

const fn bar(x: f32, y: f32, w: f32, h: f32, angle: f32) -> Part {
    Part {
        shape: Shape::Bar,
        x,
        y,
        w,
        h,
        angle,
    }
}

const fn disc(x: f32, y: f32, d: f32) -> Part {
    Part {
        shape: Shape::Disc,
        x,
        y,
        w: d,
        h: d,
        angle: 0.0,
    }
}

const fn ring(x: f32, y: f32, d: f32) -> Part {
    Part {
        shape: Shape::Ring,
        x,
        y,
        w: d,
        h: d,
        angle: 0.0,
    }
}

/// The stroke width of a bar or ring, as a share of the icon box.
pub const STROKE: f32 = 0.14;

/// The icons the foundation ships (a screen that needs another adds it here).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    Check,
    Cross,
    /// A filled dot (a modified value).
    Dot,
    /// A circle with an arrowhead: regenerate or restart.
    Regen,
    /// A circle with a short tail: reset one value.
    Reset,
    Search,
    /// An arrow into a tray.
    Save,
    /// An arrow out of a tray.
    Load,
    /// An exclamation mark in a ring.
    Warn,
}

impl Icon {
    pub fn parts(self) -> Vec<Part> {
        const A: f32 = FRAC_PI_4;
        match self {
            Self::ChevronRight => vec![
                bar(0.42, 0.32, 0.52, STROKE, A),
                bar(0.42, 0.68, 0.52, STROKE, -A),
            ],
            Self::ChevronLeft => vec![
                bar(0.58, 0.32, 0.52, STROKE, -A),
                bar(0.58, 0.68, 0.52, STROKE, A),
            ],
            Self::ChevronDown => vec![
                bar(0.32, 0.42, 0.52, STROKE, A),
                bar(0.68, 0.42, 0.52, STROKE, -A),
            ],
            Self::ChevronUp => vec![
                bar(0.32, 0.58, 0.52, STROKE, -A),
                bar(0.68, 0.58, 0.52, STROKE, A),
            ],
            Self::Check => vec![
                bar(0.30, 0.60, 0.30, STROKE, A),
                bar(0.56, 0.50, 0.62, STROKE, -A),
            ],
            Self::Cross => vec![
                bar(0.5, 0.5, 0.78, STROKE, A),
                bar(0.5, 0.5, 0.78, STROKE, -A),
            ],
            Self::Dot => vec![disc(0.5, 0.5, 0.46)],
            Self::Regen => vec![
                ring(0.5, 0.5, 0.66),
                bar(0.5, 0.17, 0.30, STROKE, 0.0),
                bar(0.64, 0.07, 0.22, STROKE, A),
                bar(0.64, 0.27, 0.22, STROKE, -A),
            ],
            Self::Reset => vec![
                ring(0.5, 0.5, 0.66),
                bar(0.5, 0.5, 0.30, STROKE, 0.0),
                bar(0.36, 0.5, 0.18, STROKE, A),
            ],
            Self::Search => vec![ring(0.42, 0.42, 0.56), bar(0.76, 0.76, 0.34, STROKE, A)],
            Self::Save => vec![
                bar(0.5, 0.30, 0.46, STROKE, std::f32::consts::FRAC_PI_2),
                bar(0.5, 0.82, 0.70, STROKE, 0.0),
                bar(0.38, 0.52, 0.28, STROKE, A),
                bar(0.62, 0.52, 0.28, STROKE, -A),
            ],
            Self::Load => vec![
                bar(0.5, 0.52, 0.46, STROKE, std::f32::consts::FRAC_PI_2),
                bar(0.5, 0.86, 0.70, STROKE, 0.0),
                bar(0.38, 0.30, 0.28, STROKE, -A),
                bar(0.62, 0.30, 0.28, STROKE, A),
            ],
            Self::Warn => vec![
                ring(0.5, 0.5, 0.78),
                bar(0.5, 0.40, 0.30, STROKE, std::f32::consts::FRAC_PI_2),
                disc(0.5, 0.72, 0.14),
            ],
        }
    }

    pub const ALL: [Icon; 13] = [
        Self::ChevronLeft,
        Self::ChevronRight,
        Self::ChevronUp,
        Self::ChevronDown,
        Self::Check,
        Self::Cross,
        Self::Dot,
        Self::Regen,
        Self::Reset,
        Self::Search,
        Self::Save,
        Self::Load,
        Self::Warn,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_parts_inside_the_box() {
        for icon in Icon::ALL {
            let parts = icon.parts();
            assert!(!parts.is_empty() && parts.len() <= 4, "{icon:?}");
            for p in &parts {
                assert!(p.w > 0.0 && p.h > 0.0, "{icon:?} has an empty part");
                assert!(
                    (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y),
                    "{icon:?} centers a part outside the box"
                );
                // A rotated bar's reach stays within the box (half the long side).
                let reach = p.w.max(p.h) / 2.0;
                assert!(
                    p.x - reach * p.angle.cos().abs() >= -0.12
                        && p.x + reach * p.angle.cos().abs() <= 1.12,
                    "{icon:?} spills out sideways"
                );
            }
        }
    }

    #[test]
    fn mirrored_chevrons_mirror() {
        let left = Icon::ChevronLeft.parts();
        let right = Icon::ChevronRight.parts();
        for (l, r) in left.iter().zip(right) {
            assert!((l.x - (1.0 - r.x)).abs() < 1e-6);
        }
    }
}
