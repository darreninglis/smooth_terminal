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

// ---------------------------------------------------------------------------
// macOS geometry types used for window tiling and tab-bar hit-testing.
// These mirror the C layout of CGPoint / CGSize / CGRect so they can be
// passed directly through objc2 msg_send! calls.
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
mod mac_geom {
    use objc2::encode::{Encode, Encoding};

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }

    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding =
            Encoding::Struct("CGPoint", &[Encoding::Double, Encoding::Double]);
    }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding =
            Encoding::Struct("CGSize", &[Encoding::Double, Encoding::Double]);
    }
    unsafe impl Encode for CGRect {
        const ENCODING: Encoding =
            Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }
}

#[cfg(target_os = "macos")]
use mac_geom::{CGPoint, CGRect, CGSize};

/// Window-tiling target positions (used by macOS tile helpers).
#[cfg(target_os = "macos")]
enum MacTilePos {
    Left,
    Right,
    Maximize,
    Restore,
}

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
    // Retained NSEvent monitor for double-click tab renaming (macOS only).
    #[cfg(target_os = "macos")]
    _event_monitor: Option<objc2::rc::Retained<objc2::runtime::AnyObject>>,
}

impl App {
    pub fn new(config: Config) -> Self {
        Self {
            windows: HashMap::new(),
            config,
            first_window_id: None,
            #[cfg(target_os = "macos")]
            _event_monitor: None,
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

    // -----------------------------------------------------------------------
    // macOS helpers
    // -----------------------------------------------------------------------

    /// Switch to the tab at 1-based index `n` using the native macOS
    /// NSWindowTabGroup API.
    #[cfg(target_os = "macos")]
    fn macos_switch_tab(window: &Arc<Window>, n: usize) {
        use objc2::{msg_send, msg_send_id};
        use objc2::rc::Retained;
        use objc2::runtime::AnyObject;
        use objc2_app_kit::NSWindow;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let Ok(handle) = window.window_handle() else { return };
        let RawWindowHandle::AppKit(h) = handle.as_raw() else { return };

        unsafe {
            let view = h.ns_view.as_ptr() as *const AnyObject;
            let ns_window: Option<Retained<NSWindow>> = msg_send_id![&*view, window];
            let Some(ns_window) = ns_window else { return };

            let tab_group: Option<Retained<AnyObject>> =
                msg_send_id![&*ns_window, tabGroup];
            let Some(tab_group) = tab_group else { return };

            let tabs: Retained<AnyObject> = msg_send_id![&*tab_group, windows];
            let count: usize = msg_send![&*tabs, count];
            let idx = n.saturating_sub(1);
            if idx < count {
                let target: Retained<AnyObject> =
                    msg_send_id![&*tabs, objectAtIndex: idx];
                let _: () = msg_send![&*target, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
            }
        }
    }

    /// Tile the current window to a screen position using NSWindow
    /// `setFrame:display:animate:`.
    #[cfg(target_os = "macos")]
    fn macos_tile_window(window: &Arc<Window>, pos: MacTilePos) {
        use objc2::{class, msg_send, msg_send_id};
        use objc2::rc::Retained;
        use objc2::runtime::AnyObject;
        use objc2_app_kit::NSWindow;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let Ok(handle) = window.window_handle() else { return };
        let RawWindowHandle::AppKit(h) = handle.as_raw() else { return };

        unsafe {
            let view = h.ns_view.as_ptr() as *const AnyObject;
            let ns_window: Option<Retained<NSWindow>> = msg_send_id![&*view, window];
            let Some(ns_window) = ns_window else { return };

            // Prefer the window's own screen; fall back to the first screen.
            let screen: Option<Retained<AnyObject>> = msg_send_id![&*ns_window, screen];
            let screen = screen.or_else(|| {
                let screens: Retained<AnyObject> =
                    msg_send_id![class!(NSScreen), screens];
                let count: usize = msg_send![&*screens, count];
                if count > 0 {
                    Some(msg_send_id![&*screens, objectAtIndex: 0_usize])
                } else {
                    None
                }
            });
            let Some(screen) = screen else { return };

            let visible: CGRect = msg_send![&*screen, visibleFrame];
            let new_frame: CGRect = match pos {
                MacTilePos::Left => CGRect {
                    origin: visible.origin,
                    size: CGSize {
                        width: visible.size.width / 2.0,
                        height: visible.size.height,
                    },
                },
                MacTilePos::Right => CGRect {
                    origin: CGPoint {
                        x: visible.origin.x + visible.size.width / 2.0,
                        y: visible.origin.y,
                    },
                    size: CGSize {
                        width: visible.size.width / 2.0,
                        height: visible.size.height,
                    },
                },
                MacTilePos::Maximize => visible,
                MacTilePos::Restore => {
                    let w = 1200.0_f64;
                    let h = 800.0_f64;
                    CGRect {
                        origin: CGPoint {
                            x: visible.origin.x + (visible.size.width - w) / 2.0,
                            y: visible.origin.y + (visible.size.height - h) / 2.0,
                        },
                        size: CGSize { width: w, height: h },
                    }
                }
            };

            let _: () = msg_send![
                &*ns_window,
                setFrame: new_frame
                display: true
                animate: true
            ];
        }
    }

    /// Install a local NSEvent monitor that fires for every left-mouse-down
    /// event and shows a rename dialog when the user double-clicks inside the
    /// window's title-bar / tab-bar area (above the content layout rect).
    #[cfg(target_os = "macos")]
    fn install_tab_rename_monitor()
        -> Option<objc2::rc::Retained<objc2::runtime::AnyObject>>
    {
        use block2::StackBlock;
        use objc2::{class, msg_send, msg_send_id};
        use objc2::rc::Retained;
        use objc2::runtime::AnyObject;

        // NSEventMaskLeftMouseDown = 1 << 1
        let mask: u64 = 1 << 1;

        let block = StackBlock::new(|event: *mut AnyObject| -> *mut AnyObject {
            unsafe {
                if event.is_null() {
                    return event;
                }
                let click_count: isize = msg_send![&*event, clickCount];
                if click_count == 2 {
                    let win_ptr: *mut AnyObject = msg_send![&*event, window];
                    if !win_ptr.is_null() {
                        let content_rect: CGRect =
                            msg_send![&*win_ptr, contentLayoutRect];
                        let loc: CGPoint = msg_send![&*event, locationInWindow];
                        let content_top =
                            content_rect.origin.y + content_rect.size.height;
                        // Click is above the content area → title bar / tab bar.
                        if loc.y > content_top {
                            // Retain the window so it stays alive during the
                            // synchronous modal dialog.
                            let retained: Option<Retained<AnyObject>> =
                                Retained::retain(win_ptr);
                            if let Some(win) = retained {
                                macos_show_rename_dialog(&win);
                            }
                        }
                    }
                }
                event
            }
        });

        unsafe {
            msg_send_id![
                class!(NSEvent),
                addLocalMonitorForEventsMatchingMask: mask
                handler: &*block
            ]
        }
    }
}

/// Show an NSAlert with an NSTextField accessory that lets the user rename
/// the tab associated with `ns_window`.  Called on the main thread.
#[cfg(target_os = "macos")]
unsafe fn macos_show_rename_dialog(ns_window: &objc2::runtime::AnyObject) {
    use objc2::{class, msg_send, msg_send_id};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;

    // Get the current window title as a Rust String via UTF8String.
    let title_obj: Retained<AnyObject> = msg_send_id![ns_window, title];
    let cstr: *const std::ffi::c_char = msg_send![&*title_obj, UTF8String];
    let current_title = if cstr.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(cstr).to_string_lossy().into_owned()
    };

    // Build a 300×24 NSTextField pre-filled with the current title.
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: 300.0, height: 24.0 },
    };
    // Use `+new` (alloc+init) then set the frame explicitly; this avoids
    // having to work with objc2's typed `Allocated<T>` return from `+alloc`.
    let text_field: Retained<AnyObject> = msg_send_id![class!(NSTextField), new];
    let _: () = msg_send![&*text_field, setFrame: frame];
    let title_ns = NSString::from_str(&current_title);
    let _: () = msg_send![&*text_field, setStringValue: &*title_ns];
    // Pre-select the existing text so the user can type to replace it.
    let _: () = msg_send![&*text_field, selectText: std::ptr::null::<AnyObject>()];

    // Build the NSAlert.
    let alert: Retained<AnyObject> = msg_send_id![class!(NSAlert), new];
    let msg_text = NSString::from_str("Rename Tab");
    let _: () = msg_send![&*alert, setMessageText: &*msg_text];
    let ok_str = NSString::from_str("OK");
    let _: () = msg_send![&*alert, addButtonWithTitle: &*ok_str];
    let cancel_str = NSString::from_str("Cancel");
    let _: () = msg_send![&*alert, addButtonWithTitle: &*cancel_str];
    let _: () = msg_send![&*alert, setAccessoryView: &*text_field];

    // Run modally on the main thread.
    let response: isize = msg_send![&*alert, runModal];
    // NSAlertFirstButtonReturn == 1000
    if response == 1000 {
        let new_title_obj: Retained<AnyObject> =
            msg_send_id![&*text_field, stringValue];
        let cstr2: *const std::ffi::c_char =
            msg_send![&*new_title_obj, UTF8String];
        if !cstr2.is_null() {
            let new_str = std::ffi::CStr::from_ptr(cstr2).to_string_lossy();
            if !new_str.is_empty() {
                let new_ns = NSString::from_str(&*new_str);
                let _: () = msg_send![ns_window, setTitle: &*new_ns];
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let (window_id, state) = Self::create_window_state(event_loop, &self.config);

        #[cfg(target_os = "macos")]
        {
            crate::menubar::setup_menubar();
            self._event_monitor = Self::install_tab_rename_monitor();
        }

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
                    InputAction::SwitchTab(n) => {
                        #[cfg(target_os = "macos")]
                        if let Some(state) = self.windows.get(&window_id) {
                            Self::macos_switch_tab(&state.window, n);
                        }
                    }
                    InputAction::TileLeft => {
                        #[cfg(target_os = "macos")]
                        if let Some(state) = self.windows.get(&window_id) {
                            Self::macos_tile_window(&state.window, MacTilePos::Left);
                        }
                    }
                    InputAction::TileRight => {
                        #[cfg(target_os = "macos")]
                        if let Some(state) = self.windows.get(&window_id) {
                            Self::macos_tile_window(&state.window, MacTilePos::Right);
                        }
                    }
                    InputAction::Maximize => {
                        #[cfg(target_os = "macos")]
                        if let Some(state) = self.windows.get(&window_id) {
                            Self::macos_tile_window(&state.window, MacTilePos::Maximize);
                        }
                    }
                    InputAction::RestoreWindow => {
                        #[cfg(target_os = "macos")]
                        if let Some(state) = self.windows.get(&window_id) {
                            Self::macos_tile_window(&state.window, MacTilePos::Restore);
                        }
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
