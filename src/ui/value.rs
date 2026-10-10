//! Pure stepping of a bounded number for sliders and steppers: how far one d-pad press moves
//! it, how holding accelerates, whole numbers, wide ranges that step by ratio, and where the
//! default sits on the track. Rules never live here; this only proposes the next value, and the
//! registry (`Game::tune_set`) clamps and refuses.

/// A number's bounds, as the registry reports them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Span {
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// Holds a whole number.
    pub whole: bool,
}

impl Span {
    /// Ranges that cover three or more decades from a positive minimum step by ratio.
    pub fn logarithmic(&self) -> bool {
        self.min > 0.0 && self.max / self.min >= 1000.0
    }

    /// The smallest step: about a fiftieth of the range, on a 1, 2, 5 grid, never below 1 for a
    /// whole number.
    pub fn base_step(&self) -> f32 {
        let range = (self.max - self.min).abs().max(f32::MIN_POSITIVE);
        let raw = range / 50.0;
        let decade = 10f32.powf(raw.log10().floor());
        let unit = raw / decade;
        let nice = if unit < 1.5 {
            1.0
        } else if unit < 3.5 {
            2.0
        } else if unit < 7.5 {
            5.0
        } else {
            10.0
        };
        let step = nice * decade;
        if self.whole {
            step.max(1.0).round()
        } else {
            step
        }
    }

    /// The step multiplier for the nth repeat of a held key: 1 for a tap, growing to 10 then 50.
    pub fn accel(count: u32) -> f32 {
        match count {
            0..=5 => 1.0,
            6..=14 => 2.0,
            15..=29 => 5.0,
            30..=59 => 10.0,
            _ => 25.0,
        }
    }

    /// The value after one step in `dir` (negative or positive) at repeat `count`, within the
    /// span. Crossing the default lands on it first so it is easy to come back to.
    pub fn stepped(&self, current: f32, dir: i32, count: u32) -> f32 {
        let dir = dir.signum() as f32;
        if dir == 0.0 || !current.is_finite() {
            return current;
        }
        let mult = Self::accel(count);
        let mut next = if self.logarithmic() {
            // One tap is a twelfth of a decade.
            current * 10f32.powf(dir * mult / 12.0)
        } else {
            current + dir * self.base_step() * mult
        };
        if self.whole {
            next = next.round();
            if next == current {
                next = current + dir;
            }
        } else {
            next = sig(next, 4);
        }
        if (current - self.default) * (next - self.default) < 0.0 {
            next = self.default;
        }
        next.clamp(self.min, self.max)
    }

    /// Where `value` sits on the track, 0 to 1 (by ratio for a wide range).
    pub fn fraction(&self, value: f32) -> f32 {
        let f = if self.logarithmic() {
            (value.max(self.min) / self.min).ln() / (self.max / self.min).ln()
        } else if self.max > self.min {
            (value - self.min) / (self.max - self.min)
        } else {
            0.0
        };
        if f.is_finite() {
            f.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// `value` rounded to `digits` significant digits (removes the noise of repeated float steps).
pub fn sig(value: f32, digits: i32) -> f32 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    let scale = 10f32.powi(digits - 1 - value.abs().log10().floor() as i32);
    (value * scale).round() / scale
}

/// A value as short text: whole numbers plain, others to at most four significant digits, large
/// and tiny magnitudes in scientific form.
pub fn format(value: f32) -> String {
    if !value.is_finite() {
        return "?".to_string();
    }
    let v = sig(value, 4);
    let a = v.abs();
    if v == 0.0 {
        "0".to_string()
    } else if !(1.0e-3..1.0e6).contains(&a) {
        format!("{v:.3e}")
    } else if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: Span = Span {
        min: 0.0,
        max: 1.0,
        default: 0.35,
        whole: false,
    };

    #[test]
    fn a_tap_steps_a_fiftieth_of_the_range_on_a_nice_grid() {
        assert_eq!(UNIT.base_step(), 0.02);
        let wide = Span {
            min: 0.0,
            max: 1000.0,
            default: 100.0,
            whole: false,
        };
        assert_eq!(wide.base_step(), 20.0);
        assert_eq!(UNIT.stepped(0.5, 1, 0), 0.52);
        assert_eq!(UNIT.stepped(0.5, -1, 0), 0.48);
    }

    #[test]
    fn whole_numbers_step_by_at_least_one_and_stay_whole() {
        let s = Span {
            min: 1.0,
            max: 12.0,
            default: 4.0,
            whole: true,
        };
        assert_eq!(s.base_step(), 1.0);
        assert_eq!(s.stepped(6.0, 1, 0), 7.0);
        assert_eq!(s.stepped(12.0, 1, 0), 12.0, "stops at the maximum");
        assert_eq!(s.stepped(1.0, -1, 0), 1.0, "and at the minimum");
        let fine = Span {
            min: 0.0,
            max: 10.0,
            default: 5.0,
            whole: true,
        };
        assert_eq!(fine.stepped(3.0, 1, 0), 4.0);
    }

    #[test]
    fn holding_accelerates() {
        assert_eq!(Span::accel(0), 1.0);
        assert!(Span::accel(10) > Span::accel(2));
        assert!(Span::accel(40) > Span::accel(10));
        let fast = UNIT.stepped(0.5, 1, 40);
        assert!(fast > 0.5 + 5.0 * 0.02);
    }

    #[test]
    fn stepping_never_leaves_the_span_and_lands_on_the_default() {
        assert_eq!(UNIT.stepped(0.99, 1, 60), 1.0);
        assert_eq!(UNIT.stepped(0.01, -1, 60), 0.0);
        // From just below the default a tap lands on it, not past it.
        assert_eq!(UNIT.stepped(0.34, 1, 0), 0.35);
        assert_eq!(UNIT.stepped(0.36, -1, 0), 0.35);
        // And from the default it moves on.
        assert_eq!(UNIT.stepped(0.35, 1, 0), 0.37);
    }

    #[test]
    fn a_wide_range_steps_by_ratio() {
        let s = Span {
            min: 1.0,
            max: 3.2e8,
            default: 86_400.0,
            whole: false,
        };
        assert!(s.logarithmic());
        let up = s.stepped(10.0, 1, 0);
        assert!(up > 10.0 && up < 13.0, "{up}");
        assert!(s.fraction(1.0) < 0.001);
        assert!((s.fraction(3.2e8) - 1.0).abs() < 1e-4);
        assert!(s.fraction(86_400.0) > 0.4 && s.fraction(86_400.0) < 0.7);
    }

    #[test]
    fn fraction_is_total_for_odd_input() {
        assert_eq!(UNIT.fraction(-5.0), 0.0);
        assert_eq!(UNIT.fraction(5.0), 1.0);
        assert_eq!(UNIT.fraction(f32::NAN), 0.0);
        let flat = Span {
            min: 2.0,
            max: 2.0,
            default: 2.0,
            whole: false,
        };
        assert_eq!(flat.fraction(2.0), 0.0);
        assert_eq!(flat.stepped(2.0, 1, 0), 2.0);
    }

    #[test]
    fn values_format_short() {
        assert_eq!(format(0.0), "0");
        assert_eq!(format(12.0), "12");
        assert_eq!(format(0.35), "0.35");
        assert_eq!(format(0.1 + 0.2), "0.3");
        assert_eq!(format(86_400.0), "86400");
        assert_eq!(format(3.2e8), "3.200e8");
        assert_eq!(format(f32::NAN), "?");
    }
}
