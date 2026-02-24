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

    pub fn position(&self) -> (f32, f32) {
        (self.x.position, self.y.position)
    }

    pub fn snap_to_target(&mut self) {
        self.x.snap_to_target();
        self.y.snap_to_target();
    }

    pub fn is_settled(&self, threshold: f32) -> bool {
        self.x.is_settled(threshold) && self.y.is_settled(threshold)
    }
}
