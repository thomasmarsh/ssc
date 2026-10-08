//! The shared mechanic of attached life: something sits at an angle in a host's own frame,
//! so it rides the host's drift and turn. Rooted creatures (`root`), Hullworms (`parasite`)
//! and hosted residents (`resident`) all seat themselves with `seat`, share the clear-arc
//! rule (`SPACING`, `capacity`, `free_angle`), and rely on the same contract: the link is
//! only an id plus an angle, and a host found missing is the signal to let go, so nothing
//! dangles.

use super::*;

/// Clear arc kept between neighbors on a host, in world units.
pub const SPACING: f32 = 4.0;

/// A host as the attach mechanic sees it: where it is, which way it faces and how big.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub position: Vec2,
    pub angle: f32,
    pub radius: f32,
}

impl Frame {
    pub fn of(body: &Body) -> Self {
        Self {
            position: body.position,
            angle: body.angle,
            radius: body.radius,
        }
    }

    /// Where a body of `size` anchored at `angle` (host frame) sits: its center stands off
    /// the rim by `stand` times its size (below one it is sunk in a little).
    pub fn seat(&self, angle: f32, size: f32, stand: f32) -> Vec2 {
        self.position + Vec2::from_angle(self.angle + angle) * (self.radius + size * stand)
    }
}

/// How many bodies of `size` fit around a host's rim when `fill` of it may be used.
pub fn capacity(host_radius: f32, size: f32, fill: f32) -> usize {
    ((TAU * host_radius * fill / (2.0 * size + SPACING)) as usize).max(1)
}

/// Shortest arc between two angles.
pub fn apart(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(TAU);
    d.min(TAU - d)
}

/// The angle nearest `wanted` where a body of `size` clears every `(angle, size)` in
/// `taken` on a host of `host_radius`, searching a little each way.
pub fn free_angle(taken: &[(f32, f32)], host_radius: f32, wanted: f32, size: f32) -> Option<f32> {
    (0..=10).find_map(|k| {
        let step = (k as f32 / 2.0).ceil() * 0.25 * if k % 2 == 0 { 1.0 } else { -1.0 };
        let angle = wanted + step;
        taken
            .iter()
            .all(|&(a, r)| apart(angle, a) * host_radius > r + size + SPACING)
            .then_some(angle)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seat_follows_the_host_frame() {
        let frame = Frame {
            position: Vec2::new(10.0, 0.0),
            angle: std::f32::consts::FRAC_PI_2,
            radius: 50.0,
        };
        let at = frame.seat(0.0, 10.0, 0.5);
        assert!((at - Vec2::new(10.0, 55.0)).length() < 1e-3);
    }

    #[test]
    fn free_angle_keeps_clear_and_capacity_is_at_least_one() {
        assert_eq!(capacity(1.0, 100.0, 0.9), 1);
        let taken = [(0.0, 10.0)];
        let a = free_angle(&taken, 100.0, 0.0, 10.0).unwrap();
        assert!(apart(a, 0.0) * 100.0 > 24.0);
        assert!(free_angle(&[(0.0, 10.0), (0.3, 10.0), (-0.3, 10.0)], 100.0, 0.0, 10.0).is_some());
    }
}
