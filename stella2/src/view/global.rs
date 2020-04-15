//! Application-global commands
use tcw3::{
    pal,
    uicore::{ActionId, InterpretEventCtx},
};

/// Re-export system actions
pub use tcw3::uicore::actions as sys;

iota::iota! {
    pub const QUIT: ActionId = iota + 1;
            , TOGGLE_SIDEBAR
}

pub fn interpret_event(ctx: &mut InterpretEventCtx<'_>) {
    ctx.use_accel(&tcw3::pal::accel_table![
        (
            sys::SELECT_ALL,
            windows("Ctrl+A"),
            gtk("Ctrl+A"),
            macos_sel("selectAll:")
        ),
        (
            sys::COPY,
            windows("Ctrl+C"),
            gtk("Ctrl+C"),
            macos_sel("copy:")
        ),
        (
            sys::CUT,
            windows("Ctrl+X"),
            gtk("Ctrl+X"),
            macos_sel("cut:")
        ),
        (
            sys::PASTE,
            windows("Ctrl+V"),
            gtk("Ctrl+V"),
            macos_sel("paste:")
        ),
        (sys::PASTE_AS_PLAIN_TEXT, macos_sel("pasteAsPlainText:")),
        (
            QUIT,
            windows("Ctrl+Q"),
            gtk("Ctrl+Q"),
            macos_sel("terminate:")
        ),
        (TOGGLE_SIDEBAR, macos_sel("toggleSidebar:")),
    ]);
}

/// Create a main menu on macOS.
#[cfg(target_os = "macos")]
pub fn set_main_menu(_: pal::Wm) {
    // Most of these are predefined by the Interface Builder template that
    // comes with Xcode.
    static MENU: &[Item] = &[
        Item::Submenu(
            "",
            &[
                Item::leaf("About Stella 2", "orderFrontStandardAboutPanel:"),
                Item::Sep,
                Item::leaf("Preferences…", "orderFrontPreferencesPanel:").with_cmd(","),
                Item::Sep,
                Item::Submenu("Services", &[]),
                Item::Sep,
                Item::leaf("Hide Stella 2", "hide:").with_cmd("h"),
                Item::leaf("Hide Others", "hideOtherApplications:").with_cmd_opt("h"),
                Item::leaf("Show All", "unhideAllApplications:"),
                Item::Sep,
                Item::leaf("Quit Stella 2", "terminate:").with_cmd("q"),
            ],
        ),
        Item::Submenu(
            "Edit",
            &[
                Item::leaf("Undo", "undo:").with_cmd("z"),
                Item::leaf("Redo", "redo:").with_cmd("Z"),
                Item::Sep,
                Item::leaf("Cut", "cut:").with_cmd("x"),
                Item::leaf("Copy", "copy:").with_cmd("c"),
                Item::leaf("Paste", "paste:").with_cmd("v"),
                Item::leaf("Paste and Match Style", "pasteAsPlainText:").with_cmd_opt("V"),
                Item::leaf("Delete", "delete:"),
                Item::leaf("Select All", "selectAll:").with_cmd("a"),
                Item::Sep,
                Item::Submenu(
                    "Transformations",
                    &[
                        Item::leaf("Make Upper Case", "uppercaseWord:"),
                        Item::leaf("Make Lower Case", "lowercaseWord:"),
                        Item::leaf("Capitalize", "capitalizeWord:"),
                    ],
                ),
                Item::Submenu(
                    "Speech",
                    &[
                        Item::leaf("Start Speaking", "startSpeaking:"),
                        Item::leaf("Stop Speaking", "stopSpeaking:"),
                    ],
                ),
            ],
        ),
        Item::Submenu(
            "View",
            &[
                Item::leaf("Show Sidebar", "toggleSidebar:").with_cmd_ctrl("s"),
                Item::leaf("Enter Full Screen", "toggleFullScreen:").with_cmd_ctrl("f"),
            ],
        ),
        Item::Submenu(
            "Window",
            &[
                Item::leaf("Minimize", "performMiniaturize:").with_cmd("m"),
                Item::leaf("Zoom", "performZoom:"),
                Item::Sep,
                Item::leaf("Bring All to Front", "arrangeInFront:"),
            ],
        ),
        Item::Submenu(
            "Help",
            &[Item::leaf(
                "Commence Spline Reticulation",
                "reticulateSpline̦:",
            )],
        ),
    ];

    use cocoa::{
        appkit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem},
        base::{id, nil},
        foundation::{NSAutoreleasePool, NSString},
    };
    use objc::{msg_send, runtime::Sel, sel, sel_impl};

    enum Item {
        Leaf(Leaf),
        Submenu(&'static str, &'static [Item]),
        Sep,
    }

    struct Leaf {
        title: &'static str,
        action: &'static str,
        key_eq: &'static str,
        mod_flags: NSEventModifierFlags,
    }

    impl Item {
        const fn leaf(title: &'static str, action: &'static str) -> Self {
            Self::Leaf(Leaf {
                title,
                action,
                key_eq: "",
                mod_flags: NSEventModifierFlags::empty(),
            })
        }

        const fn get_leaf(self) -> Leaf {
            if let Self::Leaf(leaf) = self {
                leaf
            } else {
                // Panicking in `const fn` isn't supported yet
                Leaf {
                    title: "",
                    action: "",
                    key_eq: "",
                    mod_flags: NSEventModifierFlags::empty(),
                }
            }
        }

        const fn with_key_eq(self, key_eq: &'static str, mod_flags: NSEventModifierFlags) -> Self {
            Self::Leaf(Leaf {
                key_eq,
                mod_flags,
                ..self.get_leaf()
            })
        }

        const fn with_cmd(self, key_eq: &'static str) -> Self {
            self.with_key_eq(key_eq, NSEventModifierFlags::NSCommandKeyMask)
        }

        const fn with_cmd_opt(self, key_eq: &'static str) -> Self {
            self.with_key_eq(
                key_eq,
                NSEventModifierFlags::from_bits_truncate(
                    NSEventModifierFlags::NSCommandKeyMask.bits()
                        | NSEventModifierFlags::NSAlternateKeyMask.bits(),
                ),
            )
        }

        const fn with_cmd_ctrl(self, key_eq: &'static str) -> Self {
            self.with_key_eq(
                key_eq,
                NSEventModifierFlags::from_bits_truncate(
                    NSEventModifierFlags::NSCommandKeyMask.bits()
                        | NSEventModifierFlags::NSControlKeyMask.bits(),
                ),
            )
        }
    }

    pub struct AutoreleasePool(id);

    impl AutoreleasePool {
        pub fn new() -> Self {
            Self(unsafe { NSAutoreleasePool::new(nil) })
        }
    }

    impl Drop for AutoreleasePool {
        fn drop(&mut self) {
            let () = unsafe { msg_send![self.0, release] };
        }
    }

    let _arp = AutoreleasePool::new();

    unsafe fn new_menu(title: &str, items: &[Item], app: id) -> id {
        let menu = NSMenu::alloc(nil)
            .autorelease()
            .initWithTitle_(NSString::alloc(nil).autorelease().init_str(title));

        // Special items
        if title == "Services" {
            app.setServicesMenu_(menu);
        } else if title == "Window" {
            app.setWindowsMenu_(menu);
        }

        for item in items.iter() {
            match item {
                Item::Leaf(Leaf {
                    title,
                    action,
                    key_eq,
                    mod_flags,
                }) => {
                    let cocoa_item = menu.addItemWithTitle_action_keyEquivalent(
                        NSString::alloc(nil).autorelease().init_str(title),
                        Sel::register(action),
                        NSString::alloc(nil).autorelease().init_str(key_eq),
                    );
                    cocoa_item.setKeyEquivalentModifierMask_(*mod_flags);
                }
                Item::Submenu(title, children) => {
                    let submenu = NSMenuItem::alloc(nil).autorelease();
                    let () = msg_send![submenu, setTitle:
                        NSString::alloc(nil).autorelease().init_str(title)];
                    submenu.setSubmenu_(new_menu(title, children, app));
                    menu.addItem_(submenu);
                }
                Item::Sep => {
                    menu.addItem_(NSMenuItem::separatorItem(nil));
                }
            }
        }
        menu
    }

    unsafe {
        let app = cocoa::appkit::NSApp();
        app.setMainMenu_(new_menu("", MENU, app));
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_main_menu(_: pal::Wm) {}
