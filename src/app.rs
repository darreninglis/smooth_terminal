use crate::config::{Config, OPEN_CONFIG_REQUESTED};
use crate::input::{handle_key_event, handle_scroll, InputAction};
use crate::pane::Direction;
use crate::pane::layout::Rect;
use crate::pane::PaneTree;
use crate::renderer::Renderer;
use crossbeam_channel::Receiver;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

struct WindowState {
    window: Arc<Window>,
    renderer: Renderer,
    pane_tree: PaneTree,
    modifiers: ModifiersState,
    cursor_pos: (f32, f32),
    config_rx: Option<Receiver<()>>,
    _config_watcher: Option<RecommendedWatcher>,
    last_frame: Instant,
}

impl WindowState {
    fn window_size_rect(&self) -> Rect {
        let size = self.window.inner_size();
        Rect::new(0.0, 0.0, size.width as f32, size.height as f32)
    }

    fn content_rect(&self, config: &Config) -> Rect {
        let base = self.window_size_rect();
        let scale = self.window.scale_factor() as f32;
        let pad = config.window.padding * scale;
        Rect::new(
            base.x + pad,
            base.y + pad,
            (base.width - 2.0 * pad).max(1.0),
            (base.height - 2.0 * pad).max(1.0),
        )
    }

    fn cell_dims(&self) -> (f32, f32) {
        (self.renderer.cell_w, self.renderer.cell_h)
    }

    fn open_config_in_pane(&mut self) {
        if let Some(pane) = self.pane_tree.focused_pane_mut() {
            let path = Config::config_path();
            let cmd = format!("vim '{}'\r", path.display());
            let _ = pane.terminal.write_input(cmd.as_bytes());
        }
    }
}

pub struct App {
    windows: HashMap<WindowId, WindowState>,
    config: Config,
    // The first window ID is used as the "primary" for initial setup
    first_window_id: Option<WindowId>,
}

impl App {
    pub fn new(config: Config) -> Self {
        Self {
            windows: HashMap::new(),
            config,
            first_window_id: None,
        }
    }

    fn create_window_state(
        event_loop: &ActiveEventLoop,
        config: &Config,
    ) -> (WindowId, WindowState) {
        let attrs = WindowAttributes::default()
            .with_title(concat!("smooth terminal ", env!("BUILD_NUMBER")))
            .with_inner_size(winit::dpi::LogicalSize::new(
                config.window.width,
                config.window.height,
            ))
            .with_transparent(true);

        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let window_id = window.id();

        let renderer = Renderer::new(window.clone(), config.clone());
        let (cell_w, cell_h) = (renderer.cell_w, renderer.cell_h);
        let scale = window.scale_factor() as f32;
        let pad = config.window.padding * scale;
        let size = window.inner_size();
        let cols = (((size.width as f32) - 2.0 * pad) / cell_w).floor() as usize;
        let rows = (((size.height as f32) - 2.0 * pad) / cell_h).floor() as usize;
        let cols = cols.max(1);
        let rows = rows.max(1);

        let pane_tree = PaneTree::new(cols, rows).expect("create pane tree");

        // Set up config file watcher for hot-reload
        let config_path = Config::config_path();
        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let watch_path = config_path.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    if (event.kind.is_modify() || event.kind.is_create())
                        && event.paths.iter().any(|p| p == &watch_path)
                    {
                        let _ = tx.try_send(());
                    }
                }
            })
            .ok();
        if let Some(ref mut w) = watcher {
            if let Some(dir) = config_path.parent() {
                let _ = w.watch(dir, RecursiveMode::NonRecursive);
            }
        }

        let state = WindowState {
            window,
            renderer,
            pane_tree,
            modifiers: ModifiersState::empty(),
            cursor_pos: (0.0, 0.0),
            config_rx: Some(rx),
            _config_watcher: watcher,
            last_frame: Instant::now(),
        };

        (window_id, state)
    }

    /// Open a new tab by creating an in-process window and attaching it as a
    /// macOS native tab of the given "parent" window.
    fn open_new_tab(&mut self, event_loop: &ActiveEventLoop, parent_id: WindowId) {
        let (new_id, new_state) = Self::create_window_state(event_loop, &self.config);

        #[cfg(target_os = "macos")]
        {
            use objc2::msg_send_id;
            use objc2::rc::Retained;
            use objc2::runtime::AnyObject;
            use objc2_app_kit::{NSWindow, NSWindowOrderingMode};
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

            let parent_win = self.windows.get(&parent_id).map(|s| s.window.clone());
            if let Some(parent_arc) = parent_win {
                let parent_raw = parent_arc
                    .window_handle()
                    .ok()
                    .map(|h| h.as_raw());
                let new_raw = new_state
                    .window
                    .window_handle()
                    .ok()
                    .map(|h| h.as_raw());

                if let (
                    Some(RawWindowHandle::AppKit(parent_handle)),
                    Some(RawWindowHandle::AppKit(new_handle)),
                ) = (parent_raw, new_raw)
                {
                    unsafe {
                        // AppKitWindowHandle gives us the NSView; call [view window] to get NSWindow.
                        let parent_view = parent_handle.ns_view.as_ptr() as *const AnyObject;
                        let new_view = new_handle.ns_view.as_ptr() as *const AnyObject;

                        let parent_ns: Option<Retained<NSWindow>> =
                            msg_send_id![&*parent_view, window];
                        let new_ns: Option<Retained<NSWindow>> =
                            msg_send_id![&*new_view, window];

                        if let (Some(parent_ns), Some(new_ns)) = (parent_ns, new_ns) {
                            parent_ns.addTabbedWindow_ordered(
                                &new_ns,
                                NSWindowOrderingMode::NSWindowAbove,
                            );
                            new_ns.makeKeyAndOrderFront(None);
                        }
                    }
                }
            }
        }

        self.windows.insert(new_id, new_state);
    }

    /// Open a new standalone window (not tabbed).
    fn open_new_window(&mut self, event_loop: &ActiveEventLoop) {
        let (new_id, new_state) = Self::create_window_state(event_loop, &self.config);
        self.windows.insert(new_id, new_state);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let (window_id, state) = Self::create_window_state(event_loop, &self.config);

        #[cfg(target_os = "macos")]
        crate::menubar::setup_menubar();

        self.first_window_id = Some(window_id);
        self.windows.insert(window_id, state);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let fps = self.config.animation.target_fps.max(1) as u64;
        let frame_interval = std::time::Duration::from_millis(1000 / fps);
        let now = Instant::now();
        for state in self.windows.values() {
            if now.duration_since(state.last_frame) >= frame_interval {
                state.window.request_redraw();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.windows.remove(&window_id);
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }

            WindowEvent::Resized(new_size) => {
                if let Some(state) = self.windows.get_mut(&window_id) {
                    state.renderer.resize(new_size.width, new_size.height);
                    let rect = state.content_rect(&self.config);
                    let (cw, ch) = state.cell_dims();
                    let layout_rects = state.pane_tree.layout.compute_rects(rect);
                    state.pane_tree.resize_panes(&layout_rects, cw, ch);
                }
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(state) = self.windows.get_mut(&window_id) {
                    let rect = state.content_rect(&self.config);
                    let metrics_changed =
                        state.renderer.apply_config(self.config.clone(), scale_factor as f32);
                    if metrics_changed {
                        let layout_rects = state.pane_tree.layout.compute_rects(rect);
                        state
                            .pane_tree
                            .resize_panes(&layout_rects, state.renderer.cell_w, state.renderer.cell_h);
                    }
                }
            }

            WindowEvent::ModifiersChanged(new_mods) => {
                if let Some(state) = self.windows.get_mut(&window_id) {
                    state.modifiers = new_mods.state();
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                let modifiers = self
                    .windows
                    .get(&window_id)
                    .map(|s| s.modifiers)
                    .unwrap_or_default();
                let action = handle_key_event(&event, modifiers);
                match action {
                    InputAction::WriteBytes(bytes) => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            if let Some(pane) = state.pane_tree.focused_pane_mut() {
                                let _ = pane.terminal.write_input(&bytes);
                            }
                        }
                    }
                    InputAction::SplitHorizontal => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            let rect = state.content_rect(&self.config);
                            let (cw, ch) = state.cell_dims();
                            let _ = state.pane_tree.split_horizontal(cw, ch, rect);
                        }
                    }
                    InputAction::SplitVertical => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            let rect = state.content_rect(&self.config);
                            let (cw, ch) = state.cell_dims();
                            let _ = state.pane_tree.split_vertical(cw, ch, rect);
                        }
                    }
                    InputAction::ClosePane => {
                        let should_close_window = if let Some(state) =
                            self.windows.get_mut(&window_id)
                        {
                            state.pane_tree.close_focused();
                            state.pane_tree.panes.is_empty()
                        } else {
                            false
                        };
                        if should_close_window {
                            self.windows.remove(&window_id);
                            if self.windows.is_empty() {
                                event_loop.exit();
                            }
                        }
                    }
                    InputAction::FocusNext => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            state.pane_tree.focus_next();
                        }
                    }
                    InputAction::FocusPrev => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            state.pane_tree.focus_prev();
                        }
                    }
                    InputAction::FocusLeft => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            let rect = state.content_rect(&self.config);
                            let rects = state.pane_tree.layout.compute_rects(rect);
                            state.pane_tree.focus_direction(&rects, Direction::Left);
                        }
                    }
                    InputAction::FocusRight => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            let rect = state.content_rect(&self.config);
                            let rects = state.pane_tree.layout.compute_rects(rect);
                            state.pane_tree.focus_direction(&rects, Direction::Right);
                        }
                    }
                    InputAction::FocusUp => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            let rect = state.content_rect(&self.config);
                            let rects = state.pane_tree.layout.compute_rects(rect);
                            state.pane_tree.focus_direction(&rects, Direction::Up);
                        }
                    }
                    InputAction::FocusDown => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            let rect = state.content_rect(&self.config);
                            let rects = state.pane_tree.layout.compute_rects(rect);
                            state.pane_tree.focus_direction(&rects, Direction::Down);
                        }
                    }
                    InputAction::OpenConfig => {
                        if let Some(state) = self.windows.get_mut(&window_id) {
                            state.open_config_in_pane();
                        }
                    }
                    InputAction::NewTab => {
                        eprintln!("[debug] NewTab triggered");
                        self.open_new_tab(event_loop, window_id);
                        eprintln!("[debug] NewTab done, windows={}", self.windows.len());
                    }
                    InputAction::NewWindow => {
                        eprintln!("[debug] NewWindow triggered");
                        self.open_new_window(event_loop);
                        eprintln!("[debug] NewWindow done, windows={}", self.windows.len());
                    }
                    InputAction::None => {}
                    InputAction::Scroll(_) => {}
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                if let Some(state) = self.windows.get_mut(&window_id) {
                    state.cursor_pos = (position.x as f32, position.y as f32);
                }
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(state) = self.windows.get_mut(&window_id) {
                    let rect = state.content_rect(&self.config);
                    let layout_rects = state.pane_tree.layout.compute_rects(rect);
                    let (cx, cy) = state.cursor_pos;
                    for (pane_id, pane_rect) in &layout_rects {
                        if cx >= pane_rect.x
                            && cx < pane_rect.x + pane_rect.width
                            && cy >= pane_rect.y
                            && cy < pane_rect.y + pane_rect.height
                        {
                            state.pane_tree.focused_id = *pane_id;
                            break;
                        }
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(state) = self.windows.get_mut(&window_id) {
                    let scale = state.window.scale_factor();
                    let dy = handle_scroll(delta, scale);
                    let focused = state.pane_tree.focused_id;
                    state.renderer.ensure_pane_state(focused);
                    if let Some(spring) = state.renderer.scroll_springs.get_mut(&focused) {
                        spring.scroll_by(-dy);
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();

                // Open config in pane if requested via menu item (only for first window)
                let open_config = OPEN_CONFIG_REQUESTED.swap(false, Ordering::Relaxed);

                if let Some(state) = self.windows.get_mut(&window_id) {
                    let dt = now.duration_since(state.last_frame).as_secs_f32().min(0.05);
                    state.last_frame = now;

                    // Hot-reload config if file changed
                    if state
                        .config_rx
                        .as_ref()
                        .map_or(false, |rx| rx.try_recv().is_ok())
                    {
                        let new_config = Config::load_or_default();
                        self.config = new_config.clone();
                        let rect = state.content_rect(&self.config);
                        let scale = state.window.scale_factor() as f32;
                        let metrics_changed = state.renderer.apply_config(new_config, scale);
                        if metrics_changed {
                            let layout_rects = state.pane_tree.layout.compute_rects(rect);
                            state
                                .pane_tree
                                .resize_panes(&layout_rects, state.renderer.cell_w, state.renderer.cell_h);
                        }
                    }

                    if open_config {
                        state.open_config_in_pane();
                    }

                    // Auto-close panes whose shell has exited
                    let dead = state.pane_tree.dead_pane_ids();
                    for id in dead {
                        state.pane_tree.close_pane(id);
                    }
                    if state.pane_tree.panes.is_empty() {
                        self.windows.remove(&window_id);
                        if self.windows.is_empty() {
                            event_loop.exit();
                        }
                        return;
                    }

                    // Drain PTY output
                    state.pane_tree.drain_all_pty_output();

                    // Update cursor spring targets
                    let rect = state.content_rect(&self.config);
                    let layout_rects = state.pane_tree.layout.compute_rects(rect);
                    for (pane_id, pane_rect) in &layout_rects {
                        if let Some(pane) = state.pane_tree.panes.iter().find(|p| p.id == *pane_id) {
                            let grid = pane.terminal.grid.lock();
                            let col = grid.cursor_col;
                            let row = grid.cursor_row;
                            drop(grid);
                            state.renderer.update_cursor_for_pane(*pane_id, col, row, *pane_rect);
                        }
                    }

                    // Tick animations
                    state.renderer.tick_animations(dt);

                    // Render
                    match state.renderer.render(&state.pane_tree, rect) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            let s = state.window.inner_size();
                            state.renderer.resize(s.width, s.height);
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
