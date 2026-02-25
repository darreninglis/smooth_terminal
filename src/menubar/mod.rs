#[cfg(target_os = "macos")]
use objc2::{declare_class, msg_send, msg_send_id, mutability, ClassType, DeclaredClass};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2_foundation::NSObject;

// A tiny NSObject subclass whose sole purpose is to respond to `openConfig:`
// from any menu item that targets it.
#[cfg(target_os = "macos")]
declare_class!(
    struct ConfigOpener;

    unsafe impl ClassType for ConfigOpener {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "STConfigOpener";
    }

    impl DeclaredClass for ConfigOpener {
        type Ivars = ();
    }

    unsafe impl ConfigOpener {
        #[method(openConfig:)]
        fn open_config(&self, _sender: *mut AnyObject) {
            let _ = crate::config::Config::open_in_editor();
        }
    }
);

#[cfg(target_os = "macos")]
pub fn setup_menubar() {
    use objc2::sel;
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
    use objc2_foundation::{MainThreadMarker, NSString};

    // BUILD_NUMBER is injected at compile time by build.rs.
    const BUILD: &str = env!("BUILD_NUMBER");

    unsafe {
        let mtm = MainThreadMarker::new().expect("must be on main thread");

        // Allocate the config opener; leaked so it stays alive for the app lifetime.
        let opener: Retained<ConfigOpener> = msg_send_id![ConfigOpener::class(), new];

        // ── Left-side menu bar: rename app-menu title + add Preferences ────────
        let ns_app = NSApplication::sharedApplication(mtm);
        if let Some(main_menu) = ns_app.mainMenu() {
            if main_menu.numberOfItems() > 0 {
                if let Some(app_menu_item) = main_menu.itemAtIndex(0) {
                    let new_title =
                        NSString::from_str(&format!("smooth terminal {}", BUILD));
                    app_menu_item.setTitle(&new_title);

                    // Get the app submenu (winit creates it automatically).
                    let app_submenu: Option<Retained<NSMenu>> =
                        msg_send_id![&*app_menu_item, submenu];
                    if let Some(submenu) = app_submenu {
                        // Insert "Preferences…" at index 1 (after "About …"),
                        // which is the standard macOS position.
                        let prefs_title = NSString::from_str("Preferences\u{2026}");
                        let prefs_key = NSString::from_str(",");
                        let prefs_item = NSMenuItem::initWithTitle_action_keyEquivalent(
                            mtm.alloc(),
                            &prefs_title,
                            Some(sel!(openConfig:)),
                            &prefs_key,
                        );
                        let _: () = msg_send![&*prefs_item, setTarget: &*opener];
                        let _: () =
                            msg_send![&*submenu, insertItem: &*prefs_item atIndex: 1_isize];
                    }
                }
            }
        }

        // Keep opener alive — it is the target for all config menu items.
        std::mem::forget(opener);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn setup_menubar() {}
