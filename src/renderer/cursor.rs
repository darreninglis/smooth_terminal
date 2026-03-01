use crate::animation::spring::Spring2D;
use crate::renderer::cell_bg::CellBgVertex;

/// Cursor animator using 4 corner springs.
/// Each corner of the cursor block has its own spring.
/// On movement, leading corners (in travel direction) get higher omega.
pub struct CursorAnimator {
    /// 4 corner springs: top-left, top-right, bottom-right, bottom-left
    pub corners: [Spring2D; 4],
    pub target_col: usize,
    pub target_row: usize,
    pub color: [f32; 4],
    pub cell_w: f32,
    pub cell_h: f32,
    pub trail_enabled: bool,
    base_omega: f32,
    /// Snap instead of animate for the first N ticks so the shell prompt
    /// appears instantly rather than sliding in from the corner.
    startup_snaps: u32,
}

impl CursorAnimator {
    pub fn new(omega: f32, color: [f32; 4], cell_w: f32, cell_h: f32, trail_enabled: bool) -> Self {
        let corners = [
            Spring2D::new(omega),
            Spring2D::new(omega),
            Spring2D::new(omega),
            Spring2D::new(omega),
        ];
        Self {
            corners,
            target_col: 0,
            target_row: 0,
            color,
            cell_w,
            cell_h,
            trail_enabled,
            base_omega: omega,
            startup_snaps: 30,
        }
    }

    /// Update cell size (after font/resize change)
    pub fn set_cell_size(&mut self, w: f32, h: f32) {
        self.cell_w = w;
        self.cell_h = h;
    }

    /// Move cursor to new grid position. Sets spring targets.
    /// On trail mode: leading corners get higher omega.
    pub fn move_to(
        &mut self,
        col: usize,
        row: usize,
        pane_x: f32,
        pane_y: f32,
        scroll_offset: f32,
    ) {
        let prev_col = self.target_col;
        let prev_row = self.target_row;
        self.target_col = col;
        self.target_row = row;

        let px = pane_x + col as f32 * self.cell_w;
        let py = pane_y + row as f32 * self.cell_h + scroll_offset;

        // Corner positions: TL, TR, BR, BL
        let targets = [
            (px, py),
            (px + self.cell_w, py),
            (px + self.cell_w, py + self.cell_h),
            (px, py + self.cell_h),
        ];

        if self.trail_enabled {
            // Travel vector
            let dx = col as f32 - prev_col as f32;
            let dy = row as f32 - prev_row as f32;

            // Assign omega based on dot product with travel direction (corners aligned with
            // travel direction get snappier response)
            // Corner vectors from center: TL=(-1,-1), TR=(1,-1), BR=(1,1), BL=(-1,1)
            let corner_dirs: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let dir = (dx / len, dy / len);

            for (i, (cdx, cdy)) in corner_dirs.iter().enumerate() {
                let dot = cdx * dir.0 + cdy * dir.1;
                // Leading corners (dot > 0) get higher omega
                let corner_omega = if dot > 0.0 {
                    self.base_omega * (1.0 + dot * 0.5)
                } else {
                    self.base_omega * (1.0 + dot * 0.3)
                };
                self.corners[i].x.omega = corner_omega;
                self.corners[i].y.omega = corner_omega;
                self.corners[i].set_target(targets[i].0, targets[i].1);
            }
        } else {
            for (i, (tx, ty)) in targets.iter().enumerate() {
                self.corners[i].set_target(*tx, *ty);
            }
        }
    }

    /// Clamp each corner so it never lags more than `max_x`/`max_y` pixels
    /// behind its target. Preserves velocity so the spring still animates
    /// smoothly from the clamped position — the cursor glides rather than
    /// teleporting, but never falls visibly behind during fast typing.
    pub fn clamp_lag(&mut self, max_x: f32, max_y: f32) {
        for corner in &mut self.corners {
            let dx = corner.x.position - corner.x.target;
            if dx.abs() > max_x {
                corner.x.position = corner.x.target + dx.signum() * max_x;
            }
            let dy = corner.y.position - corner.y.target;
            if dy.abs() > max_y {
                corner.y.position = corner.y.target + dy.signum() * max_y;
            }
        }
    }

    /// Snap all corners to current target (no animation — use on init/resize)
    pub fn snap_to(
        &mut self,
        col: usize,
        row: usize,
        pane_x: f32,
        pane_y: f32,
        scroll_offset: f32,
    ) {
        let px = pane_x + col as f32 * self.cell_w;
        let py = pane_y + row as f32 * self.cell_h + scroll_offset;
        let targets = [
            (px, py),
            (px + self.cell_w, py),
            (px + self.cell_w, py + self.cell_h),
            (px, py + self.cell_h),
        ];
        for (i, (tx, ty)) in targets.iter().enumerate() {
            self.corners[i].set_target(*tx, *ty);
            self.corners[i].x.position = *tx;
            self.corners[i].y.position = *ty;
            self.corners[i].x.velocity = 0.0;
            self.corners[i].y.velocity = 0.0;
        }
        self.target_col = col;
        self.target_row = row;
    }

    pub fn is_warming_up(&self) -> bool {
        self.startup_snaps > 0
    }

    pub fn tick(&mut self, dt: f32) {
        self.startup_snaps = self.startup_snaps.saturating_sub(1);
        for corner in &mut self.corners {
            corner.tick(dt);
        }
    }

    /// Build vertices for the animated cursor quad (deformed by corner springs)
    pub fn build_vertices(&self, surface_w: f32, surface_h: f32) -> [CellBgVertex; 4] {
        let to_ndc_x = |px: f32| (px / surface_w) * 2.0 - 1.0;
        let to_ndc_y = |py: f32| 1.0 - (py / surface_h) * 2.0;

        let color = self.color;
        let corners = &self.corners;
        [
            CellBgVertex { position: [to_ndc_x(corners[0].x.position), to_ndc_y(corners[0].y.position)], color },
            CellBgVertex { position: [to_ndc_x(corners[1].x.position), to_ndc_y(corners[1].y.position)], color },
            CellBgVertex { position: [to_ndc_x(corners[2].x.position), to_ndc_y(corners[2].y.position)], color },
            CellBgVertex { position: [to_ndc_x(corners[3].x.position), to_ndc_y(corners[3].y.position)], color },
        ]
    }

}
