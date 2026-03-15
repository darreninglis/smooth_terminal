# Autoresearch Session

## Objective
Optimize rendering frame time — reduce the average time per frame for the full rendering pipeline (PTY drain → VTE parse → text shaping → GPU render).

## Metric
- **Name:** `frame_time_us`
- **Direction:** `lower`
- **Unit:** microseconds (µs)

## Benchmark Command
`./autoresearch.sh`

## Files In Scope
- `src/renderer/mod.rs` — main render pipeline, caching, layout
- `src/renderer/text_renderer.rs` — glyphon text shaping, span buffers
- `src/renderer/cell_bg.rs` — quad rendering (cell backgrounds, cursor, selection)
- `src/renderer/cursor.rs` — cursor animation
- `src/app.rs` — event loop, frame orchestration
- `src/pane/mod.rs` — pane tree, PTY drain
- `src/terminal/mod.rs` — PTY + VTE integration

## Files NOT In Scope
- `src/config/mod.rs` — config parsing (not in hot path)
- `src/input/mod.rs` — keyboard input handling
- `src/menubar/mod.rs` — macOS menu bar
- `src/terminal/grid.rs` — grid data structures (changing these risks correctness)
- `src/terminal/parser.rs` — VTE performer (changing risks correctness)
- Test files
- `Cargo.toml` (unless adding an optimization-related dep)

## Context
- GPU-accelerated terminal emulator using wgpu (Metal on macOS), glyphon for text
- Benchmark fills entire terminal grid with colored text and reshapes every frame (worst case)
- Text cache invalidation triggers full glyphon reshape — this is the expected hot path
- The `generation` counter on TerminalGrid triggers cache rebuilds

## Ideas To Try
1. ~~Batch glyphon text shaping — shape all panes in a single prepare() call instead of per-pane~~
2. ~~Reduce SpanBuffer allocations — reuse Vec buffers across frames~~
3. Skip text reshape when only cursor position changed (separate cursor gen from content gen)
4. ~~Use indexed drawing for cell_bg quads instead of per-vertex~~
5. ~~Reduce mutex lock contention on TerminalGrid~~
6. ~~Pre-compute color lookups (avoid re-parsing hex colors every frame)~~
7. Avoid recomputing layout_rects every frame when nothing changed
8. ~~Use smaller vertex format for cell background quads~~
9. ~~Profile and optimize the render pass structure (fewer passes)~~
10. ~~Cache the command encoder setup between frames~~
11. Reuse Buffer objects across frames instead of recreating

---

## Results

| Run | Description | Metric | Status | Notes |
|-----|-------------|--------|--------|-------|
| 0 | baseline | 14971µs | kept | p50=14820µs p95=16072µs |
| 1 | avoid per-cell String allocation | 15994µs | reverted | no improvement |
| 2 | use Shaping::Basic instead of Advanced | 13132µs | kept | -12.3% |
| 3 | cache x_offset per char + avoid to_string | 16432µs | reverted | HashMap overhead worse |
| 4 | pre-allocate span buffer Vec capacity | 10711µs | kept | -18.4% |
| 5 | batch same-color cells into multi-char Buffers | 11412µs | reverted | multi-char shaping slower |
| 6 | skip hex color detection entirely | 10220µs | reverted | removes feature |
| 7 | lazy hex color detection (only rows with #) | 10388µs | kept | -3% |
| 8 | remove redundant all-empty row scan | 9989µs | kept | -3.8% |
| 9 | skip space characters | 10312µs | reverted | no improvement |
| 10 | avoid cloning font_family String | 9657µs | kept | -3.3% |
| 11 | hoist cursor_info out of loop | 10046µs | reverted | compiler already did it |
| 12 | reduce frame latency 2→1 | 13128µs | reverted | GPU waits more |
| 13 | inline hot path functions | 10649µs | reverted | code size hurts cache |
| 14 | codegen-units = 1 | 9933µs | reverted | no improvement, slower builds |
| 15 | pre-compute mono glyph x_offset | 10249µs | reverted | not the bottleneck |
| 16 | **row-level Buffers with set_rich_text** | **2271µs** | **kept** | **-76.5%** — massive win |
| 17 | row-level Buffer for scrollback spans | 1895µs | kept | -16.5% |

**Current best:** 1895µs (was 14971µs baseline — **87.3% total reduction**)

## Learnings

### What worked
- **Shaping::Basic** instead of Advanced — no complex text layout needed for terminal
- **Pre-allocating Vec** with estimated capacity — avoids reallocation
- **Lazy hex color detection** — skip scanning rows without '#'
- **Removing redundant empty-row checks** — the cell loop already skips empty cells
- **Avoiding String clones** — use references where possible
- **ROW-LEVEL BUFFERS** — the single biggest win (76.5%). Using set_rich_text() to create one Buffer per row instead of one per cell reduces Buffer::new() + shape_until_scroll() calls from ~1920 to ~24 per cache rebuild

### What didn't work
- Per-cell String allocation avoidance (to_string → encode_utf8) — negligible cost
- HashMap-based x_offset cache — lookup overhead worse than recomputing
- Multi-char Buffer batching (before set_rich_text) — multi-char shaping is slower per-char
- #[inline(always)] on hot functions — increases code size, hurts icache
- codegen-units = 1 — LTO already handles cross-crate inlining
- Pre-computed mono x_offset — not the bottleneck
- Reducing frame latency — makes GPU wait more

### Dead ends
- Micro-optimizing per-cell String allocation (the real cost is glyphon Buffer creation)
- Explicit inlining hints (compiler does better job)
