use cocoa::{
    appkit,
    appkit::{NSWindow, NSWindowStyleMask},
    base::nil,
    foundation::{NSPoint, NSRect, NSSize, NSString},
};
use objc::{msg_send, runtime::NO, sel, sel_impl};

use super::super::types;
use super::{IdRef, WM};

#[derive(Clone)]
pub struct HWnd {
    window: IdRef,
}

impl HWnd {
    pub(super) fn new(attrs: &types::WndAttrs<WM, &str>) -> Self {
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

            let this = Self { window };
            this.set_attrs(attrs);

            this
        }
    }

    pub(super) fn set_attrs(&self, attrs: &types::WndAttrs<WM, &str>) {
        unsafe {
            if let Some(value) = attrs.size {
                self.window
                    .setContentSize_(NSSize::new(value[0] as _, value[1] as _));
            }

            if let Some(value) = attrs.caption {
                let title = IdRef::new(NSString::alloc(nil).init_str(value));
                self.window.setTitle_(*title);
            }

            match attrs.visible {
                Some(true) => {
                    self.window.makeKeyAndOrderFront_(nil);
                }
                Some(false) => {
                    self.window.orderOut_(nil);
                }
                None => {}
            }

            // TODO: window listener
        }
    }

    pub(super) fn remove(&self) {
        unsafe {
            let () = msg_send![*self.window, close];
        }
    }
}
