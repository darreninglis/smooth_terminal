use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set by `Config::open_in_editor()` (called from ObjC menu handlers that
/// have no access to `App`).  Polled each frame in the winit event loop.
pub static OPEN_CONFIG_REQUESTED: AtomicBool = AtomicBool::new(false);
const DEFAULT_CONFIG: &str = include_str!("../../assets/default_config.toml");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    pub opacity: f32,
    pub blur: bool,
    /// Padding around the content area in logical (CSS) pixels.
    /// Scaled by the display's DPI factor before use.
    #[serde(default = "default_padding")]
    pub padding: f32,
}

fn default_padding() -> f32 { 10.0 }

impl Default for WindowConfig {
    fn default() -> Self {
        Self { width: 1200, height: 800, opacity: 0.95, blur: true, padding: default_padding() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    pub line_height: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "JetBrains Mono".to_string(),
            size: 14.0,
            line_height: 1.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorsConfig {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    #[serde(default = "default_black")]
    pub black: String,
    #[serde(default = "default_red")]
    pub red: String,
    #[serde(default = "default_green")]
    pub green: String,
    #[serde(default = "default_yellow")]
    pub yellow: String,
    #[serde(default = "default_blue")]
    pub blue: String,
    #[serde(default = "default_magenta")]
    pub magenta: String,
    #[serde(default = "default_cyan")]
    pub cyan: String,
    #[serde(default = "default_white")]
    pub white: String,
    #[serde(default = "default_bright_black")]
    pub bright_black: String,
    #[serde(default = "default_bright_red")]
    pub bright_red: String,
    #[serde(default = "default_bright_green")]
    pub bright_green: String,
    #[serde(default = "default_bright_yellow")]
    pub bright_yellow: String,
    #[serde(default = "default_bright_blue")]
    pub bright_blue: String,
    #[serde(default = "default_bright_magenta")]
    pub bright_magenta: String,
    #[serde(default = "default_bright_cyan")]
    pub bright_cyan: String,
    #[serde(default = "default_bright_white")]
    pub bright_white: String,
}

fn default_black() -> String { "#45475a".to_string() }
fn default_red() -> String { "#f38ba8".to_string() }
fn default_green() -> String { "#a6e3a1".to_string() }
fn default_yellow() -> String { "#f9e2af".to_string() }
fn default_blue() -> String { "#89b4fa".to_string() }
fn default_magenta() -> String { "#f5c2e7".to_string() }
fn default_cyan() -> String { "#94e2d5".to_string() }
fn default_white() -> String { "#bac2de".to_string() }
fn default_bright_black() -> String { "#585b70".to_string() }
fn default_bright_red() -> String { "#f38ba8".to_string() }
fn default_bright_green() -> String { "#a6e3a1".to_string() }
fn default_bright_yellow() -> String { "#f9e2af".to_string() }
fn default_bright_blue() -> String { "#89b4fa".to_string() }
fn default_bright_magenta() -> String { "#f5c2e7".to_string() }
fn default_bright_cyan() -> String { "#94e2d5".to_string() }
fn default_bright_white() -> String { "#a6adc8".to_string() }

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            background: "#000000".to_string(),
            foreground: "#ffffff".to_string(),
            cursor: "#bf00ff".to_string(),
            black: default_black(),
            red: default_red(),
            green: default_green(),
            yellow: default_yellow(),
            blue: default_blue(),
            magenta: default_magenta(),
            cyan: default_cyan(),
            white: default_white(),
            bright_black: default_bright_black(),
            bright_red: default_bright_red(),
            bright_green: default_bright_green(),
            bright_yellow: default_bright_yellow(),
            bright_blue: default_bright_blue(),
            bright_magenta: default_bright_magenta(),
            bright_cyan: default_bright_cyan(),
            bright_white: default_bright_white(),
        }
    }
}

impl ColorsConfig {
    pub fn ansi_palette(&self) -> [[f32; 4]; 16] {
        let colors = [
            &self.black, &self.red, &self.green, &self.yellow,
            &self.blue, &self.magenta, &self.cyan, &self.white,
            &self.bright_black, &self.bright_red, &self.bright_green, &self.bright_yellow,
            &self.bright_blue, &self.bright_magenta, &self.bright_cyan, &self.bright_white,
        ];
        let mut palette = [[0.0f32; 4]; 16];
        for (i, hex) in colors.iter().enumerate() {
            palette[i] = parse_hex_color(hex).unwrap_or([1.0, 1.0, 1.0, 1.0]);
        }
        palette
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationConfig {
    pub target_fps: u32,
    pub cursor_spring_frequency: f32,
    pub scroll_spring_frequency: f32,
    pub cursor_trail_enabled: bool,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            target_fps: 120,
            cursor_spring_frequency: 8.0,
            scroll_spring_frequency: 15.0,
            cursor_trail_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackgroundConfig {
    pub image_path: Option<String>,
    pub image_opacity: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
    pub split_horizontal: String,
    pub split_vertical: String,
    pub close_pane: String,
    pub focus_next: String,
    pub focus_prev: String,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            split_horizontal: "Cmd+D".to_string(),
            split_vertical: "Cmd+Shift+D".to_string(),
            close_pane: "Cmd+W".to_string(),
            focus_next: "Cmd+]".to_string(),
            focus_prev: "Cmd+[".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub font: FontConfig,
    #[serde(default)]
    pub colors: ColorsConfig,
    #[serde(default)]
    pub animation: AnimationConfig,
    #[serde(default)]
    pub background: BackgroundConfig,
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
}

impl Config {
    pub fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
        base.join("smooth_terminal").join("config.toml")
    }

    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(cfg) => return cfg,
                    Err(e) => {
                        log::warn!("Failed to parse config at {:?}: {}", path, e);
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read config at {:?}: {}", path, e);
                }
            }
        } else {
            // Write default config
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, DEFAULT_CONFIG);
        }
        toml::from_str(DEFAULT_CONFIG).unwrap_or_default()
    }

    /// Signal the winit event loop to open the config file in vim inside the
    /// focused terminal pane.  Safe to call from ObjC handlers.
    pub fn open_in_editor() -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            std::fs::write(&path, DEFAULT_CONFIG)?;
        }
        OPEN_CONFIG_REQUESTED.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Toggle between dark and light theme by rewriting the [colors] section
    /// of config.toml.  The file-watcher hot-reload picks up the change.
    pub fn toggle_theme(&mut self) {
        let is_dark = is_dark_background(&self.colors.background);
        if is_dark {
            self.colors = light_colors();
        } else {
            self.colors = dark_colors();
        }
        // Write the updated config back to disk
        if let Ok(toml_str) = toml::to_string_pretty(self) {
            let path = Self::config_path();
            let _ = std::fs::write(&path, toml_str);
        }
    }
}

pub(crate) fn is_dark_background(hex: &str) -> bool {
    if let Some(rgba) = parse_hex_color(hex) {
        // Luminance: 0.299*R + 0.587*G + 0.114*B
        let lum = 0.299 * rgba[0] + 0.587 * rgba[1] + 0.114 * rgba[2];
        lum < 0.5
    } else {
        true // assume dark if unparseable
    }
}

pub(crate) fn dark_colors() -> ColorsConfig {
    ColorsConfig {
        background: "#000000".into(),
        foreground: "#ffffff".into(),
        cursor: "#bf00ff".into(),
        black: "#45475a".into(),
        red: "#f38ba8".into(),
        green: "#a6e3a1".into(),
        yellow: "#f9e2af".into(),
        blue: "#89b4fa".into(),
        magenta: "#f5c2e7".into(),
        cyan: "#94e2d5".into(),
        white: "#bac2de".into(),
        bright_black: "#585b70".into(),
        bright_red: "#f38ba8".into(),
        bright_green: "#a6e3a1".into(),
        bright_yellow: "#f9e2af".into(),
        bright_blue: "#89b4fa".into(),
        bright_magenta: "#f5c2e7".into(),
        bright_cyan: "#94e2d5".into(),
        bright_white: "#a6adc8".into(),
    }
}

pub(crate) fn light_colors() -> ColorsConfig {
    ColorsConfig {
        background: "#eff1f5".into(),
        foreground: "#4c4f69".into(),
        cursor: "#7c3aed".into(),
        black: "#5c5f77".into(),
        red: "#d20f39".into(),
        green: "#40a02b".into(),
        yellow: "#df8e1d".into(),
        blue: "#1e66f5".into(),
        magenta: "#ea76cb".into(),
        cyan: "#179299".into(),
        white: "#acb0be".into(),
        bright_black: "#6c6f85".into(),
        bright_red: "#d20f39".into(),
        bright_green: "#40a02b".into(),
        bright_yellow: "#df8e1d".into(),
        bright_blue: "#1e66f5".into(),
        bright_magenta: "#ea76cb".into(),
        bright_cyan: "#179299".into(),
        bright_white: "#bcc0cc".into(),
    }
}

pub fn parse_hex_color(hex: &str) -> Option<[f32; 4]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
    } else if hex.len() == 8 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
        Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_hex_color ─────────────────────────────────────────────────

    #[test]
    fn parse_hex_6_digit() {
        let c = parse_hex_color("#ff0000").unwrap();
        assert!((c[0] - 1.0).abs() < 0.001);
        assert!((c[1]).abs() < 0.001);
        assert!((c[2]).abs() < 0.001);
        assert!((c[3] - 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_hex_8_digit() {
        let c = parse_hex_color("#ff000080").unwrap();
        assert!((c[0] - 1.0).abs() < 0.001);
        assert!((c[3] - 128.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn parse_hex_without_hash() {
        let c = parse_hex_color("00ff00").unwrap();
        assert!((c[1] - 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_hex_invalid() {
        assert!(parse_hex_color("xyz").is_none());
        assert!(parse_hex_color("#gg0000").is_none());
        assert!(parse_hex_color("").is_none());
    }

    #[test]
    fn parse_hex_wrong_length() {
        assert!(parse_hex_color("#fff").is_none());
        assert!(parse_hex_color("#fffffffff").is_none());
    }

    // ── is_dark_background ──────────────────────────────────────────────

    #[test]
    fn black_is_dark() {
        assert!(is_dark_background("#000000"));
    }

    #[test]
    fn white_is_light() {
        assert!(!is_dark_background("#ffffff"));
    }

    #[test]
    fn mid_gray_threshold() {
        // Luminance of #808080: 0.299*0.502 + 0.587*0.502 + 0.114*0.502 ≈ 0.502
        assert!(!is_dark_background("#808080"));
        // Darker gray
        assert!(is_dark_background("#333333"));
    }

    #[test]
    fn invalid_hex_assumed_dark() {
        assert!(is_dark_background("not-a-color"));
    }

    // ── ColorsConfig::ansi_palette ──────────────────────────────────────

    #[test]
    fn ansi_palette_has_16_entries() {
        let colors = ColorsConfig::default();
        let palette = colors.ansi_palette();
        assert_eq!(palette.len(), 16);
    }

    #[test]
    fn ansi_palette_index_0_is_black() {
        let colors = ColorsConfig::default();
        let palette = colors.ansi_palette();
        let expected = parse_hex_color(&colors.black).unwrap();
        assert_eq!(palette[0], expected);
    }

    #[test]
    fn ansi_palette_index_8_is_bright_black() {
        let colors = ColorsConfig::default();
        let palette = colors.ansi_palette();
        let expected = parse_hex_color(&colors.bright_black).unwrap();
        assert_eq!(palette[8], expected);
    }

    // ── Config round-trip ───────────────────────────────────────────────

    #[test]
    fn default_config_round_trips_toml() {
        let cfg = Config::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let cfg2: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(cfg.font.family, cfg2.font.family);
        assert_eq!(cfg.window.width, cfg2.window.width);
        assert_eq!(cfg.colors.background, cfg2.colors.background);
    }

    // ── dark_colors / light_colors ──────────────────────────────────────

    #[test]
    fn dark_colors_bg_is_dark() {
        let c = dark_colors();
        assert!(is_dark_background(&c.background));
    }

    #[test]
    fn light_colors_bg_is_light() {
        let c = light_colors();
        assert!(!is_dark_background(&c.background));
    }
}
