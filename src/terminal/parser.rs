use super::cell::{Cell, CellAttributes, Color};
use super::grid::TerminalGrid;
use unicode_width::UnicodeWidthChar;
use parking_lot::Mutex;
use std::sync::Arc;
use vte::Perform;

pub struct VtePerformer {
    pub grid: Arc<Mutex<TerminalGrid>>,
    /// Saved cursor state
    saved_cursor: Option<(usize, usize)>,
    /// Origin mode (DECOM)
    origin_mode: bool,
    /// Auto-wrap mode
    auto_wrap: bool,
}

impl VtePerformer {
    pub fn new(grid: Arc<Mutex<TerminalGrid>>) -> Self {
        Self {
            grid,
            saved_cursor: None,
            origin_mode: false,
            auto_wrap: true,
        }
    }
}

impl Perform for VtePerformer {
    fn print(&mut self, c: char) {
        // Determine display width: 2 for wide chars (CJK, emoji, etc.), 1 for normal.
        let width = c.width().unwrap_or(1).max(1);
        let mut grid = self.grid.lock();
        // Handle pending wrap
        if grid.pending_wrap && self.auto_wrap {
            let row = grid.cursor_row;
            // Move to next line
            if row == grid.scroll_bottom {
                grid.scroll_up_region(1);
            } else if row < grid.rows - 1 {
                grid.cursor_row += 1;
            }
            grid.cursor_col = 0;
            grid.pending_wrap = false;
        }
        let col = grid.cursor_col;
        let row = grid.cursor_row;
        grid.set_cell(col, row, c);
        // For wide (double-width) characters, blank the second cell so that
        // subsequent characters don't overwrite the right half of the glyph.
        if width == 2 {
            if col + 1 < grid.cols {
                grid.cells[row][col + 1] = Cell::default();
            }
        }
        grid.advance_cursor_by_width(width);
    }

    fn execute(&mut self, byte: u8) {
        let mut grid = self.grid.lock();
        match byte {
            0x08 => {
                // Backspace
                if grid.cursor_col > 0 {
                    grid.cursor_col -= 1;
                }
                grid.pending_wrap = false;
            }
            0x09 => {
                // Tab — advance to next tab stop (every 8 cols)
                let col = grid.cursor_col;
                let next_tab = ((col / 8) + 1) * 8;
                grid.cursor_col = next_tab.min(grid.cols - 1);
                grid.pending_wrap = false;
            }
            0x0a | 0x0b | 0x0c => {
                // LF, VT, FF
                drop(grid);
                let mut grid = self.grid.lock();
                grid.newline();
            }
            0x0d => {
                // CR
                grid.carriage_return();
            }
            0x07 => {
                // Bell — ignore
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        match params[0] {
            b"0" | b"2" => {
                // Set window title
                if params.len() > 1 {
                    if let Ok(title) = std::str::from_utf8(params[1]) {
                        let mut grid = self.grid.lock();
                        grid.title = title.to_string();
                    }
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }

        let ps: Vec<u16> = params.iter().map(|s| s[0]).collect();

        let mut grid = self.grid.lock();
        let rows = grid.rows;
        let cols = grid.cols;

        match (intermediates.first().copied(), action) {
            // Cursor up
            (None, 'A') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_row = grid.cursor_row.saturating_sub(n);
                grid.pending_wrap = false;
            }
            // Cursor down
            (None, 'B') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_row = (grid.cursor_row + n).min(rows - 1);
                grid.pending_wrap = false;
            }
            // Cursor forward
            (None, 'C') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_col = (grid.cursor_col + n).min(cols - 1);
                grid.pending_wrap = false;
            }
            // Cursor back
            (None, 'D') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_col = grid.cursor_col.saturating_sub(n);
                grid.pending_wrap = false;
            }
            // Cursor Next Line
            (None, 'E') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_row = (grid.cursor_row + n).min(rows - 1);
                grid.cursor_col = 0;
                grid.pending_wrap = false;
            }
            // Cursor Previous Line
            (None, 'F') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_row = grid.cursor_row.saturating_sub(n);
                grid.cursor_col = 0;
                grid.pending_wrap = false;
            }
            // Cursor Horizontal Absolute (CHA)
            (None, 'G') | (None, '`') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_col = (n - 1).min(cols - 1);
                grid.pending_wrap = false;
            }
            // Cursor Position (row, col — 1-indexed)
            (None, 'H') | (None, 'f') => {
                let row = ps.first().copied().unwrap_or(1).max(1) as usize;
                let col = ps.get(1).copied().unwrap_or(1).max(1) as usize;
                grid.cursor_row = (row - 1).min(rows - 1);
                grid.cursor_col = (col - 1).min(cols - 1);
                grid.pending_wrap = false;
            }
            // Erase in Display
            (None, 'J') => {
                let n = ps.first().copied().unwrap_or(0);
                let (cr, cc) = (grid.cursor_row, grid.cursor_col);
                match n {
                    0 => {
                        // Erase from cursor to end
                        grid.clear_line_range(cr, cc, cols);
                        for r in (cr + 1)..rows {
                            grid.clear_line(r);
                        }
                    }
                    1 => {
                        // Erase from start to cursor
                        for r in 0..cr {
                            grid.clear_line(r);
                        }
                        grid.clear_line_range(cr, 0, cc + 1);
                    }
                    2 | 3 => {
                        for r in 0..rows {
                            grid.clear_line(r);
                        }
                    }
                    _ => {}
                }
            }
            // Erase in Line
            (None, 'K') => {
                let n = ps.first().copied().unwrap_or(0);
                let (cr, cc) = (grid.cursor_row, grid.cursor_col);
                match n {
                    0 => grid.clear_line_range(cr, cc, cols),
                    1 => grid.clear_line_range(cr, 0, cc + 1),
                    2 => grid.clear_line(cr),
                    _ => {}
                }
            }
            // Insert Lines
            (None, 'L') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.scroll_down_region(n);
            }
            // Delete Lines
            (None, 'M') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.scroll_up_region(n);
            }
            // Delete Characters
            (None, 'P') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                let cr = grid.cursor_row;
                let cc = grid.cursor_col;
                let end = cols;
                let row = &mut grid.cells[cr];
                let shift = n.min(end - cc);
                for i in cc..(end - shift) {
                    row[i] = row[i + shift].clone();
                }
                for i in (end - shift)..end {
                    row[i] = Default::default();
                }
            }
            // Erase Characters
            (None, 'X') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                let cr = grid.cursor_row;
                let cc = grid.cursor_col;
                let end = (cc + n).min(cols);
                grid.clear_line_range(cr, cc, end);
            }
            // Insert Characters (ICH)
            (None, '@') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                let cr = grid.cursor_row;
                let cc = grid.cursor_col;
                let shift = n.min(cols - cc);
                let row = &mut grid.cells[cr];
                // Shift existing characters right to make room
                for i in (cc + shift..cols).rev() {
                    row[i] = row[i - shift].clone();
                }
                // Clear the inserted positions
                for i in cc..(cc + shift).min(cols) {
                    row[i] = Cell::default();
                }
                grid.generation = grid.generation.wrapping_add(1);
            }
            // Vertical Position Absolute (VPA) — CSI Pn d
            // Moves cursor to absolute row Pn (1-based) without changing column.
            // Used heavily by TUI frameworks (Ink/Claude Code) to position the
            // cursor at the input-box row after rendering the full UI.
            (None, 'd') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_row = (n - 1).min(rows - 1);
                grid.pending_wrap = false;
            }
            // Horizontal Position Relative (HPR) — CSI Pn a
            (None, 'a') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_col = (grid.cursor_col + n).min(cols - 1);
                grid.pending_wrap = false;
            }
            // Vertical Position Relative (VPR) — CSI Pn e
            (None, 'e') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.cursor_row = (grid.cursor_row + n).min(rows - 1);
                grid.pending_wrap = false;
            }
            // Repeat preceding graphic character (REP) — CSI Pn b
            (None, 'b') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                // Repeat the character in the cell just before the cursor
                let ch = if grid.cursor_col > 0 {
                    grid.cells[grid.cursor_row][grid.cursor_col - 1].ch
                } else {
                    ' '
                };
                if ch != '\0' {
                    for _ in 0..n {
                        let col = grid.cursor_col;
                        let row = grid.cursor_row;
                        if col < cols {
                            grid.set_cell(col, row, ch);
                            grid.advance_cursor();
                        }
                    }
                }
            }
            // Scroll Up
            (None, 'S') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.scroll_up_region(n);
            }
            // Scroll Down
            (None, 'T') => {
                let n = ps.first().copied().unwrap_or(1).max(1) as usize;
                grid.scroll_down_region(n);
            }
            // Set Scrolling Region (DECSTBM)
            (None, 'r') => {
                let top = ps.first().copied().unwrap_or(1).max(1) as usize;
                let bottom = ps.get(1).copied().unwrap_or(rows as u16) as usize;
                grid.scroll_top = (top - 1).min(rows - 1);
                grid.scroll_bottom = (bottom - 1).min(rows - 1);
                grid.cursor_row = if self.origin_mode { grid.scroll_top } else { 0 };
                grid.cursor_col = 0;
                grid.pending_wrap = false;
            }
            // SGR — Select Graphic Rendition
            (None, 'm') => {
                apply_sgr(&mut grid.current_attrs, &ps);
            }
            // Save cursor (ANSI)
            (None, 's') => {
                self.saved_cursor = Some((grid.cursor_row, grid.cursor_col));
            }
            // Restore cursor (ANSI)
            (None, 'u') => {
                if let Some((row, col)) = self.saved_cursor {
                    grid.cursor_row = row.min(rows - 1);
                    grid.cursor_col = col.min(cols - 1);
                    grid.pending_wrap = false;
                }
            }
            // DEC private modes
            (Some(b'?'), 'h') => {
                for p in &ps {
                    match p {
                        1 => {} // DECCKM — application cursor keys (ignore for now)
                        7 => { self.auto_wrap = true; }
                        25 => { grid.cursor_visible = true; }
                        1049 => {
                            // Alternate screen: save cursor, clear, reset margins
                            self.saved_cursor = Some((grid.cursor_row, grid.cursor_col));
                            for r in 0..rows { grid.clear_line(r); }
                            grid.cursor_row = 0;
                            grid.cursor_col = 0;
                            grid.scroll_top = 0;
                            grid.scroll_bottom = rows.saturating_sub(1);
                            grid.pending_wrap = false;
                        }
                        2004 => { grid.bracketed_paste = true; }
                        _ => {}
                    }
                }
            }
            (Some(b'?'), 'l') => {
                for p in &ps {
                    match p {
                        7 => { self.auto_wrap = false; }
                        25 => { grid.cursor_visible = false; }
                        2004 => { grid.bracketed_paste = false; }
                        1049 => {
                            // Exit alternate screen: clear, restore cursor & margins
                            for r in 0..rows { grid.clear_line(r); }
                            if let Some((row, col)) = self.saved_cursor {
                                grid.cursor_row = row.min(rows - 1);
                                grid.cursor_col = col.min(cols - 1);
                            } else {
                                grid.cursor_row = 0;
                                grid.cursor_col = 0;
                            }
                            grid.scroll_top = 0;
                            grid.scroll_bottom = rows.saturating_sub(1);
                            grid.pending_wrap = false;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        let mut grid = self.grid.lock();
        match (intermediates.first().copied(), byte) {
            // Save cursor (DECSC)
            (None, b'7') => {
                self.saved_cursor = Some((grid.cursor_row, grid.cursor_col));
            }
            // Restore cursor (DECRC)
            (None, b'8') => {
                if let Some((row, col)) = self.saved_cursor {
                    grid.cursor_row = row.min(grid.rows - 1);
                    grid.cursor_col = col.min(grid.cols - 1);
                    grid.pending_wrap = false;
                }
            }
            // Index (IND)
            (None, b'D') => {
                drop(grid);
                let mut grid = self.grid.lock();
                grid.newline();
            }
            // Next Line (NEL)
            (None, b'E') => {
                drop(grid);
                let mut grid = self.grid.lock();
                grid.cursor_col = 0;
                grid.newline();
            }
            // Reverse Index (RI)
            (None, b'M') => {
                if grid.cursor_row == grid.scroll_top {
                    grid.scroll_down_region(1);
                } else if grid.cursor_row > 0 {
                    grid.cursor_row -= 1;
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn apply_sgr(attrs: &mut CellAttributes, params: &[u16]) {
    let mut i = 0;
    if params.is_empty() {
        *attrs = CellAttributes::default();
        return;
    }
    while i < params.len() {
        match params[i] {
            0 => *attrs = CellAttributes::default(),
            1 => attrs.bold = true,
            2 => attrs.dim = true,
            3 => attrs.italic = true,
            4 => attrs.underline = true,
            5 | 6 => attrs.blink = true,
            7 => attrs.reverse = true,
            8 => attrs.invisible = true,
            9 => attrs.strikethrough = true,
            22 => { attrs.bold = false; attrs.dim = false; }
            23 => attrs.italic = false,
            24 => attrs.underline = false,
            25 => attrs.blink = false,
            27 => attrs.reverse = false,
            28 => attrs.invisible = false,
            29 => attrs.strikethrough = false,
            // Foreground colors (30-37 → palette 0-7)
            30..=37 => attrs.fg = Color::Indexed(params[i] as u8 - 30),
            38 => {
                if i + 1 < params.len() && params[i + 1] == 5 && i + 2 < params.len() {
                    attrs.fg = Color::Indexed(params[i + 2] as u8);
                    i += 2;
                } else if i + 1 < params.len() && params[i + 1] == 2 && i + 4 < params.len() {
                    attrs.fg = Color::Rgb(params[i+2] as u8, params[i+3] as u8, params[i+4] as u8);
                    i += 4;
                }
            }
            39 => attrs.fg = Color::Default,
            // Background colors (40-47 → palette 0-7, NOT 8-15)
            40..=47 => attrs.bg = Color::Indexed(params[i] as u8 - 40),
            48 => {
                if i + 1 < params.len() && params[i + 1] == 5 && i + 2 < params.len() {
                    attrs.bg = Color::Indexed(params[i + 2] as u8);
                    i += 2;
                } else if i + 1 < params.len() && params[i + 1] == 2 && i + 4 < params.len() {
                    attrs.bg = Color::Rgb(params[i+2] as u8, params[i+3] as u8, params[i+4] as u8);
                    i += 4;
                }
            }
            49 => attrs.bg = Color::Default,
            // Bright foreground (90-97 → palette 8-15)
            90..=97 => attrs.fg = Color::Indexed(params[i] as u8 - 90 + 8),
            // Bright background (100-107 → palette 8-15, NOT 16-23)
            100..=107 => attrs.bg = Color::Indexed(params[i] as u8 - 100 + 8),
            _ => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> CellAttributes {
        CellAttributes::default()
    }

    // ── SGR reset ───────────────────────────────────────────────────────

    #[test]
    fn sgr_0_resets() {
        let mut a = fresh();
        a.bold = true;
        a.fg = Color::Indexed(1);
        apply_sgr(&mut a, &[0]);
        assert_eq!(a, fresh());
    }

    #[test]
    fn empty_params_reset() {
        let mut a = fresh();
        a.bold = true;
        apply_sgr(&mut a, &[]);
        assert_eq!(a, fresh());
    }

    // ── SGR attributes ──────────────────────────────────────────────────

    #[test]
    fn sgr_bold() {
        let mut a = fresh();
        apply_sgr(&mut a, &[1]);
        assert!(a.bold);
    }

    #[test]
    fn sgr_italic() {
        let mut a = fresh();
        apply_sgr(&mut a, &[3]);
        assert!(a.italic);
    }

    #[test]
    fn sgr_underline() {
        let mut a = fresh();
        apply_sgr(&mut a, &[4]);
        assert!(a.underline);
    }

    #[test]
    fn sgr_reverse() {
        let mut a = fresh();
        apply_sgr(&mut a, &[7]);
        assert!(a.reverse);
    }

    #[test]
    fn sgr_strikethrough() {
        let mut a = fresh();
        apply_sgr(&mut a, &[9]);
        assert!(a.strikethrough);
    }

    #[test]
    fn sgr_dim() {
        let mut a = fresh();
        apply_sgr(&mut a, &[2]);
        assert!(a.dim);
    }

    // ── SGR un-attributes ───────────────────────────────────────────────

    #[test]
    fn sgr_22_unbold_undim() {
        let mut a = fresh();
        a.bold = true;
        a.dim = true;
        apply_sgr(&mut a, &[22]);
        assert!(!a.bold);
        assert!(!a.dim);
    }

    #[test]
    fn sgr_23_unitalic() {
        let mut a = fresh();
        a.italic = true;
        apply_sgr(&mut a, &[23]);
        assert!(!a.italic);
    }

    #[test]
    fn sgr_24_ununderline() {
        let mut a = fresh();
        a.underline = true;
        apply_sgr(&mut a, &[24]);
        assert!(!a.underline);
    }

    #[test]
    fn sgr_27_unreverse() {
        let mut a = fresh();
        a.reverse = true;
        apply_sgr(&mut a, &[27]);
        assert!(!a.reverse);
    }

    #[test]
    fn sgr_29_unstrikethrough() {
        let mut a = fresh();
        a.strikethrough = true;
        apply_sgr(&mut a, &[29]);
        assert!(!a.strikethrough);
    }

    // ── Foreground colors ───────────────────────────────────────────────

    #[test]
    fn sgr_30_37_fg_indexed() {
        let mut a = fresh();
        apply_sgr(&mut a, &[31]); // red
        assert_eq!(a.fg, Color::Indexed(1));
        apply_sgr(&mut a, &[37]); // white
        assert_eq!(a.fg, Color::Indexed(7));
    }

    #[test]
    fn sgr_38_5_n_fg_256() {
        let mut a = fresh();
        apply_sgr(&mut a, &[38, 5, 200]);
        assert_eq!(a.fg, Color::Indexed(200));
    }

    #[test]
    fn sgr_38_2_rgb_fg() {
        let mut a = fresh();
        apply_sgr(&mut a, &[38, 2, 255, 128, 0]);
        assert_eq!(a.fg, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn sgr_39_default_fg() {
        let mut a = fresh();
        a.fg = Color::Indexed(5);
        apply_sgr(&mut a, &[39]);
        assert_eq!(a.fg, Color::Default);
    }

    // ── Background colors ───────────────────────────────────────────────

    #[test]
    fn sgr_40_47_bg_indexed() {
        let mut a = fresh();
        apply_sgr(&mut a, &[41]); // red bg
        assert_eq!(a.bg, Color::Indexed(1));
        apply_sgr(&mut a, &[47]); // white bg
        assert_eq!(a.bg, Color::Indexed(7));
    }

    #[test]
    fn sgr_48_5_n_bg_256() {
        let mut a = fresh();
        apply_sgr(&mut a, &[48, 5, 100]);
        assert_eq!(a.bg, Color::Indexed(100));
    }

    #[test]
    fn sgr_48_2_rgb_bg() {
        let mut a = fresh();
        apply_sgr(&mut a, &[48, 2, 10, 20, 30]);
        assert_eq!(a.bg, Color::Rgb(10, 20, 30));
    }

    #[test]
    fn sgr_49_default_bg() {
        let mut a = fresh();
        a.bg = Color::Indexed(3);
        apply_sgr(&mut a, &[49]);
        assert_eq!(a.bg, Color::Default);
    }

    // ── Bright colors ───────────────────────────────────────────────────

    #[test]
    fn sgr_90_97_bright_fg() {
        let mut a = fresh();
        apply_sgr(&mut a, &[90]); // bright black
        assert_eq!(a.fg, Color::Indexed(8));
        apply_sgr(&mut a, &[97]); // bright white
        assert_eq!(a.fg, Color::Indexed(15));
    }

    #[test]
    fn sgr_100_107_bright_bg() {
        let mut a = fresh();
        apply_sgr(&mut a, &[100]); // bright black bg
        assert_eq!(a.bg, Color::Indexed(8));
        apply_sgr(&mut a, &[107]); // bright white bg
        assert_eq!(a.bg, Color::Indexed(15));
    }

    // ── Multiple params in one call ─────────────────────────────────────

    #[test]
    fn sgr_multiple_params() {
        let mut a = fresh();
        apply_sgr(&mut a, &[1, 3, 31]); // bold + italic + red fg
        assert!(a.bold);
        assert!(a.italic);
        assert_eq!(a.fg, Color::Indexed(1));
    }
}
