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

    /// Nudge the split ratio of any split that directly contains `target_id`.
    /// `h_delta` adjusts HSplit ratio (positive → widen left pane, negative → widen right).
    /// `v_delta` adjusts VSplit ratio (positive → widen top pane, negative → widen bottom).
    pub fn nudge_ratio_for(&mut self, target_id: usize, h_delta: f32, v_delta: f32) {
        match self {
            Layout::HSplit { left, right, ratio } => {
                if left.contains(target_id) || right.contains(target_id) {
                    *ratio = (*ratio + h_delta).clamp(0.1, 0.9);
                } else {
                    left.nudge_ratio_for(target_id, h_delta, v_delta);
                    right.nudge_ratio_for(target_id, h_delta, v_delta);
                }
            }
            Layout::VSplit { top, bottom, ratio } => {
                if top.contains(target_id) || bottom.contains(target_id) {
                    *ratio = (*ratio + v_delta).clamp(0.1, 0.9);
                } else {
                    top.nudge_ratio_for(target_id, h_delta, v_delta);
                    bottom.nudge_ratio_for(target_id, h_delta, v_delta);
                }
            }
            Layout::Leaf(_) => {}
        }
    }

    /// Returns true if this subtree contains the given pane ID.
    pub fn contains(&self, target_id: usize) -> bool {
        match self {
            Layout::Leaf(id) => *id == target_id,
            Layout::HSplit { left, right, .. } => left.contains(target_id) || right.contains(target_id),
            Layout::VSplit { top, bottom, .. } => top.contains(target_id) || bottom.contains(target_id),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn leaf_compute_rects_returns_input() {
        let r = Rect::new(10.0, 20.0, 100.0, 200.0);
        let result = Layout::Leaf(0).compute_rects(r);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, 0);
        assert_eq!(result[0].1, r);
    }

    #[test]
    fn hsplit_divides_width() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let rects = layout.compute_rects(Rect::new(0.0, 0.0, 100.0, 50.0));
        assert_eq!(rects.len(), 2);
        assert!(approx_eq(rects[0].1.width, 50.0));
        assert!(approx_eq(rects[1].1.width, 50.0));
        assert!(approx_eq(rects[1].1.x, 50.0));
        // Heights should be equal
        assert!(approx_eq(rects[0].1.height, 50.0));
        assert!(approx_eq(rects[1].1.height, 50.0));
    }

    #[test]
    fn vsplit_divides_height() {
        let layout = Layout::VSplit {
            top: Box::new(Layout::Leaf(0)),
            bottom: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let rects = layout.compute_rects(Rect::new(0.0, 0.0, 100.0, 200.0));
        assert_eq!(rects.len(), 2);
        assert!(approx_eq(rects[0].1.height, 100.0));
        assert!(approx_eq(rects[1].1.height, 100.0));
        assert!(approx_eq(rects[1].1.y, 100.0));
    }

    #[test]
    fn nested_splits() {
        // HSplit { VSplit(0,1), Leaf(2) }
        let layout = Layout::HSplit {
            left: Box::new(Layout::VSplit {
                top: Box::new(Layout::Leaf(0)),
                bottom: Box::new(Layout::Leaf(1)),
                ratio: 0.5,
            }),
            right: Box::new(Layout::Leaf(2)),
            ratio: 0.5,
        };
        let rects = layout.compute_rects(Rect::new(0.0, 0.0, 200.0, 200.0));
        assert_eq!(rects.len(), 3);
        // Pane 0: top-left quadrant
        assert!(approx_eq(rects[0].1.width, 100.0));
        assert!(approx_eq(rects[0].1.height, 100.0));
        // Pane 1: bottom-left quadrant
        assert!(approx_eq(rects[1].1.y, 100.0));
        // Pane 2: right half
        assert!(approx_eq(rects[2].1.x, 100.0));
        assert!(approx_eq(rects[2].1.width, 100.0));
        assert!(approx_eq(rects[2].1.height, 200.0));
    }

    #[test]
    fn pane_ids_collects_all() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::VSplit {
                top: Box::new(Layout::Leaf(1)),
                bottom: Box::new(Layout::Leaf(2)),
                ratio: 0.5,
            }),
            ratio: 0.5,
        };
        let mut ids = layout.pane_ids();
        ids.sort();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    #[test]
    fn split_h_creates_hsplit() {
        let layout = Layout::Leaf(0).split_h(0, 1);
        match &layout {
            Layout::HSplit { left, right, ratio } => {
                assert!(matches!(**left, Layout::Leaf(0)));
                assert!(matches!(**right, Layout::Leaf(1)));
                assert!(approx_eq(*ratio, 0.5));
            }
            _ => panic!("expected HSplit"),
        }
    }

    #[test]
    fn split_v_creates_vsplit() {
        let layout = Layout::Leaf(0).split_v(0, 1);
        match &layout {
            Layout::VSplit { top, bottom, ratio } => {
                assert!(matches!(**top, Layout::Leaf(0)));
                assert!(matches!(**bottom, Layout::Leaf(1)));
                assert!(approx_eq(*ratio, 0.5));
            }
            _ => panic!("expected VSplit"),
        }
    }

    #[test]
    fn split_non_matching_id_unchanged() {
        let layout = Layout::Leaf(0).split_h(99, 1);
        assert!(matches!(layout, Layout::Leaf(0)));
    }

    #[test]
    fn contains_present_and_absent() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        assert!(layout.contains(0));
        assert!(layout.contains(1));
        assert!(!layout.contains(99));
    }

    #[test]
    fn remove_leaf_collapses_parent() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let result = layout.remove(0).unwrap();
        assert!(matches!(result, Layout::Leaf(1)));
    }

    #[test]
    fn remove_nonexistent_returns_unchanged() {
        let layout = Layout::Leaf(0);
        let result = layout.remove(99);
        assert!(matches!(result, Some(Layout::Leaf(0))));
    }

    #[test]
    fn remove_only_leaf_returns_none() {
        let layout = Layout::Leaf(0);
        assert!(layout.remove(0).is_none());
    }

    #[test]
    fn nudge_ratio_hsplit() {
        let mut layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        layout.nudge_ratio_for(0, 0.1, 0.0);
        match &layout {
            Layout::HSplit { ratio, .. } => assert!(approx_eq(*ratio, 0.6)),
            _ => panic!("expected HSplit"),
        }
    }

    #[test]
    fn nudge_ratio_clamps_low() {
        let mut layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.15,
        };
        layout.nudge_ratio_for(0, -0.1, 0.0);
        match &layout {
            Layout::HSplit { ratio, .. } => assert!(approx_eq(*ratio, 0.1)),
            _ => panic!("expected HSplit"),
        }
    }

    #[test]
    fn nudge_ratio_clamps_high() {
        let mut layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.85,
        };
        layout.nudge_ratio_for(0, 0.1, 0.0);
        match &layout {
            Layout::HSplit { ratio, .. } => assert!(approx_eq(*ratio, 0.9)),
            _ => panic!("expected HSplit"),
        }
    }
}
