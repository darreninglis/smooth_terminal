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

    #[allow(dead_code)]
    pub fn set_target_pixels(&mut self, offset: f32) {
        self.spring.target = offset.max(0.0).min(self.max_offset);
    }

    pub fn pixel_offset(&self) -> f32 {
        self.spring.position
    }

    #[allow(dead_code)]
    pub fn is_settled(&self) -> bool {
        self.spring.is_settled(0.5)
    }

    pub fn snap_to_bottom(&mut self) {
        self.spring.target = 0.0;
        self.spring.snap_to_target();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_by_clamps_to_zero() {
        let mut s = ScrollSpring::new(15.0);
        s.max_offset = 100.0;
        s.scroll_by(-50.0);
        assert_eq!(s.spring.target, 0.0);
    }

    #[test]
    fn scroll_by_clamps_to_max() {
        let mut s = ScrollSpring::new(15.0);
        s.max_offset = 100.0;
        s.scroll_by(200.0);
        assert_eq!(s.spring.target, 100.0);
    }

    #[test]
    fn scroll_by_normal() {
        let mut s = ScrollSpring::new(15.0);
        s.max_offset = 100.0;
        s.scroll_by(30.0);
        assert_eq!(s.spring.target, 30.0);
    }

    #[test]
    fn set_target_pixels_clamps() {
        let mut s = ScrollSpring::new(15.0);
        s.max_offset = 50.0;
        s.set_target_pixels(999.0);
        assert_eq!(s.spring.target, 50.0);
        s.set_target_pixels(-10.0);
        assert_eq!(s.spring.target, 0.0);
    }

    #[test]
    fn snap_to_bottom_resets() {
        let mut s = ScrollSpring::new(15.0);
        s.max_offset = 100.0;
        s.scroll_by(50.0);
        s.tick(1.0 / 60.0);
        s.snap_to_bottom();
        assert_eq!(s.pixel_offset(), 0.0);
        assert_eq!(s.spring.target, 0.0);
    }

    #[test]
    fn tick_clamps_position() {
        let mut s = ScrollSpring::new(15.0);
        s.max_offset = 100.0;
        // Force spring position negative
        s.spring.position = -10.0;
        s.tick(1.0 / 60.0);
        assert!(s.pixel_offset() >= 0.0);
    }

    #[test]
    fn is_settled_when_at_rest() {
        let s = ScrollSpring::new(15.0);
        assert!(s.is_settled());
    }
}
