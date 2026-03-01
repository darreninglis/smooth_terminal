# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Development run (no bundle, fastest iteration)
make run          # or: ~/.cargo/bin/cargo run

# Release build only
~/.cargo/bin/cargo build --release

# Build + assemble Smooth Terminal.app in project folder
make bundle

# Build + install to /Applications (replaces existing)
make install

# Copy new binary directly into existing /Applications bundle (fastest deploy)
cp target/release/smooth_terminal "/Applications/Smooth Terminal.app/Contents/MacOS/smooth_terminal"

# Regenerate app icon (only when macos/create_icon.sh changes)
make icon

# Clean build artefacts and .app bundle
make clean
```

Note: `cargo` is not in the default PATH — always use `~/.cargo/bin/cargo` directly.

**After making any code changes, test, build and deploy with:**
```bash
~/.cargo/bin/cargo test && ~/.cargo/bin/cargo build --release && cp target/release/smooth_terminal "/Applications/Smooth Terminal.app/Contents/MacOS/smooth_terminal"
```

**Run tests before building:**
```bash
~/.cargo/bin/cargo test
# or: make test
```

## Architecture

The app is a macOS-only GPU-accelerated terminal emulator. The top-level data flow each frame is:

```
winit event loop (ControlFlow::Poll)
  └── App (ApplicationHandler)
        └── HashMap<WindowId, WindowState>
              ├── Window (winit Arc<Window>)
              ├── Renderer  ← wgpu/Metal, glyphon text
              └── PaneTree  ← layout tree + Vec<Pane>
                    └── Pane { id, Terminal }
                          ├── TerminalGrid (Arc<Mutex>)  ← cell buffer
                          └── PtyHandle  ← OS pseudo-terminal + child process
```

### Key architectural points

**WindowState** (`src/app.rs`) owns everything for one window. Each tab is a full WindowState with its own Renderer, PaneTree, and PTYs. Native macOS tabs are created via `addTabbedWindow_ordered` — there is no custom tab bar.

**Event → Action → PTY pipeline**: `WindowEvent::KeyboardInput` → `handle_key_event()` returns an `InputAction` enum → `app.rs` dispatches it. To add a new shortcut, add a variant to `InputAction` in `src/input/mod.rs`, pattern-match it in `handle_key_event`, then handle it in the `match action { ... }` block in `app.rs`.

**Rendering pipeline** (called each frame via `RedrawRequested`):
1. `pane_tree.drain_all_pty_output()` — drains PTY bytes through the VTE parser into `TerminalGrid`
2. `renderer.update_cursor_for_pane(...)` — updates spring targets
3. `renderer.tick_animations(dt)` — advances spring physics
4. `renderer.render(&pane_tree, rect)` — wgpu draw: background → cell backgrounds → text (glyphon) → cursor

**Layout tree** (`src/pane/layout.rs`): `Layout` is a recursive enum — `Leaf(pane_id)`, `HSplit`, or `VSplit`. `compute_rects(root_rect)` walks the tree and returns `Vec<(pane_id, Rect)>` used by both renderer and focus logic. All splits default to ratio 0.5.

**Config hot-reload**: A `notify` file watcher sends on a `crossbeam_channel` when the config TOML changes. The channel is polled each `RedrawRequested` frame; on change, `renderer.apply_config()` rebuilds font metrics and returns `true` if cell dimensions changed (triggering pane resize).

**macOS interop** (`src/app.rs`): CGPoint/CGSize/CGRect are defined locally as `#[repr(C)]` structs with manual `Encode` impls (not from any crate). Window tiling (`macos_tile_window`) uses `NSWindow.setFrame:display:animate:`. Tab switching uses `NSWindowTabGroup.windows`. The tab-rename double-click monitor uses `block2::StackBlock` — ObjC copies it to the heap automatically.

**Terminal grid** (`src/terminal/grid.rs`): `TerminalGrid` holds a `Vec<Cell>` (rows × cols), cursor position, scrollback, and a `generation: u64` counter incremented on every mutation. The renderer uses this generation to decide whether to rebuild its `SpanBuffer` text cache for a pane.
