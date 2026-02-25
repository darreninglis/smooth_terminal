# Smooth Terminal

A GPU-accelerated macOS terminal emulator built in Rust, using Metal (via wgpu) for rendering and a spring-based animation system for smooth scrolling and cursor movement.

## Features

- Metal GPU rendering via `wgpu`
- Smooth spring-animated cursor and scrolling
- True colour (24-bit) and 256-colour support
- Powerline / Nerd Font glyph support via multi-font fallback
- Configurable padding, opacity, and blur
- Native macOS menu bar integration
- Catppuccin Mocha colour scheme by default

## Requirements

- macOS 13 or later
- [Rust](https://rustup.rs) (stable toolchain)
- Xcode Command Line Tools (`xcode-select --install`)

## Building the app

### First time setup

```bash
# Clone the repo
git clone https://github.com/darreninglis/smooth_terminal.git
cd smooth_terminal

# Generate the app icon (only needed once, or when you change create_icon.sh)
make icon
```

### Build and run as a native .app

```bash
# Compile a release build and assemble Smooth Terminal.app in the project folder
make bundle

# Launch it immediately
open "Smooth Terminal.app"
```

### Install to /Applications

```bash
make install
```

After installing, **Smooth Terminal** will appear in Launchpad and can be launched from Spotlight or Finder like any other app.

### Quick development run (no bundle)

```bash
make run
# or
cargo run
```

### All make targets

| Command | Description |
|---|---|
| `make bundle` | Build release binary → assemble + sign `Smooth Terminal.app` |
| `make install` | `make bundle` then copy to `/Applications/` |
| `make icon` | Regenerate `macos/AppIcon.icns` from `macos/create_icon.sh` |
| `make run` | `cargo run` (development, no bundle) |
| `make clean` | Remove build artefacts and `Smooth Terminal.app` |

## Key bindings

### Pane management

| Shortcut | Action |
|---|---|
| `Cmd+D` | Split pane horizontally |
| `Cmd+Shift+D` | Split pane vertically |
| `Cmd+W` | Close current pane |
| `Cmd+]` | Focus next pane |
| `Cmd+[` | Focus previous pane |
| `Shift+Left` | Focus pane to the left |
| `Shift+Right` | Focus pane to the right |
| `Shift+Up` | Focus pane above |
| `Shift+Down` | Focus pane below |

### Other

| Shortcut | Action |
|---|---|
| `Cmd+,` | Open config file |

## Configuration

On first launch, a default config is written to:

```
~/Library/Application Support/smooth_terminal/config.toml
```

Edit that file to customise fonts, colours, padding, keybindings, and more. The app picks up changes on the next launch.

### Example config options

```toml
[window]
width   = 1200
height  = 800
opacity = 0.95
blur    = true
padding = 10

[font]
family      = "SF Mono Terminal Regular"
size        = 14.0
line_height = 1.2

[colors]
background = "#1e1e2e"
foreground = "#cdd6f4"
```

## Project structure

```
smooth_terminal/
├── src/
│   ├── main.rs              # Entry point
│   ├── app.rs               # Main application loop (winit event handler)
│   ├── config/              # Config loading and defaults
│   ├── renderer/            # wgpu / glyphon rendering pipeline
│   ├── terminal/            # VTE parser, PTY, grid
│   ├── animation/           # Spring and scroll animation
│   ├── pane/                # Pane layout
│   ├── input/               # Keyboard and mouse handling
│   └── menubar/             # macOS native menu bar
├── assets/
│   ├── shaders/             # WGSL shaders (embedded at compile time)
│   └── default_config.toml  # Default config (embedded at compile time)
├── macos/
│   ├── Info.plist           # App bundle metadata
│   ├── AppIcon.icns         # App icon
│   └── create_icon.sh       # Icon generation script
└── Makefile                 # Build system
```
