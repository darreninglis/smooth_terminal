use winit::event::{ElementState, KeyEvent, MouseScrollDelta};
use winit::keyboard::{Key, ModifiersState, NamedKey};

pub enum InputAction {
    WriteBytes(Vec<u8>),
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    FocusNext,
    FocusPrev,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    Scroll(f32),
    OpenConfig,
    NewTab,
    NewWindow,
    SwitchTab(usize),
    TileLeft,
    TileRight,
    Maximize,
    RestoreWindow,
    // Scrollback navigation
    ScrollViewUp,
    ScrollViewDown,
    // Clipboard
    CopySelection,
    Paste,
    // Pane resize (Ctrl+Option+Arrow)
    ResizePaneLeft,
    ResizePaneRight,
    ResizePaneUp,
    ResizePaneDown,
    ToggleTheme,
    None,
}

pub fn handle_key_event(
    event: &KeyEvent,
    modifiers: ModifiersState,
) -> InputAction {
    if event.state != ElementState::Pressed {
        return InputAction::None;
    }

    let cmd = modifiers.super_key();
    let shift = modifiers.shift_key();
    let ctrl = modifiers.control_key();
    let alt = modifiers.alt_key();

    // Pane management shortcuts (macOS Cmd-based)
    match &event.logical_key {
        Key::Character(s) => {
            let ch = s.as_str();
            // Lowercase only for shortcut matching: winit reports "D" not "d"
            // for Cmd+Shift+D, but we must keep the original for PTY writes so
            // that uppercase letters are not silently downcased.
            let lc = s.to_lowercase();
            let lc = lc.as_str();
            if cmd && !shift && lc == "d" {
                return InputAction::SplitHorizontal;
            }
            if cmd && shift && lc == "d" {
                return InputAction::SplitVertical;
            }
            if cmd && !shift && lc == "w" {
                return InputAction::ClosePane;
            }
            if cmd && lc == "]" {
                return InputAction::FocusNext;
            }
            if cmd && lc == "[" {
                return InputAction::FocusPrev;
            }
            if cmd && lc == "," {
                return InputAction::OpenConfig;
            }
            if cmd && !shift && lc == "t" {
                return InputAction::NewTab;
            }
            if cmd && !shift && lc == "n" {
                return InputAction::NewWindow;
            }
            // Cmd+Shift+L: toggle light/dark theme
            if cmd && shift && !ctrl && lc == "l" {
                return InputAction::ToggleTheme;
            }
            // Cmd+C: copy selection
            if cmd && !shift && !ctrl && lc == "c" {
                return InputAction::CopySelection;
            }
            // Cmd+V: paste
            if cmd && !shift && !ctrl && lc == "v" {
                return InputAction::Paste;
            }
            // Cmd+1-9: switch to tab N
            if cmd && !shift && !ctrl && !alt {
                if let Ok(n) = lc.parse::<usize>() {
                    if (1..=9).contains(&n) {
                        return InputAction::SwitchTab(n);
                    }
                }
            }
            // Ctrl+F: maximize window (fn+Ctrl+F on macOS)
            if ctrl && !shift && !cmd && !alt && lc == "f" {
                return InputAction::Maximize;
            }
            // Pass character to PTY
            if cmd {
                return InputAction::None; // Don't pass Cmd shortcuts to shell
            }
            return InputAction::WriteBytes(encode_key_character(ch, ctrl, alt));
        }
        Key::Named(named) => {
            if shift {
                match named {
                    NamedKey::ArrowLeft  => return InputAction::FocusLeft,
                    NamedKey::ArrowRight => return InputAction::FocusRight,
                    NamedKey::ArrowUp    => return InputAction::FocusUp,
                    NamedKey::ArrowDown  => return InputAction::FocusDown,
                    _ => {}
                }
            }
            // fn+Ctrl+Arrow keys: map to window tiling actions.
            // On macOS fn+Right=End, fn+Left=Home, fn+Up=PageUp, fn+Down=PageDown.
            if ctrl && !shift && !cmd {
                match named {
                    NamedKey::End      => return InputAction::TileRight,
                    NamedKey::Home     => return InputAction::TileLeft,
                    NamedKey::PageUp   => return InputAction::Maximize,
                    NamedKey::PageDown => return InputAction::RestoreWindow,
                    _ => {}
                }
            }
            // Ctrl+Option+Arrow: resize focused pane
            if ctrl && alt && !cmd && !shift {
                match named {
                    NamedKey::ArrowLeft  => return InputAction::ResizePaneLeft,
                    NamedKey::ArrowRight => return InputAction::ResizePaneRight,
                    NamedKey::ArrowUp    => return InputAction::ResizePaneUp,
                    NamedKey::ArrowDown  => return InputAction::ResizePaneDown,
                    _ => {}
                }
            }
            // Cmd+Up/Down: scrollback navigation
            if cmd && !shift && !ctrl {
                match named {
                    NamedKey::ArrowUp   => return InputAction::ScrollViewUp,
                    NamedKey::ArrowDown => return InputAction::ScrollViewDown,
                    _ => {}
                }
            }
            // Don't forward Cmd+named-key combos to the PTY; let macOS handle them
            // (e.g. Cmd+Arrow for window management).
            if cmd {
                return InputAction::None;
            }
            return InputAction::WriteBytes(encode_named_key(named, modifiers));
        }
        _ => {}
    }

    InputAction::None
}

pub(crate) fn encode_key_character(ch: &str, ctrl: bool, alt: bool) -> Vec<u8> {
    if ctrl {
        // Ctrl+char: send control code
        if let Some(c) = ch.chars().next() {
            let c_upper = c.to_ascii_uppercase();
            if c_upper >= 'A' && c_upper <= '_' {
                return vec![c_upper as u8 - b'A' + 1];
            }
        }
    }
    if alt {
        // Alt+char: send ESC prefix
        let mut bytes = vec![0x1b];
        bytes.extend(ch.as_bytes());
        return bytes;
    }
    ch.as_bytes().to_vec()
}

pub(crate) fn encode_named_key(key: &NamedKey, modifiers: ModifiersState) -> Vec<u8> {
    let shift = modifiers.shift_key();
    let ctrl = modifiers.control_key();
    let alt = modifiers.alt_key();

    match key {
        NamedKey::Space => vec![b' '],
        NamedKey::Enter => vec![b'\r'],
        NamedKey::Tab => {
            if shift {
                vec![0x1b, b'[', b'Z'] // Backtab
            } else {
                vec![b'\t']
            }
        }
        NamedKey::Backspace => vec![0x7f],
        NamedKey::Delete => vec![0x1b, b'[', b'3', b'~'],
        NamedKey::Escape => vec![0x1b],
        NamedKey::ArrowUp => {
            if ctrl { vec![0x1b, b'[', b'1', b';', b'5', b'A'] }
            else { vec![0x1b, b'[', b'A'] }
        }
        NamedKey::ArrowDown => {
            if ctrl { vec![0x1b, b'[', b'1', b';', b'5', b'B'] }
            else { vec![0x1b, b'[', b'B'] }
        }
        NamedKey::ArrowRight => {
            if ctrl { vec![0x1b, b'[', b'1', b';', b'5', b'C'] }
            else if alt { vec![0x1b, b'b'] }
            else { vec![0x1b, b'[', b'C'] }
        }
        NamedKey::ArrowLeft => {
            if ctrl { vec![0x1b, b'[', b'1', b';', b'5', b'D'] }
            else if alt { vec![0x1b, b'f'] }
            else { vec![0x1b, b'[', b'D'] }
        }
        NamedKey::Home => vec![0x1b, b'[', b'H'],
        NamedKey::End => vec![0x1b, b'[', b'F'],
        NamedKey::PageUp => vec![0x1b, b'[', b'5', b'~'],
        NamedKey::PageDown => vec![0x1b, b'[', b'6', b'~'],
        NamedKey::F1 => vec![0x1b, b'O', b'P'],
        NamedKey::F2 => vec![0x1b, b'O', b'Q'],
        NamedKey::F3 => vec![0x1b, b'O', b'R'],
        NamedKey::F4 => vec![0x1b, b'O', b'S'],
        NamedKey::F5 => vec![0x1b, b'[', b'1', b'5', b'~'],
        NamedKey::F6 => vec![0x1b, b'[', b'1', b'7', b'~'],
        NamedKey::F7 => vec![0x1b, b'[', b'1', b'8', b'~'],
        NamedKey::F8 => vec![0x1b, b'[', b'1', b'9', b'~'],
        NamedKey::F9 => vec![0x1b, b'[', b'2', b'0', b'~'],
        NamedKey::F10 => vec![0x1b, b'[', b'2', b'1', b'~'],
        NamedKey::F11 => vec![0x1b, b'[', b'2', b'3', b'~'],
        NamedKey::F12 => vec![0x1b, b'[', b'2', b'4', b'~'],
        _ => vec![],
    }
}

pub fn handle_scroll(delta: MouseScrollDelta, scale_factor: f64) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y * 20.0,
        MouseScrollDelta::PixelDelta(p) => p.y as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::ModifiersState;

    // ── encode_key_character ────────────────────────────────────────────

    #[test]
    fn plain_char() {
        assert_eq!(encode_key_character("a", false, false), b"a".to_vec());
        assert_eq!(encode_key_character("Z", false, false), b"Z".to_vec());
    }

    #[test]
    fn ctrl_a_is_0x01() {
        assert_eq!(encode_key_character("a", true, false), vec![0x01]);
    }

    #[test]
    fn ctrl_z_is_0x1a() {
        assert_eq!(encode_key_character("z", true, false), vec![0x1a]);
    }

    #[test]
    fn ctrl_c_is_0x03() {
        assert_eq!(encode_key_character("c", true, false), vec![0x03]);
    }

    #[test]
    fn alt_a_is_esc_a() {
        assert_eq!(encode_key_character("a", false, true), vec![0x1b, b'a']);
    }

    #[test]
    fn alt_z_is_esc_z() {
        assert_eq!(encode_key_character("z", false, true), vec![0x1b, b'z']);
    }

    // ── encode_named_key ────────────────────────────────────────────────

    fn mods(shift: bool, ctrl: bool, alt: bool) -> ModifiersState {
        let mut m = ModifiersState::empty();
        if shift { m |= ModifiersState::SHIFT; }
        if ctrl { m |= ModifiersState::CONTROL; }
        if alt { m |= ModifiersState::ALT; }
        m
    }

    #[test]
    fn enter_is_cr() {
        assert_eq!(encode_named_key(&NamedKey::Enter, mods(false, false, false)), vec![b'\r']);
    }

    #[test]
    fn tab_is_tab() {
        assert_eq!(encode_named_key(&NamedKey::Tab, mods(false, false, false)), vec![b'\t']);
    }

    #[test]
    fn shift_tab_is_backtab() {
        assert_eq!(encode_named_key(&NamedKey::Tab, mods(true, false, false)), vec![0x1b, b'[', b'Z']);
    }

    #[test]
    fn backspace_is_0x7f() {
        assert_eq!(encode_named_key(&NamedKey::Backspace, mods(false, false, false)), vec![0x7f]);
    }

    #[test]
    fn delete_sequence() {
        assert_eq!(encode_named_key(&NamedKey::Delete, mods(false, false, false)), vec![0x1b, b'[', b'3', b'~']);
    }

    #[test]
    fn escape_is_0x1b() {
        assert_eq!(encode_named_key(&NamedKey::Escape, mods(false, false, false)), vec![0x1b]);
    }

    #[test]
    fn arrow_up() {
        assert_eq!(encode_named_key(&NamedKey::ArrowUp, mods(false, false, false)), vec![0x1b, b'[', b'A']);
    }

    #[test]
    fn arrow_down() {
        assert_eq!(encode_named_key(&NamedKey::ArrowDown, mods(false, false, false)), vec![0x1b, b'[', b'B']);
    }

    #[test]
    fn arrow_right() {
        assert_eq!(encode_named_key(&NamedKey::ArrowRight, mods(false, false, false)), vec![0x1b, b'[', b'C']);
    }

    #[test]
    fn arrow_left() {
        assert_eq!(encode_named_key(&NamedKey::ArrowLeft, mods(false, false, false)), vec![0x1b, b'[', b'D']);
    }

    #[test]
    fn ctrl_arrow_up() {
        assert_eq!(encode_named_key(&NamedKey::ArrowUp, mods(false, true, false)), vec![0x1b, b'[', b'1', b';', b'5', b'A']);
    }

    #[test]
    fn ctrl_arrow_down() {
        assert_eq!(encode_named_key(&NamedKey::ArrowDown, mods(false, true, false)), vec![0x1b, b'[', b'1', b';', b'5', b'B']);
    }

    #[test]
    fn alt_arrow_right_is_word_forward() {
        assert_eq!(encode_named_key(&NamedKey::ArrowRight, mods(false, false, true)), vec![0x1b, b'b']);
    }

    #[test]
    fn alt_arrow_left_is_word_backward() {
        assert_eq!(encode_named_key(&NamedKey::ArrowLeft, mods(false, false, true)), vec![0x1b, b'f']);
    }

    #[test]
    fn f1_to_f4() {
        assert_eq!(encode_named_key(&NamedKey::F1, mods(false, false, false)), vec![0x1b, b'O', b'P']);
        assert_eq!(encode_named_key(&NamedKey::F2, mods(false, false, false)), vec![0x1b, b'O', b'Q']);
        assert_eq!(encode_named_key(&NamedKey::F3, mods(false, false, false)), vec![0x1b, b'O', b'R']);
        assert_eq!(encode_named_key(&NamedKey::F4, mods(false, false, false)), vec![0x1b, b'O', b'S']);
    }

    #[test]
    fn f5_to_f12() {
        assert_eq!(encode_named_key(&NamedKey::F5, mods(false, false, false)), vec![0x1b, b'[', b'1', b'5', b'~']);
        assert_eq!(encode_named_key(&NamedKey::F12, mods(false, false, false)), vec![0x1b, b'[', b'2', b'4', b'~']);
    }

    #[test]
    fn home_end_pageup_pagedown() {
        assert_eq!(encode_named_key(&NamedKey::Home, mods(false, false, false)), vec![0x1b, b'[', b'H']);
        assert_eq!(encode_named_key(&NamedKey::End, mods(false, false, false)), vec![0x1b, b'[', b'F']);
        assert_eq!(encode_named_key(&NamedKey::PageUp, mods(false, false, false)), vec![0x1b, b'[', b'5', b'~']);
        assert_eq!(encode_named_key(&NamedKey::PageDown, mods(false, false, false)), vec![0x1b, b'[', b'6', b'~']);
    }

    #[test]
    fn space_is_space() {
        assert_eq!(encode_named_key(&NamedKey::Space, mods(false, false, false)), vec![b' ']);
    }
}
