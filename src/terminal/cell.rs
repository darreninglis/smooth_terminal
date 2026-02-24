#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
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

#[derive(Debug, Clone, Default, PartialEq)]
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
