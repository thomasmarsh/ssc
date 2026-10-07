//! Game feel that the rules own: the score chain. Pure and tested; the adapter only draws it.
//! (Screen shake, hit stops and the like are added below as their own types.)

/// Seconds a chain survives without a new link.
pub const STREAK_WINDOW: f32 = 3.0;
/// Each link after the first adds this much to the score multiplier, up to `STREAK_MAX`.
pub const STREAK_STEP: f32 = 0.25;
pub const STREAK_MAX: f32 = 3.0;

/// The arcade chain: kills, grazes and perfect parries within `STREAK_WINDOW` seconds of each
/// other grow a score multiplier (never a damage one).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Streak {
    links: u32,
    left: f32,
}

/// The score multiplier of a chain with `links` links.
pub fn streak_multiplier(links: u32) -> f32 {
    (1.0 + STREAK_STEP * links.saturating_sub(1) as f32).min(STREAK_MAX)
}

impl Streak {
    /// Adds a link and returns the multiplier it earns.
    pub fn link(&mut self) -> f32 {
        self.links = self.links.saturating_add(1);
        self.left = STREAK_WINDOW;
        streak_multiplier(self.links)
    }

    pub fn tick(&mut self, dt: f32) {
        if self.left > 0.0 {
            self.left = (self.left - dt).max(0.0);
            if self.left == 0.0 {
                self.links = 0;
            }
        }
    }

    pub fn links(&self) -> u32 {
        self.links
    }

    /// The multiplier and the share of the window left, while a chain of two or more runs.
    pub fn view(&self) -> Option<(f32, f32)> {
        (self.links >= 2).then(|| (streak_multiplier(self.links), self.left / STREAK_WINDOW))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_multiplier_grows_a_step_per_link_and_caps() {
        assert_eq!(streak_multiplier(0), 1.0);
        assert_eq!(streak_multiplier(1), 1.0);
        assert!((streak_multiplier(2) - 1.25).abs() < 1e-6);
        assert_eq!(streak_multiplier(100), STREAK_MAX);
    }

    #[test]
    fn a_chain_breaks_when_the_window_runs_out() {
        let mut streak = Streak::default();
        assert_eq!(streak.link(), 1.0);
        assert!(streak.view().is_none());
        assert!((streak.link() - 1.25).abs() < 1e-6);
        assert!(streak.view().is_some());
        streak.tick(STREAK_WINDOW - 0.1);
        assert_eq!(streak.links(), 2);
        streak.tick(0.2);
        assert_eq!(streak.links(), 0);
        assert!(streak.view().is_none());
        assert_eq!(streak.link(), 1.0);
    }
}
