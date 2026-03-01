/// Critically damped spring using analytic solution (Ryan Juckett method).
/// Pre-computed coefficients for a fixed dt. O(1) per tick, no oscillation.
#[derive(Debug, Clone)]
pub struct CriticallyDampedSpring {
    pub position: f32,
    pub velocity: f32,
    pub target: f32,
    /// Angular frequency (stiffness). Higher = snappier.
    pub omega: f32,
}

impl CriticallyDampedSpring {
    pub fn new(omega: f32) -> Self {
        Self { position: 0.0, velocity: 0.0, target: 0.0, omega }
    }

    #[allow(dead_code)]
    pub fn with_position(omega: f32, position: f32) -> Self {
        Self { position, velocity: 0.0, target: position, omega }
    }

    /// Tick by `dt` seconds using analytic critically-damped spring solution.
    pub fn tick(&mut self, dt: f32) {
        let x = self.position - self.target;
        let v = self.velocity;
        let w = self.omega;

        // Analytic solution for critically damped spring:
        // x(t) = e^(-wt) * ((x0 + (v0 + w*x0)*t))
        // v(t) = e^(-wt) * (v0 - w*(v0 + w*x0)*t)
        let exp = (-w * dt).exp();
        let c1 = x;
        let c2 = v + w * x;

        self.position = self.target + exp * (c1 + c2 * dt);
        self.velocity = exp * (v - w * c2 * dt);
    }

    pub fn snap_to_target(&mut self) {
        self.position = self.target;
        self.velocity = 0.0;
    }

    pub fn is_settled(&self, threshold: f32) -> bool {
        (self.position - self.target).abs() < threshold && self.velocity.abs() < threshold
    }
}

/// 2D spring (pair of 1D springs)
#[derive(Debug, Clone)]
pub struct Spring2D {
    pub x: CriticallyDampedSpring,
    pub y: CriticallyDampedSpring,
}

impl Spring2D {
    pub fn new(omega: f32) -> Self {
        Self {
            x: CriticallyDampedSpring::new(omega),
            y: CriticallyDampedSpring::new(omega),
        }
    }

    #[allow(dead_code)]
    pub fn with_position(omega: f32, px: f32, py: f32) -> Self {
        Self {
            x: CriticallyDampedSpring::with_position(omega, px),
            y: CriticallyDampedSpring::with_position(omega, py),
        }
    }

    pub fn set_target(&mut self, tx: f32, ty: f32) {
        self.x.target = tx;
        self.y.target = ty;
    }

    pub fn tick(&mut self, dt: f32) {
        self.x.tick(dt);
        self.y.tick(dt);
    }

    #[allow(dead_code)]
    pub fn position(&self) -> (f32, f32) {
        (self.x.position, self.y.position)
    }

    #[allow(dead_code)]
    pub fn snap_to_target(&mut self) {
        self.x.snap_to_target();
        self.y.snap_to_target();
    }

    #[allow(dead_code)]
    pub fn is_settled(&self, threshold: f32) -> bool {
        self.x.is_settled(threshold) && self.y.is_settled(threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CriticallyDampedSpring ──────────────────────────────────────────

    #[test]
    fn new_starts_at_zero() {
        let s = CriticallyDampedSpring::new(10.0);
        assert_eq!(s.position, 0.0);
        assert_eq!(s.velocity, 0.0);
        assert_eq!(s.target, 0.0);
    }

    #[test]
    fn with_position_sets_pos_and_target() {
        let s = CriticallyDampedSpring::with_position(10.0, 5.0);
        assert_eq!(s.position, 5.0);
        assert_eq!(s.target, 5.0);
        assert_eq!(s.velocity, 0.0);
    }

    #[test]
    fn tick_converges_toward_target() {
        let mut s = CriticallyDampedSpring::new(10.0);
        s.target = 100.0;
        for _ in 0..1000 {
            s.tick(1.0 / 60.0);
        }
        assert!((s.position - 100.0).abs() < 0.01);
    }

    #[test]
    fn tick_no_oscillation_past_target() {
        // Critically damped should not overshoot significantly
        let mut s = CriticallyDampedSpring::new(10.0);
        s.target = 100.0;
        let mut max_pos = 0.0f32;
        for _ in 0..1000 {
            s.tick(1.0 / 60.0);
            max_pos = max_pos.max(s.position);
        }
        // Should not overshoot target by more than a tiny amount
        assert!(max_pos < 101.0, "overshot to {}", max_pos);
    }

    #[test]
    fn snap_to_target_zeroes_velocity() {
        let mut s = CriticallyDampedSpring::new(10.0);
        s.target = 50.0;
        s.tick(1.0 / 60.0); // move a bit
        s.snap_to_target();
        assert_eq!(s.position, 50.0);
        assert_eq!(s.velocity, 0.0);
    }

    #[test]
    fn is_settled_true_at_target() {
        let s = CriticallyDampedSpring::with_position(10.0, 5.0);
        assert!(s.is_settled(0.01));
    }

    #[test]
    fn is_settled_false_when_displaced() {
        let mut s = CriticallyDampedSpring::new(10.0);
        s.target = 100.0;
        assert!(!s.is_settled(0.01));
    }

    #[test]
    fn is_settled_false_with_velocity() {
        let mut s = CriticallyDampedSpring::with_position(10.0, 100.0);
        s.velocity = 50.0;
        assert!(!s.is_settled(0.01));
    }

    // ── Spring2D ────────────────────────────────────────────────────────

    #[test]
    fn spring2d_tick_both_axes() {
        let mut s = Spring2D::new(10.0);
        s.set_target(100.0, 200.0);
        for _ in 0..1000 {
            s.tick(1.0 / 60.0);
        }
        let (px, py) = s.position();
        assert!((px - 100.0).abs() < 0.01);
        assert!((py - 200.0).abs() < 0.01);
    }

    #[test]
    fn spring2d_set_target() {
        let mut s = Spring2D::new(10.0);
        s.set_target(3.0, 7.0);
        assert_eq!(s.x.target, 3.0);
        assert_eq!(s.y.target, 7.0);
    }

    #[test]
    fn spring2d_is_settled() {
        let s = Spring2D::with_position(10.0, 5.0, 5.0);
        assert!(s.is_settled(0.01));
    }

    #[test]
    fn spring2d_snap_to_target() {
        let mut s = Spring2D::new(10.0);
        s.set_target(50.0, 60.0);
        s.tick(1.0 / 60.0);
        s.snap_to_target();
        assert_eq!(s.position(), (50.0, 60.0));
        assert!(s.is_settled(0.01));
    }
}
