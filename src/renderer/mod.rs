pub mod background;
pub mod cell_bg;
pub mod cursor;
pub mod text_renderer;

use crate::animation::scroll::ScrollSpring;
use crate::config::{parse_hex_color, Config};
use crate::pane::layout::Rect;
use crate::pane::PaneTree;
use crate::renderer::background::BackgroundRenderer;
use crate::renderer::cell_bg::{cell_quad_vertices, CellBgRenderer, CellBgVertex};
use crate::renderer::cursor::CursorAnimator;
use crate::renderer::text_renderer::{
    build_scrollback_span_buffers, build_span_buffers, to_glyphon_color, PaneTextRenderer,
    SpanBuffer,
};
use glyphon::{TextArea, TextBounds};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::SurfaceError;
use winit::window::Window;

const DEFAULT_CURSOR_COLOR: [f32; 4] = [0.75, 0.0, 1.0, 1.0];

/// A selected region in absolute-row coordinates.
/// abs_row = 0..scrollback_len   → scrollback row
/// abs_row = scrollback_len..    → visible row (abs_row - scrollback_len)
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub anchor: (usize, usize), // (abs_row, col)
    pub head: (usize, usize),
}

impl Selection {
    /// Returns (start, end) in (abs_row, col) order.
    pub fn normalized(&self) -> ((usize, usize), (usize, usize)) {
        if (self.anchor.0, self.anchor.1) <= (self.head.0, self.head.1) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,

    pub cell_bg_renderer: CellBgRenderer,
    /// Separate renderer for post-text overlay quads (pane borders).
    /// Must NOT share a vertex buffer with cell_bg_renderer: wgpu batches all
    /// write_buffer calls before any GPU draw executes, so multiple writes to
    /// the same buffer in one frame are collapsed to the last write.
    pub border_renderer: CellBgRenderer,
    pub text_renderer: PaneTextRenderer,
    pub background_renderer: Option<BackgroundRenderer>,

    pub cursor_animators: HashMap<usize, CursorAnimator>,
    /// Per-pane cursor visibility (DECTCEM). TUI apps hide the terminal cursor.
    pub cursor_visible: HashMap<usize, bool>,
    pub scroll_springs: HashMap<usize, ScrollSpring>,
    /// Per-pane visible span-buffer cache. Key = pane_id, Value = (grid generation, buffers).
    text_cache: HashMap<usize, (u64, Vec<SpanBuffer>)>,
    /// Per-pane scrollback span-buffer cache. Key = pane_id,
    /// Value = ((scrollback_len, first_abs_row), buffers).
    scrollback_text_cache: HashMap<usize, ((usize, usize), Vec<SpanBuffer>)>,

    pub cell_w: f32,
    pub cell_h: f32,
    pub font_size_px: f32,
    pub scale_factor: f32,
    pub app_config: Config,
}

impl Renderer {
    pub fn new(window: Arc<Window>, app_config: Config) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).expect("create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("request adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            },
        ))
        .expect("request device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let alpha_mode = surface_caps
            .alpha_modes
            .iter()
            .find(|m| **m == wgpu::CompositeAlphaMode::PreMultiplied)
            .copied()
            .unwrap_or(surface_caps.alpha_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let cell_bg_renderer = CellBgRenderer::new(&device, surface_format);
        let border_renderer = CellBgRenderer::new(&device, surface_format);
        let mut text_renderer = PaneTextRenderer::new(&device, &queue, surface_format);

        // Load background image if configured
        let background_renderer = app_config.background.image_path.as_ref().and_then(|path| {
            let opacity = app_config.background.image_opacity.unwrap_or(0.3);
            match image::open(path) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    Some(BackgroundRenderer::new(
                        &device,
                        &queue,
                        surface_format,
                        rgba.as_raw(),
                        w,
                        h,
                        opacity,
                    ))
                }
                Err(e) => {
                    log::warn!("Failed to load background image: {}", e);
                    None
                }
            }
        });

        // Scale font to physical pixels (Retina = 2.0x, standard = 1.0x)
        let scale_factor = window.scale_factor() as f32;
        let font_size_px = app_config.font.size * scale_factor;
        let line_height = app_config.font.line_height;
        let cell_h = font_size_px * line_height;

        // Measure the actual advance width of a monospace character
        let cell_w = measure_cell_width(
            &mut text_renderer.font_system,
            font_size_px,
            cell_h,
            &app_config.font.family,
        );

        Self {
            surface,
            device,
            queue,
            config,
            surface_format,
            cell_bg_renderer,
            border_renderer,
            text_renderer,
            background_renderer,
            cursor_animators: HashMap::new(),
            cursor_visible: HashMap::new(),
            scroll_springs: HashMap::new(),
            text_cache: HashMap::new(),
            scrollback_text_cache: HashMap::new(),
            cell_w,
            cell_h,
            font_size_px,
            scale_factor,
            app_config,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn ensure_pane_state(&mut self, pane_id: usize) {
        let cursor_freq = self.app_config.animation.cursor_spring_frequency;
        let scroll_freq = self.app_config.animation.scroll_spring_frequency;
        let cursor_color = parse_hex_color(&self.app_config.colors.cursor)
            .unwrap_or(DEFAULT_CURSOR_COLOR);
        let trail = self.app_config.animation.cursor_trail_enabled;

        self.cursor_animators.entry(pane_id).or_insert_with(|| {
            CursorAnimator::new(cursor_freq, cursor_color, self.cell_w, self.cell_h, trail)
        });
        self.scroll_springs.entry(pane_id).or_insert_with(|| {
            ScrollSpring::new(scroll_freq)
        });
    }

    pub fn tick_animations(&mut self, dt: f32) {
        for anim in self.cursor_animators.values_mut() {
            anim.tick(dt);
        }
        for spring in self.scroll_springs.values_mut() {
            spring.tick(dt);
        }
    }

    pub fn render(
        &mut self,
        pane_tree: &PaneTree,
        window_rect: Rect,
        selection: Option<(usize, &Selection)>, // (focused_pane_id, selection)
        hovered_url: Option<(usize, usize, usize, usize)>, // (pane_id, abs_row, col_start, col_end)
    ) -> Result<(), SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let surface_w = self.config.width as f32;
        let surface_h = self.config.height as f32;

        let bg_color = parse_hex_color(&self.app_config.colors.background)
            .unwrap_or([0.118, 0.118, 0.18, 1.0]);
        let window_opacity = self.app_config.window.opacity;
        let fg_color = parse_hex_color(&self.app_config.colors.foreground)
            .unwrap_or([0.8, 0.84, 0.96, 1.0]);
        let palette = self.app_config.colors.ansi_palette();

        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });

        // Clear pass
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: (bg_color[0] * window_opacity) as f64,
                            g: (bg_color[1] * window_opacity) as f64,
                            b: (bg_color[2] * window_opacity) as f64,
                            a: window_opacity as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // Background image pass
        if let Some(bg) = &self.background_renderer {
            bg.render(&mut encoder, &view);
        }

        // Compute layout rects
        let layout_rects = pane_tree.layout.compute_rects(window_rect);

        // Ensure animation state for all panes
        for (pane_id, _) in &layout_rects {
            self.ensure_pane_state(*pane_id);
        }

        // ---- Update scroll max_offsets and build text caches ----
        let cell_w = self.cell_w;
        let cell_h = self.cell_h;
        let font_size_px = self.font_size_px;
        let font_family = self.app_config.font.family.clone();

        for (pane_id, pane_rect) in &layout_rects {
            let pane = match pane_tree.panes.iter().find(|p| p.id == *pane_id) {
                Some(p) => p,
                None => continue,
            };
            let grid = pane.terminal.grid.lock();
            let scrollback_len = grid.scrollback.len();
            let visible_rows = grid.rows;
            let current_gen = grid.generation;

            // Update scroll spring max_offset from actual scrollback size
            if let Some(spring) = self.scroll_springs.get_mut(pane_id) {
                spring.max_offset = scrollback_len as f32 * cell_h;
            }

            let scroll_offset = self.scroll_springs
                .get(pane_id)
                .map(|s| s.pixel_offset())
                .unwrap_or(0.0);

            // Rebuild visible span buffer cache if grid changed
            if !self.text_cache.get(pane_id).map_or(false, |(g, _)| *g == current_gen) {
                let span_buffers = build_span_buffers(
                    &mut self.text_renderer.font_system,
                    &grid,
                    cell_h,
                    font_size_px,
                    &font_family,
                    cell_w,
                    fg_color,
                    &palette,
                );
                self.text_cache.insert(*pane_id, (current_gen, span_buffers));
            }

            // Rebuild scrollback span buffer cache if scrolled and cache is stale
            if scroll_offset > 0.5 && scrollback_len > 0 {
                // Determine which scrollback rows are visible:
                // y = pane_rect.y + row_idx * cell_h + scroll_offset
                // row_idx = abs_row - scrollback_len (negative for scrollback)
                // Visible: y >= pane_rect.y - cell_h  &&  y < pane_rect.y + pane_rect.height + cell_h
                let rows_above = (scroll_offset / cell_h).ceil() as usize;
                let first_abs = scrollback_len.saturating_sub(rows_above + visible_rows);
                let last_abs = scrollback_len; // exclusive

                let cache_key = (scrollback_len, first_abs);
                let cache_hit = self
                    .scrollback_text_cache
                    .get(pane_id)
                    .map_or(false, |(k, _)| *k == cache_key);

                if !cache_hit {
                    let rows_slice = &grid.scrollback[first_abs..last_abs];
                    let sb_buffers = build_scrollback_span_buffers(
                        &mut self.text_renderer.font_system,
                        rows_slice,
                        first_abs,
                        scrollback_len,
                        cell_h,
                        font_size_px,
                        &font_family,
                        cell_w,
                        fg_color,
                        &palette,
                    );
                    self.scrollback_text_cache.insert(*pane_id, (cache_key, sb_buffers));
                }
            } else {
                // Not scrolled: evict scrollback cache to free memory
                self.scrollback_text_cache.remove(pane_id);
            }

            drop(grid);
        }

        // Border padding: panes that don't start at the window edge have a separator line;
        // content is inset by BORDER_W + BORDER_PAD so text clears the border visually.
        const BORDER_W: f32 = 1.0;
        const BORDER_PAD: f32 = 8.0;
        const BORDER_TOTAL: f32 = BORDER_W + BORDER_PAD;
        let content_x = |px: f32| if px > window_rect.x + 0.5 { px + BORDER_TOTAL } else { px };
        let content_y = |py: f32| if py > window_rect.y + 0.5 { py + BORDER_TOTAL } else { py };

        // ---- Phase 1+2: Selection highlights + cursor block (single batch) ----
        // CellBgRenderer uses a shared vertex buffer. All write_buffer calls submitted
        // in one frame are applied before any GPU draw executes, so the last write wins.
        // Batching selection and cursor into one render call avoids clobbering either.
        let mut bg_vertices: Vec<CellBgVertex> = Vec::new();

        if let Some((sel_pane_id, sel)) = selection {
            if !sel.is_empty() {
                if let Some(pane_rect) = layout_rects.iter().find(|(id, _)| *id == sel_pane_id).map(|(_, r)| r) {
                    if let Some(pane) = pane_tree.panes.iter().find(|p| p.id == sel_pane_id) {
                        let grid = pane.terminal.grid.lock();
                        let scrollback_len = grid.scrollback.len();
                        let visible_rows = grid.rows;
                        let cols = grid.cols;
                        drop(grid);

                        let scroll_offset = self.scroll_springs
                            .get(&sel_pane_id)
                            .map(|s| s.pixel_offset())
                            .unwrap_or(0.0);

                        let sel_color = [0.3_f32, 0.5, 0.9, 0.4];
                        let (start, end) = sel.normalized();
                        let total_rows = scrollback_len + visible_rows;
                        let cx = content_x(pane_rect.x);

                        for abs_row in start.0..=end.0.min(total_rows.saturating_sub(1)) {
                            let row_idx = abs_row as f32 - scrollback_len as f32;
                            let y = pane_rect.y + row_idx * cell_h + scroll_offset;

                            // Skip rows outside the pane
                            if y + cell_h < pane_rect.y || y > pane_rect.y + pane_rect.height {
                                continue;
                            }

                            let col_start = if abs_row == start.0 { start.1 } else { 0 };
                            let col_end = if abs_row == end.0 { end.1 } else { cols.saturating_sub(1) };
                            let col_end = col_end.min(cols.saturating_sub(1));

                            for col in col_start..=col_end {
                                let x = cx + col as f32 * cell_w;
                                let verts = cell_quad_vertices(
                                    x, y, cell_w, cell_h,
                                    sel_color,
                                    surface_w, surface_h,
                                );
                                bg_vertices.extend_from_slice(&verts);
                                if bg_vertices.len() / 4 >= self.cell_bg_renderer.max_quads() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Cursor block — always rendered for the focused pane.
        // All PTY output is drained before rendering, so by this point the
        // cursor position is stable (at the input area, not mid-render-cycle).
        // We ignore DECTCEM (cursor_visible) because TUI apps like Claude Code
        // hide the terminal cursor to draw their own styled text cursor, but we
        // want our GPU-animated cursor to always appear at the active position.
        let focused_id = pane_tree.focused_id;
        if let Some(anim) = self.cursor_animators.get(&focused_id) {
            let verts = anim.build_vertices(surface_w, surface_h);
            bg_vertices.extend_from_slice(&verts);
        }

        // Hovered URL underline
        if let Some((url_pane_id, url_abs_row, url_col_start, url_col_end)) = hovered_url {
            if let Some(pane_rect) = layout_rects.iter().find(|(id, _)| *id == url_pane_id).map(|(_, r)| r) {
                if let Some(pane) = pane_tree.panes.iter().find(|p| p.id == url_pane_id) {
                    let grid = pane.terminal.grid.lock();
                    let scrollback_len = grid.scrollback.len();
                    drop(grid);

                    let scroll_offset = self.scroll_springs
                        .get(&url_pane_id)
                        .map(|s| s.pixel_offset())
                        .unwrap_or(0.0);

                    let row_idx = url_abs_row as f32 - scrollback_len as f32;
                    let y = pane_rect.y + row_idx * cell_h + scroll_offset;
                    let underline_h = 2.0_f32;
                    let underline_y = y + cell_h - underline_h;
                    let cx = content_x(pane_rect.x);
                    let underline_color = [fg_color[0], fg_color[1], fg_color[2], 0.6];

                    if underline_y + underline_h >= pane_rect.y && underline_y < pane_rect.y + pane_rect.height {
                        for col in url_col_start..url_col_end {
                            let x = cx + col as f32 * cell_w;
                            let verts = cell_quad_vertices(
                                x, underline_y, cell_w, underline_h,
                                underline_color,
                                surface_w, surface_h,
                            );
                            bg_vertices.extend_from_slice(&verts);
                        }
                    }
                }
            }
        }

        let quad_count = bg_vertices.len() / 4;
        if quad_count > 0 {
            self.cell_bg_renderer.render(
                &mut encoder, &view, &self.queue, &bg_vertices, quad_count,
            );
        }

        // ---- Phase 3b: Build TextAreas from the caches ----
        // y formula: y = cy + row_idx * cell_h + scroll_offset
        // (scroll_offset > 0 → content moves down to reveal scrollback from top)
        // cx/cy are the content-inset origins accounting for any left/top border padding.
        let default_color = to_glyphon_color(fg_color);
        let mut text_areas: Vec<TextArea> = Vec::new();

        for (pane_id, pane_rect) in &layout_rects {
            let scroll_offset = self.scroll_springs
                .get(pane_id)
                .map(|s| s.pixel_offset())
                .unwrap_or(0.0);

            let cx = content_x(pane_rect.x);
            let cy = content_y(pane_rect.y);

            let bounds = TextBounds {
                left: cx as i32,
                top: cy as i32,
                right: (pane_rect.x + pane_rect.width) as i32,
                bottom: (pane_rect.y + pane_rect.height) as i32,
            };

            // Visible rows
            if let Some((_, span_buffers)) = self.text_cache.get(pane_id) {
                for sb in span_buffers {
                    let y = cy + sb.row_idx as f32 * cell_h + scroll_offset;
                    if y + cell_h < pane_rect.y || y > pane_rect.y + pane_rect.height {
                        continue;
                    }
                    let x = cx + sb.col_start as f32 * cell_w + sb.x_offset;
                    text_areas.push(TextArea {
                        buffer: &sb.buffer,
                        left: x,
                        top: y,
                        scale: 1.0,
                        bounds,
                        default_color,
                        custom_glyphs: &[],
                    });
                }
            }

            // Scrollback rows (row_idx < 0, only when scrolled)
            if scroll_offset > 0.5 {
                if let Some((_, sb_buffers)) = self.scrollback_text_cache.get(pane_id) {
                    for sb in sb_buffers {
                        let y = cy + sb.row_idx as f32 * cell_h + scroll_offset;
                        if y + cell_h < pane_rect.y || y > pane_rect.y + pane_rect.height {
                            continue;
                        }
                        let x = cx + sb.col_start as f32 * cell_w + sb.x_offset;
                        text_areas.push(TextArea {
                            buffer: &sb.buffer,
                            left: x,
                            top: y,
                            scale: 1.0,
                            bounds,
                            default_color,
                            custom_glyphs: &[],
                        });
                    }
                }
            }
        }

        if !text_areas.is_empty() {
            let _ = self.text_renderer.prepare(
                &self.device,
                &self.queue,
                self.config.width,
                self.config.height,
                text_areas,
            );

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("text_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let _ = self.text_renderer.render(&mut pass);
        }

        // ---- Phase 4: Pane separator borders ----
        if layout_rects.len() > 1 {
            let border_color = [fg_color[0] * 0.4, fg_color[1] * 0.4, fg_color[2] * 0.4, 0.4];
            let mut border_verts: Vec<CellBgVertex> = Vec::new();

            for (_, pane_rect) in &layout_rects {
                if pane_rect.x > window_rect.x + 0.5 {
                    let verts = cell_quad_vertices(
                        pane_rect.x, pane_rect.y,
                        BORDER_W, pane_rect.height,
                        border_color,
                        surface_w, surface_h,
                    );
                    border_verts.extend_from_slice(&verts);
                }
                if pane_rect.y > window_rect.y + 0.5 {
                    let verts = cell_quad_vertices(
                        pane_rect.x, pane_rect.y,
                        pane_rect.width, BORDER_W,
                        border_color,
                        surface_w, surface_h,
                    );
                    border_verts.extend_from_slice(&verts);
                }
            }

            let quad_count = border_verts.len() / 4;
            if quad_count > 0 {
                self.border_renderer.render(
                    &mut encoder, &view, &self.queue, &border_verts, quad_count,
                );
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.text_renderer.trim_atlas();
        Ok(())
    }

    pub fn update_cursor_for_pane(
        &mut self,
        pane_id: usize,
        col: usize,
        row: usize,
        pane_rect: Rect,
    ) {
        let scroll_offset = self.scroll_springs
            .get(&pane_id)
            .map(|s| s.pixel_offset())
            .unwrap_or(0.0);
        self.ensure_pane_state(pane_id);
        if let Some(anim) = self.cursor_animators.get_mut(&pane_id) {
            anim.set_cell_size(self.cell_w, self.cell_h);
            if anim.is_warming_up() {
                anim.snap_to(col, row, pane_rect.x, pane_rect.y, scroll_offset);
            } else if anim.target_col != col || anim.target_row != row {
                // Only snap for large jumps (>5 cells in either axis) so the
                // spring can animate smoothly during normal typing and small
                // cursor movements. Big jumps (page up/down, search, etc.)
                // still snap to avoid a long slide across the screen.
                let rendered_x = anim.corners[0].x.position;
                let rendered_y = anim.corners[0].y.position;
                let new_target_x = pane_rect.x + col as f32 * self.cell_w;
                let new_target_y = pane_rect.y + row as f32 * self.cell_h + scroll_offset;
                let dx = (rendered_x - new_target_x).abs();
                let dy = (rendered_y - new_target_y).abs();
                if dx > self.cell_w * 5.0 || dy > self.cell_h * 5.0 {
                    anim.snap_to(col, row, pane_rect.x, pane_rect.y, scroll_offset);
                } else {
                    anim.move_to(col, row, pane_rect.x, pane_rect.y, scroll_offset);
                    // Keep the cursor within 1 cell of the target so it never
                    // visibly lags behind typed text during fast input.
                    anim.clamp_lag(self.cell_w, self.cell_h);
                }
            }
        }
    }

    /// Apply updated config values and/or DPI scale changes. Returns true if
    /// cell metrics changed (caller must then resize panes).
    pub fn apply_config(&mut self, new_config: Config, scale_factor: f32) -> bool {
        let font_changed = new_config.font.family != self.app_config.font.family
            || (new_config.font.size - self.app_config.font.size).abs() > 0.01
            || (new_config.font.line_height - self.app_config.font.line_height).abs() > 0.01;
        let scale_changed = (scale_factor - self.scale_factor).abs() > 0.001;
        let metrics_changed = font_changed || scale_changed;

        self.app_config = new_config;

        let cursor_color = parse_hex_color(&self.app_config.colors.cursor)
            .unwrap_or(DEFAULT_CURSOR_COLOR);
        for anim in self.cursor_animators.values_mut() {
            anim.color = cursor_color;
        }

        if metrics_changed {
            let font_size_px = self.app_config.font.size * scale_factor;
            let cell_h = font_size_px * self.app_config.font.line_height;
            let cell_w = measure_cell_width(
                &mut self.text_renderer.font_system,
                font_size_px,
                cell_h,
                &self.app_config.font.family,
            );
            self.font_size_px = font_size_px;
            self.cell_h = cell_h;
            self.cell_w = cell_w;
            self.scale_factor = scale_factor;
            for anim in self.cursor_animators.values_mut() {
                anim.set_cell_size(cell_w, cell_h);
            }
        }

        // Always clear text cache — forces re-shaping with new colors and/or font
        self.text_cache.clear();
        self.scrollback_text_cache.clear();

        metrics_changed
    }

    pub fn set_cursor_visible(&mut self, pane_id: usize, visible: bool) {
        self.cursor_visible.insert(pane_id, visible);
    }

    pub fn snap_cursor_for_pane(
        &mut self,
        pane_id: usize,
        col: usize,
        row: usize,
        pane_rect: Rect,
    ) {
        let scroll_offset = self.scroll_springs
            .get(&pane_id)
            .map(|s| s.pixel_offset())
            .unwrap_or(0.0);
        self.ensure_pane_state(pane_id);
        if let Some(anim) = self.cursor_animators.get_mut(&pane_id) {
            anim.set_cell_size(self.cell_w, self.cell_h);
            anim.snap_to(col, row, pane_rect.x, pane_rect.y, scroll_offset);
        }
    }
}

/// Shape a single "M" character at the given physical-pixel font size and return
/// its advance width. This gives us the true cell width for the loaded font.
fn measure_cell_width(font_system: &mut glyphon::FontSystem, font_size_px: f32, cell_h: f32, font_family: &str) -> f32 {
    use glyphon::{Attrs, Buffer, Family, Metrics, Shaping};
    let family = if font_family.is_empty() { Family::Monospace } else { Family::Name(font_family) };
    let metrics = Metrics::new(font_size_px, cell_h);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(font_system, None, None);
    buffer.set_text(font_system, "M", &Attrs::new().family(family), Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);
    buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.iter())
        .map(|g| g.w)
        .next()
        .unwrap_or(font_size_px * 0.6)
}
