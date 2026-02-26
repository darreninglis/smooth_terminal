pub mod layout;

use anyhow::Result;
use layout::{Layout, Rect};

pub enum Direction { Left, Right, Up, Down }

use crate::terminal::Terminal;

pub struct Pane {
    pub id: usize,
    pub terminal: Terminal,
}

impl Pane {
    pub fn new(id: usize, cols: usize, rows: usize) -> Result<Self> {
        let terminal = Terminal::new(cols, rows)?;
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
    pub fn new(cols: usize, rows: usize) -> Result<Self> {
        let pane = Pane::new(0, cols, rows)?;
        let layout = Layout::Leaf(0);
        Ok(Self {
            panes: vec![pane],
            layout,
            focused_id: 0,
            next_id: 1,
        })
    }

    pub fn focused_pane(&self) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == self.focused_id)
    }

    pub fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == self.focused_id)
    }

    /// Split focused pane side by side (left | right)
    pub fn split_horizontal(&mut self, cell_w: f32, cell_h: f32, rect: Rect) -> Result<()> {
        let focused = self.focused_id;
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

        let pane = Pane::new(new_id, cols, rows)?;
        self.panes.push(pane);

        let layout = std::mem::replace(&mut self.layout, Layout::Leaf(0));
        self.layout = layout.split_h(focused, new_id);
        self.focused_id = new_id;
        Ok(())
    }

    /// Split focused pane top/bottom
    pub fn split_vertical(&mut self, cell_w: f32, cell_h: f32, rect: Rect) -> Result<()> {
        let focused = self.focused_id;
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

        let pane = Pane::new(new_id, cols, rows)?;
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
