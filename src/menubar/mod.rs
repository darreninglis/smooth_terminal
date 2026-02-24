#[cfg(target_os = "macos")]
pub fn setup_menubar() {
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem, NSStatusBar};
    use objc2_foundation::{MainThreadMarker, NSString};

    // BUILD_NUMBER is injected at compile time by build.rs.
    const BUILD: &str = env!("BUILD_NUMBER");

    unsafe {
        let mtm = MainThreadMarker::new().expect("must be on main thread");

        // ── Left-side menu bar: rename the app-menu title ──────────────────
        // winit creates a default NSApplication main menu whose first item
        // carries the process name.  Overwrite it so the user sees
        // "smooth terminal <N>" in the menu bar when the app is frontmost.
        let ns_app = NSApplication::sharedApplication(mtm);
        if let Some(main_menu) = ns_app.mainMenu() {
            if main_menu.numberOfItems() > 0 {
                if let Some(app_menu_item) = main_menu.itemAtIndex(0) {
                    let new_title =
                        NSString::from_str(&format!("smooth terminal {}", BUILD));
                    app_menu_item.setTitle(&new_title);
                }
            }
        }

        // ── Right-side menu bar: status-bar item ────────────────────────────
        // Short label ("st <N>") reduces the chance of being clipped by the
        // notch or crowded by other status-bar icons.
        let status_bar = NSStatusBar::systemStatusBar();
        // NSVariableStatusItemLength = -1.0
        let status_item = status_bar.statusItemWithLength(-1.0_f64);

        if let Some(button) = status_item.button(mtm) {
            let label = format!("st {}", BUILD);
            button.setTitle(&NSString::from_str(&label));
        }

        // Build the drop-down menu for the status-bar item.
        let menu = NSMenu::new(mtm);

        {
            let title = NSString::from_str("Open Config");
            let action = objc2::sel!(openConfig:);
            let key = NSString::from_str("");
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &title,
                Some(action),
                &key,
            );
            menu.addItem(&item);
        }

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        {
            let title = NSString::from_str("Quit");
            let action = objc2::sel!(terminate:);
            let key = NSString::from_str("q");
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &title,
                Some(action),
                &key,
            );
            menu.addItem(&item);
        }

        status_item.setMenu(Some(&menu));

        // NSStatusBar retains the item, but we also forget our Retained<T>
        // wrapper so the Rust drop glue can never send an extra `release`.
        std::mem::forget(status_item);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn setup_menubar() {}
