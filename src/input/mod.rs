use winit::event::{ElementState, KeyEvent, MouseScrollDelta};
use winit::keyboard::{Key, ModifiersState, NamedKey};

pub enum InputAction {
    WriteBytes(Vec<u8>),
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    FocusNext,
    FocusPrev,
    Scroll(f32),
    OpenConfig,
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
            // Pass character to PTY
            if cmd {
                return InputAction::None; // Don't pass Cmd shortcuts to shell
            }
            return InputAction::WriteBytes(encode_key_character(ch, ctrl, alt));
        }
        Key::Named(named) => {
            return InputAction::WriteBytes(encode_named_key(named, modifiers));
        }
        _ => {}
    }

    InputAction::None
}

fn encode_key_character(ch: &str, ctrl: bool, alt: bool) -> Vec<u8> {
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

fn encode_named_key(key: &NamedKey, modifiers: ModifiersState) -> Vec<u8> {
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
            else if shift { vec![0x1b, b'[', b'1', b';', b'2', b'A'] }
            else { vec![0x1b, b'[', b'A'] }
        }
        NamedKey::ArrowDown => {
            if ctrl { vec![0x1b, b'[', b'1', b';', b'5', b'B'] }
            else if shift { vec![0x1b, b'[', b'1', b';', b'2', b'B'] }
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
