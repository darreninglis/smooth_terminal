/// A rectangle in logical pixels (top-left origin)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

/// Pane layout tree
#[derive(Debug, Clone)]
pub enum Layout {
    /// A single leaf pane identified by pane_id
    Leaf(usize),
    /// Two panes side by side (left | right)
    HSplit {
        left: Box<Layout>,
        right: Box<Layout>,
        /// Fraction of total width given to `left` (0..1)
        ratio: f32,
    },
    /// Two panes stacked (top / bottom)
    VSplit {
        top: Box<Layout>,
        bottom: Box<Layout>,
        /// Fraction of total height given to `top` (0..1)
        ratio: f32,
    },
}

impl Layout {
    /// Recursively compute pixel rect for each leaf pane.
    /// Returns Vec of (pane_id, Rect).
    pub fn compute_rects(&self, rect: Rect) -> Vec<(usize, Rect)> {
        match self {
            Layout::Leaf(id) => vec![(*id, rect)],
            Layout::HSplit { left, right, ratio } => {
                let left_w = rect.width * ratio;
                let right_w = rect.width - left_w;
                let left_rect = Rect::new(rect.x, rect.y, left_w, rect.height);
                let right_rect = Rect::new(rect.x + left_w, rect.y, right_w, rect.height);
                let mut rects = left.compute_rects(left_rect);
                rects.extend(right.compute_rects(right_rect));
                rects
            }
            Layout::VSplit { top, bottom, ratio } => {
                let top_h = rect.height * ratio;
                let bottom_h = rect.height - top_h;
                let top_rect = Rect::new(rect.x, rect.y, rect.width, top_h);
                let bottom_rect = Rect::new(rect.x, rect.y + top_h, rect.width, bottom_h);
                let mut rects = top.compute_rects(top_rect);
                rects.extend(bottom.compute_rects(bottom_rect));
                rects
            }
        }
    }

    /// Collect all pane IDs in this layout
    pub fn pane_ids(&self) -> Vec<usize> {
        match self {
            Layout::Leaf(id) => vec![*id],
            Layout::HSplit { left, right, .. } => {
                let mut ids = left.pane_ids();
                ids.extend(right.pane_ids());
                ids
            }
            Layout::VSplit { top, bottom, .. } => {
                let mut ids = top.pane_ids();
                ids.extend(bottom.pane_ids());
                ids
            }
        }
    }

    /// Replace the leaf with `target_id` with a horizontal split
    pub fn split_h(self, target_id: usize, new_id: usize) -> Self {
        match self {
            Layout::Leaf(id) if id == target_id => Layout::HSplit {
                left: Box::new(Layout::Leaf(target_id)),
                right: Box::new(Layout::Leaf(new_id)),
                ratio: 0.5,
            },
            Layout::HSplit { left, right, ratio } => Layout::HSplit {
                left: Box::new(left.split_h(target_id, new_id)),
                right: Box::new(right.split_h(target_id, new_id)),
                ratio,
            },
            Layout::VSplit { top, bottom, ratio } => Layout::VSplit {
                top: Box::new(top.split_h(target_id, new_id)),
                bottom: Box::new(bottom.split_h(target_id, new_id)),
                ratio,
            },
            other => other,
        }
    }

    /// Replace the leaf with `target_id` with a vertical split
    pub fn split_v(self, target_id: usize, new_id: usize) -> Self {
        match self {
            Layout::Leaf(id) if id == target_id => Layout::VSplit {
                top: Box::new(Layout::Leaf(target_id)),
                bottom: Box::new(Layout::Leaf(new_id)),
                ratio: 0.5,
            },
            Layout::HSplit { left, right, ratio } => Layout::HSplit {
                left: Box::new(left.split_v(target_id, new_id)),
                right: Box::new(right.split_v(target_id, new_id)),
                ratio,
            },
            Layout::VSplit { top, bottom, ratio } => Layout::VSplit {
                top: Box::new(top.split_v(target_id, new_id)),
                bottom: Box::new(bottom.split_v(target_id, new_id)),
                ratio,
            },
            other => other,
        }
    }

    /// Remove pane with `target_id`. Returns None if this node is removed.
    pub fn remove(self, target_id: usize) -> Option<Self> {
        match self {
            Layout::Leaf(id) if id == target_id => None,
            Layout::Leaf(_) => Some(self),
            Layout::HSplit { left, right, ratio } => {
                match (left.remove(target_id), right.remove(target_id)) {
                    (None, Some(r)) => Some(r),
                    (Some(l), None) => Some(l),
                    (Some(l), Some(r)) => Some(Layout::HSplit {
                        left: Box::new(l),
                        right: Box::new(r),
                        ratio,
                    }),
                    (None, None) => None,
                }
            }
            Layout::VSplit { top, bottom, ratio } => {
                match (top.remove(target_id), bottom.remove(target_id)) {
                    (None, Some(b)) => Some(b),
                    (Some(t), None) => Some(t),
                    (Some(t), Some(b)) => Some(Layout::VSplit {
                        top: Box::new(t),
                        bottom: Box::new(b),
                        ratio,
                    }),
                    (None, None) => None,
                }
            }
        }
    }
}
