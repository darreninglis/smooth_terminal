use super::cell::{Cell, CellAttributes};

#[derive(Debug, Clone)]
pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Vec<Cell>>,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    pub scrollback: Vec<Vec<Cell>>,
    pub scrollback_limit: usize,
    pub current_attrs: CellAttributes,
    pub title: String,
    /// Pending line wrap: next char goes to start of next line
    pub pending_wrap: bool,
    /// Incremented on every visible cell change.  The renderer compares this
    /// against a cached value to decide whether to rebuild SpanBuffers.
    pub generation: u64,
    /// Whether bracketed paste mode (DEC mode 2004) is active.
    pub bracketed_paste: bool,
    /// Whether the cursor is visible (DECTCEM / DEC mode 25). TUI apps hide
    /// the terminal cursor and draw their own; we must respect this so our
    /// animated cursor doesn't render in the wrong place.
    pub cursor_visible: bool,
    /// Reverse-video cursor position detected by scanning the grid.
    /// TUI apps like Claude Code hide the terminal cursor permanently and
    /// draw their own cursor as a single reverse-video (`ESC[7m`) character.
    /// Each frame we scan the visible cells to find that character and report
    /// its position so the GPU-animated cursor can track it.
    pub reverse_cursor: Option<(usize, usize)>,
}

impl TerminalGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self {
            cols,
            rows,
            cells,
            cursor_col: 0,
            cursor_row: 0,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            scrollback: Vec::new(),
            scrollback_limit: 10000,
            current_attrs: CellAttributes::default(),
            title: String::new(),
            pending_wrap: false,
            generation: 0,
            bracketed_paste: false,
            cursor_visible: true,
            reverse_cursor: None,
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        let mut new_cells = vec![vec![Cell::default(); cols]; rows];
        let copy_rows = self.rows.min(rows);
        let copy_cols = self.cols.min(cols);
        for r in 0..copy_rows {
            for c in 0..copy_cols {
                new_cells[r][c] = self.cells[r][c].clone();
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.cells = new_cells;
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.pending_wrap = false;
    }

    pub fn set_cell(&mut self, col: usize, row: usize, ch: char) {
        if row < self.rows && col < self.cols {
            self.cells[row][col] = Cell::new(ch, self.current_attrs);
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn clear_line(&mut self, row: usize) {
        if row < self.rows {
            for c in 0..self.cols {
                self.cells[row][c] = Cell::default();
            }
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn clear_line_range(&mut self, row: usize, col_start: usize, col_end: usize) {
        if row < self.rows {
            let end = col_end.min(self.cols);
            for c in col_start..end {
                self.cells[row][c] = Cell::default();
            }
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn clear_screen(&mut self) {
        for row in 0..self.rows {
            self.clear_line(row);
        }
        self.cursor_col = 0;
        self.cursor_row = 0;
        self.pending_wrap = false;
    }

    /// Scroll up region [scroll_top..=scroll_bottom] by `count` lines
    pub fn scroll_up_region(&mut self, count: usize) {
        self.generation = self.generation.wrapping_add(1);
        let top = self.scroll_top;
        let bottom = self.scroll_bottom.min(self.rows - 1);
        if top >= bottom {
            return;
        }
        let region_height = bottom - top + 1;
        let count = count.min(region_height);

        // Move scrolled-out rows to scrollback
        for i in 0..count {
            let row_idx = top + i;
            if row_idx < self.rows {
                let row = self.cells[row_idx].clone();
                self.scrollback.push(row);
                if self.scrollback.len() > self.scrollback_limit {
                    self.scrollback.remove(0);
                }
            }
        }

        // Shift rows up
        for r in top..(bottom + 1 - count) {
            let src = r + count;
            if src <= bottom && src < self.rows {
                self.cells[r] = self.cells[src].clone();
            }
        }
        // Clear newly exposed rows at bottom
        for r in (bottom + 1 - count)..(bottom + 1) {
            if r < self.rows {
                self.clear_line(r);
            }
        }
    }

    /// Scroll down region [scroll_top..=scroll_bottom] by `count` lines
    pub fn scroll_down_region(&mut self, count: usize) {
        self.generation = self.generation.wrapping_add(1);
        let top = self.scroll_top;
        let bottom = self.scroll_bottom.min(self.rows - 1);
        if top >= bottom {
            return;
        }
        let region_height = bottom - top + 1;
        let count = count.min(region_height);

        for r in (top..bottom + 1).rev() {
            let dst = r;
            let src = r.wrapping_sub(count);
            if src >= top && src <= bottom && dst < self.rows {
                self.cells[dst] = self.cells[src].clone();
            } else if dst >= top && dst < top + count && dst < self.rows {
                self.clear_line(dst);
            }
        }
        // Clear top rows
        for r in top..(top + count).min(bottom + 1) {
            if r < self.rows {
                self.clear_line(r);
            }
        }
    }

    pub fn newline(&mut self) {
        self.pending_wrap = false;
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up_region(1);
        } else if self.cursor_row < self.rows - 1 {
            self.cursor_row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
        self.pending_wrap = false;
    }

    pub fn advance_cursor(&mut self) {
        self.advance_cursor_by_width(1);
    }

    /// Advance the cursor by `width` columns (1 for normal chars, 2 for wide chars).
    pub fn advance_cursor_by_width(&mut self, width: usize) {
        let next_col = self.cursor_col + width;
        if next_col < self.cols {
            self.cursor_col = next_col;
            self.pending_wrap = false;
        } else {
            // At or past right edge — set pending wrap flag
            self.pending_wrap = true;
        }
    }

    pub fn total_rows(&self) -> usize {
        self.scrollback.len() + self.rows
    }

    /// Scan visible cells for a TUI-drawn cursor (single reverse-video cell).
    /// Claude Code / Ink draws its cursor as one character with `ESC[7m`
    /// (reverse attribute).  We look for the *last* row that contains exactly
    /// one isolated reverse-video cell — that is the text-input cursor.
    /// Updates `self.reverse_cursor` in place.
    pub fn detect_reverse_cursor(&mut self) {
        // Scan bottom-up: the input area is usually in the lower portion.
        for row in (0..self.rows).rev() {
            let mut rev_col: Option<usize> = None;
            let mut rev_count: usize = 0;
            for col in 0..self.cols {
                if self.cells[row][col].attrs.reverse {
                    if rev_count == 0 {
                        rev_col = Some(col);
                    }
                    rev_count += 1;
                    // More than a handful of reverse cells means this is a
                    // header / status bar, not a cursor.  Skip the row.
                    if rev_count > 3 {
                        break;
                    }
                }
            }
            if rev_count >= 1 && rev_count <= 3 {
                self.reverse_cursor = rev_col.map(|c| (row, c));
                return;
            }
        }
        self.reverse_cursor = None;
    }

    /// Extract text for a selection range.
    /// Coordinates use absolute row indexing:
    ///   abs_row 0..scrollback.len()         → scrollback rows
    ///   abs_row scrollback.len()..total_rows → visible rows
    /// Returns the selected text with lines joined by newlines.
    pub fn extract_selection(
        &self,
        start: (usize, usize), // (abs_row, col) — normalized (start <= end)
        end: (usize, usize),
    ) -> String {
        let slen = self.scrollback.len();
        let mut lines: Vec<String> = Vec::new();
        for abs_row in start.0..=end.0 {
            let row: &[Cell] = if abs_row < slen {
                &self.scrollback[abs_row]
            } else {
                let vr = abs_row - slen;
                if vr < self.rows { &self.cells[vr] } else { continue }
            };
            let col_start = if abs_row == start.0 { start.1 } else { 0 };
            let col_end = if abs_row == end.0 { end.1 + 1 } else { row.len() };
            let col_end = col_end.min(row.len());
            let mut line = String::new();
            for col in col_start..col_end {
                if col < row.len() {
                    let ch = row[col].ch;
                    if ch != '\0' { line.push(ch); }
                    else { line.push(' '); }
                }
            }
            // Trim trailing spaces from each line
            let trimmed = line.trim_end().to_string();
            lines.push(trimmed);
        }
        lines.join("\n")
    }
}
