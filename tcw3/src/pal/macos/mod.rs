use cocoa::{
    appkit,
    appkit::{NSApplication, NSApplicationActivationPolicy, NSWindow, NSWindowStyleMask},
    base::nil,
    foundation::{NSPoint, NSRect, NSSize, NSString},
};
use fragile::Fragile;
use lazy_static::lazy_static;
use objc::{runtime::NO, msg_send, sel, sel_impl};

use super::{traits, types};

mod utils;
use self::utils::{with_autorelease_pool, IdRef};

pub struct WM {}

impl WM {
    pub fn global() -> &'static WM {
        lazy_static! {
            static ref GLOBAL_WM: Fragile<WM> = {
                // Mark the current thread as the main thread
                unsafe {
                    appkit::NSApp();
                }

                // `Fragile` wraps `!Send` types and performs run-time
                // main thread checking
                Fragile::new(WM::new())
            };
        }

        GLOBAL_WM.get()
    }

    fn new() -> Self {
        Self {}
    }
}

#[derive(Clone)]
pub struct HWnd {
    window: IdRef,
}

impl traits::WM for WM {
    type HWnd = HWnd;

    fn enter_main_loop(&self) {
        unsafe {
            let app = appkit::NSApp();
            app.setActivationPolicy_(
                NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular,
            );
            app.finishLaunching();
            app.run();
        }
    }

    fn terminate(&self) {
        unsafe {
            let app = appkit::NSApp();
            let () = msg_send![app, terminate];
        }
    }

    fn new_wnd(&self, attrs: &types::WndAttrs<&str>) -> Self::HWnd {
        unsafe {
            let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0));
            let masks = NSWindowStyleMask::NSClosableWindowMask
                | NSWindowStyleMask::NSMiniaturizableWindowMask
                | NSWindowStyleMask::NSResizableWindowMask
                | NSWindowStyleMask::NSTitledWindowMask;

            let window_id = NSWindow::alloc(nil);
            let window = IdRef::new(window_id.initWithContentRect_styleMask_backing_defer_(
                frame,
                masks,
                appkit::NSBackingStoreBuffered,
                NO,
            ))
            .non_nil()
            .unwrap();

            window.center();
            window.setReleasedWhenClosed_(NO);

            let hwnd = HWnd { window };
            self.set_wnd_attr(&hwnd, attrs);

            hwnd
        }
    }

    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &types::WndAttrs<&str>) {
        unsafe {
            if let Some(value) = attrs.size {
                window
                    .window
                    .setContentSize_(NSSize::new(value[0] as _, value[1] as _));
            }

            if let Some(value) = attrs.caption {
                let title = IdRef::new(NSString::alloc(nil).init_str(value));
                window.window.setTitle_(*title);
            }

            match attrs.visible {
                Some(true) => {
                    window.window.makeKeyAndOrderFront_(nil);
                }
                Some(false) => {
                    window.window.orderOut_(nil);
                }
                None => {}
            }
        }
    }

    fn remove_wnd(&self, window: &Self::HWnd) {
        unsafe {
            let () = msg_send![*window.window, close];
        }
    }
}
