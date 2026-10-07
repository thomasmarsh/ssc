//! Presentation only: the hull follows propulsion, while `Body::angle` still aims weapons.
use bevy::prelude::*;
use ssc::simulation::{Body, Input};
use std::f32::consts::{PI, TAU};

#[derive(Default)]
pub struct ShipView {
    previous: Option<(u64, f32)>,
    pub angle: f32,
    independent: bool,
    turn: f32,
}

fn angle_delta(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

impl ShipView {
    pub fn update(&mut self, ship: &Body, input: Input, time: f32) {
        let dt = match self.previous {
            Some((id, last)) if id == ship.id && time >= last => time - last,
            _ => {
                self.angle = ship.angle;
                self.independent = false;
                self.turn = 0.0;
                0.0
            }
        };
        self.previous = Some((ship.id, time));
        let movement = input
            .move_direction
            .filter(|v| v.is_finite() && v.length_squared() > 0.001);
        if movement.is_some() {
            self.independent = true;
        } else if input.thrust > 0.0 || input.turn != 0.0 {
            self.independent = false;
        }
        let target = movement.map(|v| v.to_angle()).unwrap_or_else(|| {
            if self.independent {
                self.angle
            } else {
                ship.angle
            }
        });
        if dt > 0.0 {
            // Slew only the artwork. RCS accounts for the off-axis push during the turn.
            let change = angle_delta(self.angle, target).clamp(-8.0 * dt, 8.0 * dt);
            self.angle = (self.angle + change).rem_euclid(TAU);
            self.turn = (change / dt / 8.0).clamp(-1.0, 1.0);
        }
    }

    pub fn thrusters(&self, input: Input, ship: &Body, thrust: f32, active: bool) -> Thrusters {
        if !active {
            return Thrusters::default();
        }
        let aim = Vec2::from_angle(ship.angle);
        let push = input
            .move_direction
            .filter(|v| v.is_finite())
            .map(|v| v.clamp_length_max(1.0))
            .unwrap_or_else(|| {
                aim * if input.thrust.is_finite() {
                    input.thrust.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            });
        // The brake's damping is represented by an opposing burn, never by new forces.
        let braking = if input.brake {
            -ship.velocity * 5.0 / thrust.max(1.0)
        } else {
            Vec2::ZERO
        };
        Thrusters::from_push(
            self.angle,
            (push + braking).clamp_length_max(1.0),
            self.turn,
        )
    }
}

#[derive(Default, Debug)]
pub struct Thrusters {
    pub main: f32,
    pub reverse: f32,
    /// Positive means push to port; exhaust comes from the starboard side.
    lateral: f32,
    turn: f32,
}

impl Thrusters {
    fn from_push(angle: f32, push: Vec2, turn: f32) -> Self {
        let forward = Vec2::from_angle(angle);
        let along = push.dot(forward);
        Self {
            main: along.max(0.0),
            reverse: (-along).max(0.0),
            lateral: push.dot(Vec2::new(-forward.y, forward.x)),
            turn,
        }
    }

    /// Two separated nozzles per side: translation fires a pair, turning fires diagonals.
    pub fn side(&self, sign: f32, fore: bool) -> f32 {
        let translation = (-sign * self.lateral).max(0.0);
        let rotation = (-sign * if fore { self.turn } else { -self.turn }).max(0.0);
        (translation + rotation).min(1.0)
    }
}

/// A blunt, shouldered hull, leaving the central core and flanking equipment readable.
pub fn outline(p: Vec2, r: f32, angle: f32) -> [Vec2; 9] {
    let d = Vec2::from_angle(angle);
    let s = Vec2::new(-d.y, d.x);
    [
        (1.05, 0.4),
        (0.35, 0.85),
        (-0.7, 0.85),
        (-1.05, 0.45),
        (-1.05, -0.45),
        (-0.7, -0.85),
        (0.35, -0.85),
        (1.05, -0.4),
        (1.05, 0.4),
    ]
    .map(|(x, y)| p + r * (d * x + s * y))
}

pub fn draw_thrusters(
    gizmos: &mut Gizmos,
    ship: &Body,
    angle: f32,
    jets: &Thrusters,
    time: f32,
    calm: bool,
) {
    let (p, r) = (ship.position, ship.radius);
    let d = Vec2::from_angle(angle);
    let s = Vec2::new(-d.y, d.x);
    let main = Color::srgb(1.0, 0.65, 0.24);
    let rcs = Color::srgb(0.55, 0.83, 1.0);
    let flicker = if calm {
        1.0
    } else {
        1.0 + 0.16 * (time * 45.0).sin()
    };
    let mut nozzle = |at: Vec2, exhaust: Vec2, width: f32, power: f32, color: Color| {
        let side = Vec2::new(-exhaust.y, exhaust.x) * width;
        gizmos.line_2d(at - side, at + side, color.with_alpha(0.55));
        if power > 0.015 {
            let tip = at + exhaust * r * 1.3 * power * flicker;
            gizmos.linestrip_2d([at - side, tip, at + side], color);
        }
    };
    nozzle(p - d * r * 1.05, -d, r * 0.32, jets.main, main);
    for sign in [-1.0, 1.0] {
        nozzle(
            p + d * r * 0.85 + s * sign * r * 0.55,
            d,
            r * 0.1,
            jets.reverse,
            rcs,
        );
        for fore in [false, true] {
            let x = if fore { 0.3 } else { -0.65 };
            nozzle(
                p + d * r * x + s * sign * r * 0.87,
                s * sign,
                r * 0.1,
                jets.side(sign, fore),
                rcs,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssc::simulation::Game;

    #[test]
    fn thrusters_reconstruct_world_push_for_any_hull_heading() {
        for angle in [0.0, 0.7, PI, 5.8] {
            for push in [Vec2::X, -Vec2::X, Vec2::Y, Vec2::new(-0.3, 0.6), Vec2::ZERO] {
                let jets = Thrusters::from_push(angle, push, 0.0);
                let d = Vec2::from_angle(angle);
                let reconstructed = d * (jets.main - jets.reverse)
                    + Vec2::new(-d.y, d.x) * (jets.side(-1.0, true) - jets.side(1.0, true));
                assert!(reconstructed.distance(push) < 0.00001);
            }
        }
    }

    #[test]
    fn rotation_uses_diagonal_rcs_pairs() {
        let jets = Thrusters::from_push(0.0, Vec2::ZERO, 1.0);
        assert_eq!(jets.side(-1.0, true), 1.0);
        assert_eq!(jets.side(1.0, false), 1.0);
        assert_eq!(jets.side(1.0, true), 0.0);
        assert_eq!(jets.side(-1.0, false), 0.0);
        assert_eq!(jets.main, 0.0);
    }

    #[test]
    fn hull_follows_movement_and_holds_while_aim_changes() {
        let game = Game::new(42);
        let mut ship = game.player().unwrap().clone();
        ship.angle = 0.0;
        let mut view = ShipView::default();
        let input = Input {
            move_direction: Some(Vec2::Y),
            ..default()
        };
        view.update(&ship, input, 0.0);
        view.update(&ship, input, 1.0);
        assert!((view.angle - PI / 2.0).abs() < 0.00001);
        ship.angle = PI;
        view.update(&ship, Input::default(), 2.0);
        assert!((view.angle - PI / 2.0).abs() < 0.00001);
        view.update(
            &ship,
            Input {
                thrust: 1.0,
                ..default()
            },
            3.0,
        );
        assert!((view.angle - PI).abs() < 0.00001);
    }

    #[test]
    fn hull_turns_across_angle_wrap_and_resets_on_respawn() {
        let game = Game::new(42);
        let mut ship = game.player().unwrap().clone();
        ship.angle = TAU - 0.1;
        let mut view = ShipView::default();
        let input = Input {
            move_direction: Some(Vec2::from_angle(0.1)),
            ..default()
        };
        view.update(&ship, input, 1.0);
        view.update(&ship, input, 1.01);
        assert!(view.turn > 0.0, "take the short counterclockwise turn");
        let held = view.angle;
        view.update(&ship, input, 1.01);
        assert_eq!(view.angle, held, "a frozen clock must freeze the hull");
        ship.id += 1;
        ship.angle = PI;
        view.update(&ship, Input::default(), 1.02);
        assert_eq!(view.angle, PI);
        assert_eq!(view.turn, 0.0);
    }

    #[test]
    fn coast_is_dark_braking_opposes_velocity_and_pause_is_dark() {
        let game = Game::new(42);
        let mut ship = game.player().unwrap().clone();
        ship.angle = 0.0;
        ship.velocity = Vec2::X * 100.0;
        let view = ShipView::default();
        assert_eq!(
            view.thrusters(Input::default(), &ship, 500.0, true).main,
            0.0
        );
        let brake = Input {
            brake: true,
            ..default()
        };
        assert_eq!(view.thrusters(brake, &ship, 500.0, true).reverse, 1.0);
        assert_eq!(view.thrusters(brake, &ship, 500.0, false).reverse, 0.0);
    }
}
