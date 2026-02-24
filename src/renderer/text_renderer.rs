use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextRenderer as GlyphonTextRenderer, Viewport,
};
use unicode_width::UnicodeWidthChar;

pub struct PaneTextRenderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub atlas: TextAtlas,
    pub viewport: Viewport,
    pub text_renderer: GlyphonTextRenderer,
}

impl PaneTextRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let text_renderer = GlyphonTextRenderer::new(
            &mut atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );

        Self {
            font_system,
            swash_cache,
            atlas,
            viewport,
            text_renderer,
        }
    }

    pub fn prepare<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_width: u32,
        surface_height: u32,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
    ) -> Result<(), glyphon::PrepareError> {
        self.viewport.update(
            queue,
            Resolution {
                width: surface_width,
                height: surface_height,
            },
        );
        self.text_renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        )
    }

    pub fn render<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
    ) -> Result<(), glyphon::RenderError> {
        self.text_renderer.render(&self.atlas, &self.viewport, pass)
    }

    pub fn trim_atlas(&mut self) {
        self.atlas.trim();
    }
}

/// One color-run of text anchored to an explicit column position.
/// Positioning each span at col_start * cell_w (rather than relying on
/// accumulated font advances) ensures the cursor — also at col * cell_w —
/// never drifts away from the rendered glyphs.
pub struct SpanBuffer {
    pub buffer: Buffer,
    /// Terminal column where this span starts (used to compute left = col * cell_w)
    pub col_start: usize,
    pub row_idx: usize,
    /// Horizontal offset (pixels) to center the glyph within its cell.
    pub x_offset: f32,
}

/// Build per-cell glyphon Buffers for a terminal grid.
///
/// Each visible cell gets its own single-character Buffer placed at exactly
/// `col * cell_w`.  This guarantees the animated cursor — also positioned at
/// `col * cell_w` — is always pixel-perfectly aligned with the rendered glyph,
/// regardless of how the font's actual advance widths compare to `cell_w`.
///
/// The previous approach batched consecutive same-color cells into one Buffer
/// anchored at `col_start * cell_w`, but accumulated glyph-advance rounding
/// within a long span caused the cursor to drift by an amount proportional to
/// the span's length (visible as the cursor being offset by the directory-name
/// portion of the shell prompt).
pub fn build_span_buffers(
    font_system: &mut FontSystem,
    grid: &crate::terminal::grid::TerminalGrid,
    cell_h: f32,
    font_size: f32,
    font_family: &str,
    cell_w: f32,
    fg_color: [f32; 4],
    palette: &[[f32; 4]; 16],
) -> Vec<SpanBuffer> {
    let metrics = Metrics::new(font_size, cell_h);
    // Shaping::Advanced enables proper multi-font fallback so that any
    // characters not in the primary face (e.g. Nerd Font / Powerline glyphs)
    // are resolved from system fonts rather than rendering as artefacts.
    // Family::Monospace is the fallback when no name is configured.
    let family = if font_family.is_empty() {
        Family::Monospace
    } else {
        Family::Name(font_family)
    };
    let mut result = Vec::new();

    for (row_idx, row) in grid.cells.iter().enumerate() {
        if row.iter().all(|c| c.is_empty()) {
            continue;
        }

        for (col_idx, cell) in row.iter().enumerate() {
            // Skip empty cells (space / NUL) — rendered as background only.
            if cell.is_empty() {
                continue;
            }
            // Skip control characters — they have no visible glyph and would
            // produce glyphon atlas artefacts (spurious horizontal lines, etc.)
            if cell.ch.is_control() {
                continue;
            }

            let raw_fg = if cell.attrs.reverse {
                resolve_color(&cell.attrs.bg, fg_color, palette)
            } else {
                resolve_color(&cell.attrs.fg, fg_color, palette)
            };
            let cell_color = to_glyphon_color(raw_fg);

            // One character per Buffer, placed at exactly col * cell_w.
            // Wide (double-width) chars are given 2 × cell_w so they are not
            // clipped; normal chars get cell_w + a one-cell safety margin.
            let char_cols = cell.ch.width().unwrap_or(1).max(1);
            let buf_w = cell_w * (char_cols as f32 + 1.0);

            let mut buffer = Buffer::new(font_system, metrics);
            buffer.set_size(font_system, Some(buf_w), Some(cell_h));
            let attrs = Attrs::new().color(cell_color).family(family);
            buffer.set_text(font_system, &cell.ch.to_string(), &attrs, Shaping::Advanced);
            buffer.shape_until_scroll(font_system, false);

            // Center the glyph horizontally within its cell by computing the
            // difference between the cell width and the actual glyph advance.
            let glyph_advance: f32 = buffer
                .layout_runs()
                .flat_map(|run| run.glyphs.iter())
                .map(|g| g.w)
                .sum();
            let cell_span = cell_w * char_cols as f32;
            let x_offset = ((cell_span - glyph_advance) / 2.0).max(0.0);

            result.push(SpanBuffer {
                buffer,
                col_start: col_idx,
                row_idx,
                x_offset,
            });
        }
    }

    result
}

pub fn to_glyphon_color(c: [f32; 4]) -> Color {
    Color::rgba(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        (c[3] * 255.0) as u8,
    )
}

pub fn resolve_color(
    color: &crate::terminal::cell::Color,
    default_fg: [f32; 4],
    palette: &[[f32; 4]; 16],
) -> [f32; 4] {
    match color {
        crate::terminal::cell::Color::Default => default_fg,
        crate::terminal::cell::Color::Indexed(i) => {
            let i = *i as usize;
            if i < 16 {
                palette[i]
            } else {
                xterm256_to_rgba(i as u8)
            }
        }
        crate::terminal::cell::Color::Rgb(r, g, b) => {
            [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0]
        }
    }
}

fn xterm256_to_rgba(i: u8) -> [f32; 4] {
    if i < 16 {
        return [1.0, 1.0, 1.0, 1.0];
    }
    if i >= 232 {
        let gray = (i - 232) as f32 / 23.0;
        return [gray, gray, gray, 1.0];
    }
    let i = i - 16;
    let r = (i / 36) % 6;
    let g = (i / 6) % 6;
    let b = i % 6;
    let scale = |v: u8| if v == 0 { 0.0 } else { (55.0 + v as f32 * 40.0) / 255.0 };
    [scale(r), scale(g), scale(b), 1.0]
}
