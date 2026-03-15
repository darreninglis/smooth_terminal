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
- Spring-based cursor animation runs each frame but is lightweight
- The `generation` counter on TerminalGrid triggers cache rebuilds

## Ideas To Try
1. Batch glyphon text shaping — shape all panes in a single prepare() call instead of per-pane
2. Reduce SpanBuffer allocations — reuse Vec buffers across frames
3. Skip text reshape when only cursor position changed (separate cursor gen from content gen)
4. Use indexed drawing for cell_bg quads instead of per-vertex
5. Reduce mutex lock contention on TerminalGrid
6. Pre-compute color lookups (avoid re-parsing hex colors every frame)
7. Avoid recomputing layout_rects every frame when nothing changed
8. Use smaller vertex format for cell background quads
9. Profile and optimize the render pass structure (fewer passes)
10. Cache the command encoder setup between frames

---

## Results

| Run | Description | Metric | Status | Notes |
|-----|-------------|--------|--------|-------|
| 0 | baseline | 14971µs | kept | p50=14820µs p95=16072µs |

**Current best:** 14971µs

## Learnings

### What worked
(none yet)

### What didn't work
(none yet)

### Dead ends
(none yet)
