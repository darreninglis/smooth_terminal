#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Default for Color {
    fn default() -> Self {
        Color::Default
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellAttributes {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub blink: bool,
    pub reverse: bool,
    pub invisible: bool,
    pub dim: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub attrs: CellAttributes,
}

impl Cell {
    pub fn new(ch: char, attrs: CellAttributes) -> Self {
        Self { ch, attrs }
    }

    pub fn is_empty(&self) -> bool {
        self.ch == ' ' || self.ch == '\0'
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_default_is_default_variant() {
        assert_eq!(Color::default(), Color::Default);
    }

    #[test]
    fn cell_attributes_default_all_false() {
        let attrs = CellAttributes::default();
        assert_eq!(attrs.fg, Color::Default);
        assert_eq!(attrs.bg, Color::Default);
        assert!(!attrs.bold);
        assert!(!attrs.italic);
        assert!(!attrs.underline);
        assert!(!attrs.strikethrough);
        assert!(!attrs.blink);
        assert!(!attrs.reverse);
        assert!(!attrs.invisible);
        assert!(!attrs.dim);
    }

    #[test]
    fn cell_new_sets_fields() {
        let attrs = CellAttributes { bold: true, ..Default::default() };
        let cell = Cell::new('A', attrs);
        assert_eq!(cell.ch, 'A');
        assert!(cell.attrs.bold);
    }

    #[test]
    fn cell_default_is_empty() {
        let cell = Cell::default();
        assert!(cell.is_empty());
        assert_eq!(cell.ch, '\0');
    }

    #[test]
    fn cell_space_is_empty() {
        let cell = Cell::new(' ', CellAttributes::default());
        assert!(cell.is_empty());
    }

    #[test]
    fn cell_with_char_is_not_empty() {
        let cell = Cell::new('X', CellAttributes::default());
        assert!(!cell.is_empty());
    }

    #[test]
    fn cell_null_is_empty() {
        let cell = Cell::new('\0', CellAttributes::default());
        assert!(cell.is_empty());
    }

    #[test]
    fn color_indexed_and_rgb_variants() {
        let indexed = Color::Indexed(42);
        let rgb = Color::Rgb(255, 128, 0);
        assert_ne!(indexed, Color::Default);
        assert_ne!(rgb, Color::Default);
        assert_ne!(indexed, rgb);
    }

    #[test]
    fn cell_equality() {
        let a = Cell::new('A', CellAttributes::default());
        let b = Cell::new('A', CellAttributes::default());
        let c = Cell::new('B', CellAttributes::default());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn cell_attributes_equality_differs_on_any_field() {
        let base = CellAttributes::default();
        let bold = CellAttributes { bold: true, ..Default::default() };
        let fg = CellAttributes { fg: Color::Indexed(1), ..Default::default() };
        assert_ne!(base, bold);
        assert_ne!(base, fg);
        assert_ne!(bold, fg);
    }
}
