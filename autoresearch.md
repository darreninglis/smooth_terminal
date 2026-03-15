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
- `src/config/mod.rs`, `src/input/mod.rs`, `src/menubar/mod.rs`
- `src/terminal/grid.rs`, `src/terminal/parser.rs`
- Test files, `Cargo.toml`

## Context
- GPU-accelerated terminal emulator using wgpu (Metal on macOS), glyphon for text
- Benchmark fills entire terminal grid with colored text and reshapes every frame (worst case)

---

## Results

| Run | Description | Metric | Status | Notes |
|-----|-------------|--------|--------|-------|
| 0 | baseline | 14971µs | kept | |
| 2 | Shaping::Basic instead of Advanced | 13132µs | kept | -12.3% |
| 4 | pre-allocate span buffer Vec | 10711µs | kept | -18.4% |
| 7 | lazy hex color detection | 10388µs | kept | -3% |
| 8 | remove redundant empty-row scan | 9989µs | kept | -3.8% |
| 10 | avoid cloning font_family String | 9657µs | kept | -3.3% |
| 16 | **row-level Buffers + set_rich_text** | **2271µs** | **kept** | **-76.5%** |
| 17 | row-level Buffer for scrollback | 1895µs | kept | -16.5% |
| 18 | reuse span Vecs + inline total_cols | 1406µs | kept | -25.8% |
| 19 | remove redundant shape_until_scroll | 1060µs | kept | -24.6% |
| 20 | skip set_size before set_rich_text | 1035µs | kept | -2.4% |

**Current best: 1035µs** (was 14971µs baseline — **93.1% total reduction**)

## Learnings

### What worked
- **Row-level Buffers with set_rich_text()** — THE breakthrough (76.5% reduction). One Buffer per row instead of per cell reduces Buffer::new() + shape_until_scroll() calls from ~1920 to ~24
- **Removing double shape_until_scroll** — set_rich_text() already calls it internally, so the explicit call was shaping text twice
- **Shaping::Basic** over Advanced — no complex text layout needed for terminal
- **Pre-allocating Vec capacity** — avoids reallocation during span building
- **Reusing Vecs across rows** — avoid reallocating spans/rich_spans per row
- **Computing total_cols inline** — avoid allocating a temporary String just to count columns
- **Lazy hex color detection** — skip scanning rows without '#'
- **Using references** instead of cloning Strings

### What didn't work
- Per-cell micro-optimizations (to_string, HashMap caching, inlining)
- Reducing GPU frame latency or codegen units
- Batching same-color cells into multi-char Buffers (before set_rich_text)

### Dead ends
- Micro-optimizing per-cell allocation — the solution was reducing the NUMBER of Buffers, not making each one slightly cheaper
