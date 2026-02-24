use super::spring::CriticallyDampedSpring;

/// Per-pane scroll spring. `position` is pixel offset (positive = scrolled up).
pub struct ScrollSpring {
    spring: CriticallyDampedSpring,
    /// Maximum scroll offset in pixels (updated when content changes)
    pub max_offset: f32,
}

impl ScrollSpring {
    pub fn new(omega: f32) -> Self {
        Self {
            spring: CriticallyDampedSpring::new(omega),
            max_offset: 0.0,
        }
    }

    pub fn tick(&mut self, dt: f32) {
        // Clamp target to valid range
        self.spring.target = self.spring.target.max(0.0).min(self.max_offset);
        self.spring.tick(dt);
        // Clamp position
        self.spring.position = self.spring.position.max(0.0).min(self.max_offset + 50.0);
    }

    /// Add delta (positive = scroll down/away, negative = scroll up/toward top)
    pub fn scroll_by(&mut self, delta_pixels: f32) {
        self.spring.target = (self.spring.target + delta_pixels)
            .max(0.0)
            .min(self.max_offset);
    }

    pub fn set_target_pixels(&mut self, offset: f32) {
        self.spring.target = offset.max(0.0).min(self.max_offset);
    }

    pub fn pixel_offset(&self) -> f32 {
        self.spring.position
    }

    pub fn is_settled(&self) -> bool {
        self.spring.is_settled(0.5)
    }

    pub fn snap_to_bottom(&mut self) {
        self.spring.target = 0.0;
        self.spring.snap_to_target();
    }
}
