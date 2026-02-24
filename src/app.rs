use crate::config::Config;
use crate::input::{handle_key_event, handle_scroll, InputAction};
use crate::pane::layout::Rect;
use crate::pane::PaneTree;
use crate::renderer::Renderer;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pane_tree: Option<PaneTree>,
    config: Config,
    last_frame: Instant,
    modifiers: ModifiersState,
}

impl App {
    pub fn new(config: Config) -> Self {
        Self {
            window: None,
            renderer: None,
            pane_tree: None,
            config,
            last_frame: Instant::now(),
            modifiers: ModifiersState::empty(),
        }
    }

    fn window_size_rect(&self) -> Rect {
        if let Some(w) = &self.window {
            let size = w.inner_size();
            Rect::new(0.0, 0.0, size.width as f32, size.height as f32)
        } else {
            Rect::new(0.0, 0.0, 1200.0, 800.0)
        }
    }

    /// Content rect: window rect inset by the configured padding (converted to
    /// physical pixels via the current DPI scale factor).
    fn content_rect(&self) -> Rect {
        let base = self.window_size_rect();
        let scale = self.window
            .as_ref()
            .map(|w| w.scale_factor() as f32)
            .unwrap_or(1.0);
        let pad = self.config.window.padding * scale;
        Rect::new(
            base.x + pad,
            base.y + pad,
            (base.width - 2.0 * pad).max(1.0),
            (base.height - 2.0 * pad).max(1.0),
        )
    }

    fn cell_dims(&self) -> (f32, f32) {
        if let Some(r) = &self.renderer {
            (r.cell_w, r.cell_h)
        } else {
            (8.4, 16.8)
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title(concat!("smooth terminal ", env!("BUILD_NUMBER")))
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.window.width,
                self.config.window.height,
            ))
            .with_transparent(true);

        let window = Arc::new(
            event_loop.create_window(attrs).expect("create window"),
        );

        let renderer = Renderer::new(window.clone(), self.config.clone());
        let (cell_w, cell_h) = (renderer.cell_w, renderer.cell_h);
        let scale = window.scale_factor() as f32;
        let pad = self.config.window.padding * scale;
        let size = window.inner_size();
        let cols = (((size.width as f32) - 2.0 * pad) / cell_w).floor() as usize;
        let rows = (((size.height as f32) - 2.0 * pad) / cell_h).floor() as usize;
        let cols = cols.max(1);
        let rows = rows.max(1);

        let pane_tree = PaneTree::new(cols, rows).expect("create pane tree");

        #[cfg(target_os = "macos")]
        crate::menubar::setup_menubar();

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.pane_tree = Some(pane_tree);
        self.last_frame = Instant::now();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size.width, new_size.height);
                }
                // Compute content_rect before mutably borrowing pane_tree.
                let (cw, ch) = self.renderer.as_ref()
                    .map(|r| (r.cell_w, r.cell_h))
                    .unwrap_or((8.4, 16.8));
                let rect = self.content_rect();
                if let Some(pane_tree) = &mut self.pane_tree {
                    let layout_rects = pane_tree.layout.compute_rects(rect);
                    pane_tree.resize_panes(&layout_rects, cw, ch);
                }
            }

            WindowEvent::ModifiersChanged(new_mods) => {
                self.modifiers = new_mods.state();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                let action = handle_key_event(&event, self.modifiers);
                match action {
                    InputAction::WriteBytes(bytes) => {
                        if let Some(pt) = &mut self.pane_tree {
                            if let Some(pane) = pt.focused_pane_mut() {
                                let _ = pane.terminal.write_input(&bytes);
                            }
                        }
                    }
                    InputAction::SplitHorizontal => {
                        let rect = self.content_rect();
                        let (cw, ch) = self.cell_dims();
                        if let Some(pt) = &mut self.pane_tree {
                            let _ = pt.split_horizontal(cw, ch, rect);
                        }
                    }
                    InputAction::SplitVertical => {
                        let rect = self.content_rect();
                        let (cw, ch) = self.cell_dims();
                        if let Some(pt) = &mut self.pane_tree {
                            let _ = pt.split_vertical(cw, ch, rect);
                        }
                    }
                    InputAction::ClosePane => {
                        if let Some(pt) = &mut self.pane_tree {
                            pt.close_focused();
                            if pt.panes.is_empty() {
                                event_loop.exit();
                            }
                        }
                    }
                    InputAction::FocusNext => {
                        if let Some(pt) = &mut self.pane_tree {
                            pt.focus_next();
                        }
                    }
                    InputAction::FocusPrev => {
                        if let Some(pt) = &mut self.pane_tree {
                            pt.focus_prev();
                        }
                    }
                    InputAction::None => {}
                    InputAction::Scroll(_) => {}
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self.window.as_ref().map(|w| w.scale_factor()).unwrap_or(1.0);
                let dy = handle_scroll(delta, scale);
                let focused = self.pane_tree.as_ref().map(|pt| pt.focused_id);
                if let (Some(focused), Some(renderer)) = (focused, &mut self.renderer) {
                    renderer.ensure_pane_state(focused);
                    if let Some(spring) = renderer.scroll_springs.get_mut(&focused) {
                        spring.scroll_by(-dy);
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
                self.last_frame = now;

                // Drain PTY output
                if let Some(pt) = &mut self.pane_tree {
                    pt.drain_all_pty_output();
                }

                // Update cursor spring targets
                let rect = self.content_rect();
                if let (Some(pt), Some(renderer)) = (&self.pane_tree, &mut self.renderer) {
                    let layout_rects = pt.layout.compute_rects(rect);
                    for (pane_id, pane_rect) in &layout_rects {
                        if let Some(pane) = pt.panes.iter().find(|p| p.id == *pane_id) {
                            let grid = pane.terminal.grid.lock();
                            let col = grid.cursor_col;
                            let row = grid.cursor_row;
                            drop(grid);
                            renderer.update_cursor_for_pane(*pane_id, col, row, *pane_rect);
                        }
                    }
                }

                // Tick animations
                if let Some(renderer) = &mut self.renderer {
                    renderer.tick_animations(dt);
                }

                // Render
                let rect = self.content_rect();
                if let (Some(renderer), Some(pt)) = (&mut self.renderer, &self.pane_tree) {
                    match renderer.render(pt, rect) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            if let Some(w) = &self.window {
                                let s = w.inner_size();
                                renderer.resize(s.width, s.height);
                            }
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            log::error!("Out of GPU memory");
                            event_loop.exit();
                        }
                        Err(e) => {
                            log::warn!("Surface error: {:?}", e);
                        }
                    }
                }
            }

            _ => {}
        }
    }
}
