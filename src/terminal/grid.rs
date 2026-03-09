use super::cell::{Cell, CellAttributes};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Vec<Cell>>,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    pub scrollback: VecDeque<Vec<Cell>>,
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
            scrollback: VecDeque::new(),
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
                new_cells[r][c] = self.cells[r][c];
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

    #[allow(dead_code)]
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
                self.scrollback.push_back(row);
                if self.scrollback.len() > self.scrollback_limit {
                    self.scrollback.pop_front();
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

    #[allow(dead_code)]
    pub fn total_rows(&self) -> usize {
        self.scrollback.len() + self.rows
    }

    /// Return the index (in absolute row space) of the last row that contains
    /// any non-default content, plus the last non-empty column on that row.
    /// Returns `None` if the entire grid (including scrollback) is empty.
    pub fn last_content_position(&self) -> Option<(usize, usize)> {
        let sb_len = self.scrollback.len();
        // Search visible rows bottom-up
        for r in (0..self.rows).rev() {
            if let Some(col) = self.last_nonempty_col(&self.cells[r]) {
                return Some((sb_len + r, col));
            }
        }
        // Search scrollback bottom-up
        for r in (0..sb_len).rev() {
            if let Some(col) = self.last_nonempty_col(&self.scrollback[r]) {
                return Some((r, col));
            }
        }
        None
    }

    fn last_nonempty_col(&self, row: &[Cell]) -> Option<usize> {
        for c in (0..row.len()).rev() {
            if row[c].ch != '\0' && row[c].ch != ' ' {
                return Some(c);
            }
        }
        None
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

    /// Return the start column of user input on the cursor row by detecting
    /// common shell prompt endings (`% `, `$ `, `> `, `# `).
    /// Returns `None` if no prompt pattern is found (falls back to col 0).
    pub fn prompt_end_col(&self) -> Option<usize> {
        self.prompt_end_col_for_row(self.cursor_row)
    }

    /// Return the range of user input around the cursor, spanning multiple lines
    /// if needed. Scans upward from the cursor row to find the prompt marker,
    /// then selects from prompt end to the last non-empty column on the cursor row.
    pub fn cursor_line_input_range(&self) -> Option<(usize, usize, usize, usize)> {
        let slen = self.scrollback.len();
        // Find the prompt row by scanning upward from cursor_row
        let mut prompt_row = self.cursor_row;
        let mut prompt_col = 0usize;
        for r in (0..=self.cursor_row).rev() {
            if let Some(col) = self.prompt_end_col_for_row(r) {
                prompt_row = r;
                prompt_col = col;
                break;
            }
            // If we hit an empty row, stop scanning — input doesn't span past gaps
            if self.last_nonempty_col(&self.cells[r]).is_none() && r < self.cursor_row {
                break;
            }
        }
        let end_col = self.last_nonempty_col(&self.cells[self.cursor_row])?;
        if prompt_row == self.cursor_row && prompt_col > end_col {
            return None;
        }
        Some((slen + prompt_row, prompt_col, slen + self.cursor_row, end_col))
    }

    /// Detect prompt end column on a specific visible row.
    fn prompt_end_col_for_row(&self, row_idx: usize) -> Option<usize> {
        let row = &self.cells[row_idx];
        let limit = if row_idx == self.cursor_row {
            self.cursor_col.min(row.len())
        } else {
            row.len().saturating_sub(1)
        };
        for c in (1..=limit).rev() {
            let prev_ch = row[c - 1].ch;
            let cur_ch = row[c].ch;
            if (prev_ch == '%' || prev_ch == '$' || prev_ch == '>' || prev_ch == '#')
                && cur_ch == ' '
            {
                let start = c + 1;
                return if start <= limit { Some(start) } else { None };
            }
        }
        None
    }

    /// Return the range spanning all non-empty content (scrollback + visible).
    /// Used as a fallback for SelectAll in TUI apps where prompt detection fails.
    pub fn full_content_range(&self) -> Option<(usize, usize, usize, usize)> {
        let slen = self.scrollback.len();
        // Find first non-empty row
        let mut first_row: Option<usize> = None;
        let mut last_row: Option<usize> = None;
        for abs_row in 0..(slen + self.rows) {
            let row: &[Cell] = if abs_row < slen {
                &self.scrollback[abs_row]
            } else {
                &self.cells[abs_row - slen]
            };
            if self.last_nonempty_col(row).is_some() {
                if first_row.is_none() {
                    first_row = Some(abs_row);
                }
                last_row = Some(abs_row);
            }
        }
        let first = first_row?;
        let last = last_row?;
        let last_cells: &[Cell] = if last < slen {
            &self.scrollback[last]
        } else {
            &self.cells[last - slen]
        };
        let end_col = self.last_nonempty_col(last_cells)?;
        Some((first, 0, last, end_col))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_dimensions_and_cursor() {
        let g = TerminalGrid::new(80, 24);
        assert_eq!(g.cols, 80);
        assert_eq!(g.rows, 24);
        assert_eq!(g.cursor_col, 0);
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.scroll_bottom, 23);
        assert_eq!(g.scroll_top, 0);
    }

    #[test]
    fn resize_preserves_content() {
        let mut g = TerminalGrid::new(10, 5);
        g.set_cell(0, 0, 'A');
        g.set_cell(9, 4, 'Z');
        g.resize(20, 10);
        assert_eq!(g.cells[0][0].ch, 'A');
        assert_eq!(g.cells[4][9].ch, 'Z');
        assert_eq!(g.cols, 20);
        assert_eq!(g.rows, 10);
    }

    #[test]
    fn resize_clamps_cursor() {
        let mut g = TerminalGrid::new(80, 24);
        g.cursor_col = 79;
        g.cursor_row = 23;
        g.resize(40, 10);
        assert_eq!(g.cursor_col, 39);
        assert_eq!(g.cursor_row, 9);
    }

    #[test]
    fn resize_resets_scroll_region() {
        let mut g = TerminalGrid::new(80, 24);
        g.scroll_top = 5;
        g.scroll_bottom = 10;
        g.resize(80, 30);
        assert_eq!(g.scroll_top, 0);
        assert_eq!(g.scroll_bottom, 29);
    }

    #[test]
    fn resize_noop_when_same_size() {
        let mut g = TerminalGrid::new(80, 24);
        let gen_before = g.generation;
        g.resize(80, 24);
        assert_eq!(g.generation, gen_before);
    }

    #[test]
    fn set_cell_writes_and_increments_gen() {
        let mut g = TerminalGrid::new(10, 5);
        let gen = g.generation;
        g.set_cell(3, 2, 'X');
        assert_eq!(g.cells[2][3].ch, 'X');
        assert_eq!(g.generation, gen + 1);
    }

    #[test]
    fn set_cell_oob_is_noop() {
        let mut g = TerminalGrid::new(10, 5);
        let gen = g.generation;
        g.set_cell(100, 100, 'X');
        assert_eq!(g.generation, gen);
    }

    #[test]
    fn clear_line_empties_row() {
        let mut g = TerminalGrid::new(10, 5);
        g.set_cell(0, 0, 'A');
        g.set_cell(5, 0, 'B');
        g.clear_line(0);
        assert_eq!(g.cells[0][0].ch, '\0');
        assert_eq!(g.cells[0][5].ch, '\0');
    }

    #[test]
    fn clear_line_range_partial() {
        let mut g = TerminalGrid::new(10, 5);
        for i in 0..10 { g.set_cell(i, 0, 'X'); }
        g.clear_line_range(0, 3, 7);
        assert_eq!(g.cells[0][2].ch, 'X');
        assert_eq!(g.cells[0][3].ch, '\0');
        assert_eq!(g.cells[0][6].ch, '\0');
        assert_eq!(g.cells[0][7].ch, 'X');
    }

    #[test]
    fn clear_screen_resets_cursor() {
        let mut g = TerminalGrid::new(10, 5);
        g.set_cell(5, 3, 'A');
        g.cursor_col = 5;
        g.cursor_row = 3;
        g.pending_wrap = true;
        g.clear_screen();
        assert_eq!(g.cursor_col, 0);
        assert_eq!(g.cursor_row, 0);
        assert!(!g.pending_wrap);
        assert_eq!(g.cells[3][5].ch, '\0');
    }

    #[test]
    fn scroll_up_region_shifts_rows() {
        let mut g = TerminalGrid::new(10, 5);
        for r in 0..5 {
            g.set_cell(0, r, char::from(b'A' + r as u8));
        }
        g.scroll_up_region(1);
        assert_eq!(g.cells[0][0].ch, 'B');
        assert_eq!(g.cells[3][0].ch, 'E');
        assert_eq!(g.cells[4][0].ch, '\0'); // cleared
    }

    #[test]
    fn scroll_up_region_grows_scrollback() {
        let mut g = TerminalGrid::new(10, 5);
        g.set_cell(0, 0, 'A');
        g.scroll_up_region(1);
        assert_eq!(g.scrollback.len(), 1);
        assert_eq!(g.scrollback[0][0].ch, 'A');
    }

    #[test]
    fn scroll_up_region_scrollback_limit() {
        let mut g = TerminalGrid::new(10, 3);
        g.scrollback_limit = 2;
        for i in 0..5 {
            g.set_cell(0, 0, char::from(b'A' + i as u8));
            g.scroll_up_region(1);
        }
        assert_eq!(g.scrollback.len(), 2);
    }

    #[test]
    fn scroll_down_region_shifts_rows_down() {
        let mut g = TerminalGrid::new(10, 5);
        for r in 0..5 {
            g.set_cell(0, r, char::from(b'A' + r as u8));
        }
        g.scroll_down_region(1);
        assert_eq!(g.cells[0][0].ch, '\0'); // cleared top
        assert_eq!(g.cells[1][0].ch, 'A');
        assert_eq!(g.cells[4][0].ch, 'D');
    }

    #[test]
    fn newline_at_scroll_bottom_scrolls() {
        let mut g = TerminalGrid::new(10, 3);
        g.set_cell(0, 0, 'A');
        g.cursor_row = 2; // scroll_bottom
        g.newline();
        assert_eq!(g.cursor_row, 2);
        assert_eq!(g.scrollback.len(), 1);
    }

    #[test]
    fn newline_not_at_bottom_increments_row() {
        let mut g = TerminalGrid::new(10, 5);
        g.cursor_row = 1;
        g.newline();
        assert_eq!(g.cursor_row, 2);
    }

    #[test]
    fn newline_clears_pending_wrap() {
        let mut g = TerminalGrid::new(10, 5);
        g.pending_wrap = true;
        g.newline();
        assert!(!g.pending_wrap);
    }

    #[test]
    fn carriage_return_resets_col() {
        let mut g = TerminalGrid::new(10, 5);
        g.cursor_col = 5;
        g.pending_wrap = true;
        g.carriage_return();
        assert_eq!(g.cursor_col, 0);
        assert!(!g.pending_wrap);
    }

    #[test]
    fn advance_cursor_normal() {
        let mut g = TerminalGrid::new(10, 5);
        g.cursor_col = 3;
        g.advance_cursor();
        assert_eq!(g.cursor_col, 4);
        assert!(!g.pending_wrap);
    }

    #[test]
    fn advance_cursor_sets_pending_wrap_at_edge() {
        let mut g = TerminalGrid::new(10, 5);
        g.cursor_col = 9;
        g.advance_cursor();
        assert!(g.pending_wrap);
    }

    #[test]
    fn advance_cursor_by_width_2() {
        let mut g = TerminalGrid::new(10, 5);
        g.cursor_col = 3;
        g.advance_cursor_by_width(2);
        assert_eq!(g.cursor_col, 5);
    }

    #[test]
    fn total_rows_includes_scrollback() {
        let mut g = TerminalGrid::new(10, 5);
        assert_eq!(g.total_rows(), 5);
        g.set_cell(0, 0, 'A');
        g.scroll_up_region(1);
        assert_eq!(g.total_rows(), 6);
    }

    #[test]
    fn detect_reverse_cursor_single_cell() {
        let mut g = TerminalGrid::new(10, 5);
        g.cells[2][3].attrs.reverse = true;
        g.detect_reverse_cursor();
        assert_eq!(g.reverse_cursor, Some((2, 3)));
    }

    #[test]
    fn detect_reverse_cursor_too_many_skipped() {
        let mut g = TerminalGrid::new(10, 5);
        // 4 reverse cells in a row — should be skipped (status bar)
        for c in 0..4 { g.cells[2][c].attrs.reverse = true; }
        g.detect_reverse_cursor();
        assert_eq!(g.reverse_cursor, None);
    }

    #[test]
    fn detect_reverse_cursor_none() {
        let mut g = TerminalGrid::new(10, 5);
        g.detect_reverse_cursor();
        assert_eq!(g.reverse_cursor, None);
    }

    #[test]
    fn extract_selection_single_line() {
        let mut g = TerminalGrid::new(10, 3);
        for (i, ch) in "Hello".chars().enumerate() {
            g.set_cell(i, 0, ch);
        }
        let slen = g.scrollback.len();
        let text = g.extract_selection((slen, 0), (slen, 4));
        assert_eq!(text, "Hello");
    }

    #[test]
    fn extract_selection_multi_line() {
        let mut g = TerminalGrid::new(10, 3);
        for (i, ch) in "AAAA".chars().enumerate() { g.set_cell(i, 0, ch); }
        for (i, ch) in "BBBB".chars().enumerate() { g.set_cell(i, 1, ch); }
        let slen = g.scrollback.len();
        let text = g.extract_selection((slen, 0), (slen + 1, 3));
        assert_eq!(text, "AAAA\nBBBB");
    }

    #[test]
    fn extract_selection_null_to_space() {
        let mut g = TerminalGrid::new(10, 3);
        g.set_cell(0, 0, 'A');
        // cells[0][1] is default '\0'
        g.set_cell(2, 0, 'B');
        let slen = g.scrollback.len();
        let text = g.extract_selection((slen, 0), (slen, 2));
        assert_eq!(text, "A B");
    }

    #[test]
    fn extract_selection_across_scrollback() {
        let mut g = TerminalGrid::new(10, 3);
        for (i, ch) in "SCROLL".chars().enumerate() { g.set_cell(i, 0, ch); }
        g.scroll_up_region(1); // row 0 → scrollback
        for (i, ch) in "VISIBLE".chars().enumerate() { g.set_cell(i, 0, ch); }
        // scrollback[0] = "SCROLL", visible[0] = "VISIBLE"
        let text = g.extract_selection((0, 0), (1, 6));
        assert_eq!(text, "SCROLL\nVISIBLE");
    }

    #[test]
    fn extract_selection_trims_trailing_spaces() {
        let mut g = TerminalGrid::new(10, 3);
        g.set_cell(0, 0, 'A');
        g.set_cell(1, 0, ' ');
        g.set_cell(2, 0, ' ');
        let slen = g.scrollback.len();
        let text = g.extract_selection((slen, 0), (slen, 9));
        assert_eq!(text, "A");
    }

    #[test]
    fn last_content_position_empty_grid() {
        let g = TerminalGrid::new(10, 5);
        assert_eq!(g.last_content_position(), None);
    }

    #[test]
    fn last_content_position_visible_only() {
        let mut g = TerminalGrid::new(10, 5);
        g.set_cell(0, 0, 'A');
        g.set_cell(3, 2, 'Z');
        // Last content is row 2, col 3
        assert_eq!(g.last_content_position(), Some((2, 3)));
    }

    #[test]
    fn last_content_position_with_scrollback() {
        let mut g = TerminalGrid::new(10, 3);
        for (i, ch) in "Hello".chars().enumerate() { g.set_cell(i, 0, ch); }
        g.scroll_up_region(1); // row 0 → scrollback
        // visible rows are now empty, scrollback has "Hello"
        assert_eq!(g.last_content_position(), Some((0, 4)));
    }

    #[test]
    fn last_content_position_ignores_spaces() {
        let mut g = TerminalGrid::new(10, 5);
        g.set_cell(0, 0, 'A');
        g.cells[0][1].ch = ' ';
        // Only 'A' at col 0 counts
        assert_eq!(g.last_content_position(), Some((0, 0)));
    }

    #[test]
    fn prompt_end_col_zsh_percent() {
        // Simulate "user@host ~ % ls -la"
        let mut g = TerminalGrid::new(30, 5);
        let prompt = "user@host ~ % ls -la";
        for (i, ch) in prompt.chars().enumerate() {
            g.set_cell(i, 0, ch);
        }
        g.cursor_row = 0;
        g.cursor_col = prompt.len() - 1;
        // "% " at cols 12-13, so input starts at col 14
        assert_eq!(g.prompt_end_col(), Some(14));
    }

    #[test]
    fn prompt_end_col_bash_dollar() {
        let mut g = TerminalGrid::new(30, 5);
        let prompt = "user$ echo hi";
        for (i, ch) in prompt.chars().enumerate() {
            g.set_cell(i, 0, ch);
        }
        g.cursor_row = 0;
        g.cursor_col = prompt.len() - 1;
        assert_eq!(g.prompt_end_col(), Some(6));
    }

    #[test]
    fn prompt_end_col_none_when_no_prompt() {
        let mut g = TerminalGrid::new(30, 5);
        let text = "hello world";
        for (i, ch) in text.chars().enumerate() {
            g.set_cell(i, 0, ch);
        }
        g.cursor_row = 0;
        g.cursor_col = text.len() - 1;
        assert_eq!(g.prompt_end_col(), None);
    }

    #[test]
    fn cursor_line_input_range_selects_user_input() {
        let mut g = TerminalGrid::new(30, 5);
        let prompt = "~ % ls -la";
        for (i, ch) in prompt.chars().enumerate() {
            g.set_cell(i, 0, ch);
        }
        g.cursor_row = 0;
        g.cursor_col = prompt.len() - 1;
        let range = g.cursor_line_input_range();
        // "% " at cols 2-3, input starts col 4, last content col 9
        assert_eq!(range, Some((0, 4, 0, 9)));
    }

    #[test]
    fn cursor_line_input_range_empty_input() {
        let mut g = TerminalGrid::new(30, 5);
        // Just the prompt, no user input: "~ % "
        let prompt = "~ % ";
        for (i, ch) in prompt.chars().enumerate() {
            g.set_cell(i, 0, ch);
        }
        g.cursor_row = 0;
        g.cursor_col = 4; // cursor right after prompt
        // start_col=4 but last_nonempty is col 1 (%)... actually spaces don't count
        // last_nonempty_col would be col 1 (%), start_col=4 > 1, so None
        assert_eq!(g.cursor_line_input_range(), None);
    }

    #[test]
    fn cursor_line_input_range_multiline() {
        // Simulate: "~ % echo hello \\\n  world"
        // Row 0: prompt + first line, Row 1: continuation
        let mut g = TerminalGrid::new(30, 5);
        let line0 = "~ % echo hello \\";
        for (i, ch) in line0.chars().enumerate() {
            g.set_cell(i, 0, ch);
        }
        let line1 = "  world";
        for (i, ch) in line1.chars().enumerate() {
            g.set_cell(i, 1, ch);
        }
        g.cursor_row = 1;
        g.cursor_col = line1.len() - 1;
        let range = g.cursor_line_input_range();
        // Should start at row 0 col 4 (after "% "), end at row 1 col 6 ("world" ends at 6)
        assert_eq!(range, Some((0, 4, 1, 6)));
    }
}
