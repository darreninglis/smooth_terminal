pub mod layout;

use anyhow::Result;
use layout::{Layout, Rect};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { Left, Right, Up, Down }

use crate::terminal::Terminal;

pub struct Pane {
    pub id: usize,
    pub terminal: Terminal,
}

impl Pane {
    pub fn new(id: usize, cols: usize, rows: usize, cwd: Option<&Path>) -> Result<Self> {
        let terminal = Terminal::new(cols, rows, cwd)?;
        Ok(Self { id, terminal })
    }
}

pub struct PaneTree {
    pub panes: Vec<Pane>,
    pub layout: Layout,
    pub focused_id: usize,
    next_id: usize,
}

impl PaneTree {
    pub fn new(cols: usize, rows: usize, cwd: Option<&Path>) -> Result<Self> {
        let pane = Pane::new(0, cols, rows, cwd)?;
        let layout = Layout::Leaf(0);
        Ok(Self {
            panes: vec![pane],
            layout,
            focused_id: 0,
            next_id: 1,
        })
    }

    pub fn pane_by_id(&self, id: usize) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == id)
    }

    pub fn pane_by_id_mut(&mut self, id: usize) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == id)
    }

    pub fn focused_pane(&self) -> Option<&Pane> {
        self.pane_by_id(self.focused_id)
    }

    pub fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        self.pane_by_id_mut(self.focused_id)
    }

    pub fn focused_cwd(&self) -> Option<PathBuf> {
        self.focused_pane().and_then(|p| p.terminal.pty.get_cwd())
    }

    /// Split focused pane side by side (left | right)
    pub fn split_horizontal(&mut self, cell_w: f32, cell_h: f32, rect: Rect) -> Result<()> {
        let focused = self.focused_id;
        let cwd = self.focused_cwd();
        let new_id = self.next_id;
        self.next_id += 1;

        // Compute the focused pane's rect
        let rects = self.layout.compute_rects(rect);
        let focused_rect = rects.iter()
            .find(|(id, _)| *id == focused)
            .map(|(_, r)| *r)
            .unwrap_or(rect);

        let cols = ((focused_rect.width / 2.0) / cell_w).floor() as usize;
        let rows = (focused_rect.height / cell_h).floor() as usize;
        let cols = cols.max(1);
        let rows = rows.max(1);

        let pane = Pane::new(new_id, cols, rows, cwd.as_deref())?;
        self.panes.push(pane);

        let layout = std::mem::replace(&mut self.layout, Layout::Leaf(0));
        self.layout = layout.split_h(focused, new_id);
        self.focused_id = new_id;
        Ok(())
    }

    /// Split focused pane top/bottom
    pub fn split_vertical(&mut self, cell_w: f32, cell_h: f32, rect: Rect) -> Result<()> {
        let focused = self.focused_id;
        let cwd = self.focused_cwd();
        let new_id = self.next_id;
        self.next_id += 1;

        let rects = self.layout.compute_rects(rect);
        let focused_rect = rects.iter()
            .find(|(id, _)| *id == focused)
            .map(|(_, r)| *r)
            .unwrap_or(rect);

        let cols = (focused_rect.width / cell_w).floor() as usize;
        let rows = ((focused_rect.height / 2.0) / cell_h).floor() as usize;
        let cols = cols.max(1);
        let rows = rows.max(1);

        let pane = Pane::new(new_id, cols, rows, cwd.as_deref())?;
        self.panes.push(pane);

        let layout = std::mem::replace(&mut self.layout, Layout::Leaf(0));
        self.layout = layout.split_v(focused, new_id);
        self.focused_id = new_id;
        Ok(())
    }

    pub fn close_focused(&mut self) {
        self.close_pane(self.focused_id);
    }

    /// Close a specific pane by ID.
    pub fn close_pane(&mut self, id: usize) {
        self.panes.retain(|p| p.id != id);

        let layout = std::mem::replace(&mut self.layout, Layout::Leaf(0));
        self.layout = layout.remove(id).unwrap_or(Layout::Leaf(0));

        // If we just closed the focused pane, move focus to the first remaining one.
        if self.focused_id == id {
            if let Some(first) = self.panes.first() {
                self.focused_id = first.id;
            }
        }
    }

    /// IDs of panes whose shell process has exited.
    pub fn dead_pane_ids(&self) -> Vec<usize> {
        self.panes.iter()
            .filter(|p| p.terminal.is_pty_dead())
            .map(|p| p.id)
            .collect()
    }

    pub fn focus_next(&mut self) {
        let ids = self.layout.pane_ids();
        if ids.is_empty() { return; }
        let pos = ids.iter().position(|&id| id == self.focused_id).unwrap_or(0);
        self.focused_id = ids[(pos + 1) % ids.len()];
    }

    pub fn focus_prev(&mut self) {
        let ids = self.layout.pane_ids();
        if ids.is_empty() { return; }
        let pos = ids.iter().position(|&id| id == self.focused_id).unwrap_or(0);
        self.focused_id = ids[(pos + ids.len() - 1) % ids.len()];
    }

    /// Focus the nearest pane in the given direction from the currently focused pane.
    /// `layout_rects` must be pre-computed from the current content rect.
    pub fn focus_direction(&mut self, layout_rects: &[(usize, Rect)], dir: Direction) {
        let focused_rect = match layout_rects.iter().find(|(id, _)| *id == self.focused_id) {
            Some((_, r)) => *r,
            None => return,
        };

        let mut best_id: Option<usize> = None;
        let mut best_dist = f32::MAX;

        let fcx = focused_rect.x + focused_rect.width  * 0.5;
        let fcy = focused_rect.y + focused_rect.height * 0.5;

        for (id, rect) in layout_rects {
            if *id == self.focused_id {
                continue;
            }
            // A candidate qualifies if its opposing edge is flush with or beyond
            // the focused pane's leading edge in the chosen direction.
            let qualifies = match dir {
                Direction::Left  => rect.x + rect.width  <= focused_rect.x + 1.0,
                Direction::Right => rect.x               >= focused_rect.x + focused_rect.width  - 1.0,
                Direction::Up    => rect.y + rect.height <= focused_rect.y + 1.0,
                Direction::Down  => rect.y               >= focused_rect.y + focused_rect.height - 1.0,
            };
            if !qualifies {
                continue;
            }
            let cx = rect.x + rect.width  * 0.5;
            let cy = rect.y + rect.height * 0.5;
            let dist = (cx - fcx).powi(2) + (cy - fcy).powi(2);
            if dist < best_dist {
                best_dist = dist;
                best_id = Some(*id);
            }
        }

        if let Some(id) = best_id {
            self.focused_id = id;
        }
    }

    /// Nudge the split ratio containing the focused pane in the given direction.
    /// Left/Right adjusts horizontal splits; Up/Down adjusts vertical splits.
    pub fn resize_focused(&mut self, dir: Direction) {
        let delta = 0.05_f32;
        let (h_delta, v_delta) = match dir {
            Direction::Left  => (-delta, 0.0),
            Direction::Right => ( delta, 0.0),
            Direction::Up    => (0.0, -delta),
            Direction::Down  => (0.0,  delta),
        };
        self.layout.nudge_ratio_for(self.focused_id, h_delta, v_delta);
    }

    pub fn drain_all_pty_output(&mut self) {
        for pane in &mut self.panes {
            pane.terminal.drain_pty_output();
        }
    }

    pub fn resize_panes(&mut self, layout_rects: &[(usize, Rect)], cell_w: f32, cell_h: f32) {
        for (id, rect) in layout_rects {
            if let Some(pane) = self.panes.iter_mut().find(|p| p.id == *id) {
                let cols = (rect.width / cell_w).floor() as usize;
                let rows = (rect.height / cell_h).floor() as usize;
                let cols = cols.max(1);
                let rows = rows.max(1);
                let _ = pane.terminal.resize(cols, rows);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a PaneTree with real PTYs for testing. Panes spawn /usr/bin/true
    /// which exits immediately, so they're lightweight.
    fn test_tree(ids: &[usize], layout: Layout, focused: usize) -> PaneTree {
        let panes: Vec<Pane> = ids.iter().map(|&id| {
            Pane::new(id, 80, 24, None).expect("spawn pane for test")
        }).collect();
        let next_id = ids.iter().max().unwrap_or(&0) + 1;
        PaneTree { panes, layout, focused_id: focused, next_id }
    }

    // ── focus_next / focus_prev ──

    #[test]
    fn focus_next_cycles_forward() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.focus_next();
        assert_eq!(tree.focused_id, 1);
        tree.focus_next();
        assert_eq!(tree.focused_id, 0); // wraps
    }

    #[test]
    fn focus_prev_cycles_backward() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.focus_prev();
        assert_eq!(tree.focused_id, 1); // wraps backward
        tree.focus_prev();
        assert_eq!(tree.focused_id, 0);
    }

    #[test]
    fn focus_next_three_panes() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::HSplit {
                left: Box::new(Layout::Leaf(1)),
                right: Box::new(Layout::Leaf(2)),
                ratio: 0.5,
            }),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1, 2], layout, 0);
        tree.focus_next();
        assert_eq!(tree.focused_id, 1);
        tree.focus_next();
        assert_eq!(tree.focused_id, 2);
        tree.focus_next();
        assert_eq!(tree.focused_id, 0);
    }

    // ── focus_direction ──

    fn two_h_rects() -> Vec<(usize, Rect)> {
        vec![
            (0, Rect::new(0.0, 0.0, 400.0, 600.0)),
            (1, Rect::new(400.0, 0.0, 400.0, 600.0)),
        ]
    }

    #[test]
    fn focus_direction_right() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.focus_direction(&two_h_rects(), Direction::Right);
        assert_eq!(tree.focused_id, 1);
    }

    #[test]
    fn focus_direction_left() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 1);
        tree.focus_direction(&two_h_rects(), Direction::Left);
        assert_eq!(tree.focused_id, 0);
    }

    #[test]
    fn focus_direction_no_candidate_stays() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.focus_direction(&two_h_rects(), Direction::Left);
        assert_eq!(tree.focused_id, 0); // no pane to the left
    }

    #[test]
    fn focus_direction_up_down() {
        let rects = vec![
            (0, Rect::new(0.0, 0.0, 800.0, 300.0)),
            (1, Rect::new(0.0, 300.0, 800.0, 300.0)),
        ];
        let layout = Layout::VSplit {
            top: Box::new(Layout::Leaf(0)),
            bottom: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.focus_direction(&rects, Direction::Down);
        assert_eq!(tree.focused_id, 1);
        tree.focus_direction(&rects, Direction::Up);
        assert_eq!(tree.focused_id, 0);
    }

    #[test]
    fn focus_direction_picks_nearest() {
        // Three panes in a row: [0][1][2], focused on 0, Right should pick 1 (nearest)
        let rects = vec![
            (0, Rect::new(0.0, 0.0, 200.0, 600.0)),
            (1, Rect::new(200.0, 0.0, 200.0, 600.0)),
            (2, Rect::new(400.0, 0.0, 200.0, 600.0)),
        ];
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::HSplit {
                left: Box::new(Layout::Leaf(1)),
                right: Box::new(Layout::Leaf(2)),
                ratio: 0.5,
            }),
            ratio: 0.33,
        };
        let mut tree = test_tree(&[0, 1, 2], layout, 0);
        tree.focus_direction(&rects, Direction::Right);
        assert_eq!(tree.focused_id, 1);
    }

    // ── close_pane ──

    #[test]
    fn close_focused_moves_focus_to_first() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 1);
        tree.close_focused();
        assert_eq!(tree.panes.len(), 1);
        assert_eq!(tree.focused_id, 0);
    }

    #[test]
    fn close_non_focused_keeps_focus() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.close_pane(1);
        assert_eq!(tree.panes.len(), 1);
        assert_eq!(tree.focused_id, 0);
    }

    #[test]
    fn close_pane_updates_layout() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.close_pane(1);
        assert_eq!(tree.layout.pane_ids(), vec![0]);
    }

    // ── resize_focused ──

    #[test]
    fn resize_focused_adjusts_ratio() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.resize_focused(Direction::Right);
        match &tree.layout {
            Layout::HSplit { ratio, .. } => assert!((*ratio - 0.55).abs() < 0.001),
            _ => panic!("expected HSplit"),
        }
    }

    #[test]
    fn resize_focused_left_decreases_ratio() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        tree.resize_focused(Direction::Left);
        match &tree.layout {
            Layout::HSplit { ratio, .. } => assert!((*ratio - 0.45).abs() < 0.001),
            _ => panic!("expected HSplit"),
        }
    }

    // ── split ──

    #[test]
    fn split_horizontal_adds_pane_and_focuses_new() {
        let mut tree = PaneTree::new(80, 24, None).unwrap();
        let rect = Rect::new(0.0, 0.0, 800.0, 600.0);
        tree.split_horizontal(10.0, 20.0, rect).unwrap();
        assert_eq!(tree.panes.len(), 2);
        assert_eq!(tree.focused_id, 1);
        assert_eq!(tree.layout.pane_ids().len(), 2);
    }

    #[test]
    fn split_vertical_adds_pane_and_focuses_new() {
        let mut tree = PaneTree::new(80, 24, None).unwrap();
        let rect = Rect::new(0.0, 0.0, 800.0, 600.0);
        tree.split_vertical(10.0, 20.0, rect).unwrap();
        assert_eq!(tree.panes.len(), 2);
        assert_eq!(tree.focused_id, 1);
        assert_eq!(tree.layout.pane_ids().len(), 2);
    }

    #[test]
    fn focused_pane_returns_correct_pane() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let tree = test_tree(&[0, 1], layout, 1);
        assert_eq!(tree.focused_pane().unwrap().id, 1);
    }

    #[test]
    fn focused_pane_mut_returns_correct_pane() {
        let layout = Layout::HSplit {
            left: Box::new(Layout::Leaf(0)),
            right: Box::new(Layout::Leaf(1)),
            ratio: 0.5,
        };
        let mut tree = test_tree(&[0, 1], layout, 0);
        assert_eq!(tree.focused_pane_mut().unwrap().id, 0);
    }
}
