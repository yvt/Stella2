use tcw3_images::{dpi_scale_add_ref, dpi_scale_release};
use tcw3_pal::{self as pal, prelude::*};

use super::HWnd;

/// Register a hook (`subscribe_dpi_scale_changed`) on `HWnd` to keep the list
/// of known DPI scale values up-to-date based on currently open windows.
pub(crate) fn handle_new_wnd(hwnd: &HWnd) {
    use std::cell::Cell;

    struct ListenerState {
        wm: pal::Wm,
        dpi_scale: Cell<f32>,
    }

    impl Drop for ListenerState {
        fn drop(&mut self) {
            // This method is called when the window is destroyed.
            // Use `invoke` because we don't know the state of the call stack
            // when `drop` is called.
            let dpi_scale = self.dpi_scale.get();
            self.wm.invoke(move |wm| {
                dpi_scale_release(wm, dpi_scale);
            });
        }
    }

    let state = ListenerState {
        wm: hwnd.wm(),
        dpi_scale: Cell::new(hwnd.dpi_scale()),
    };
    dpi_scale_add_ref(hwnd.wm(), state.dpi_scale.get());

    hwnd.subscribe_dpi_scale_changed(Box::new(move |wm, hwnd| {
        let state = &state;
        let new_dpi_scale = hwnd.dpi_scale();
        if new_dpi_scale != state.dpi_scale.get() {
            dpi_scale_add_ref(wm, new_dpi_scale);
            dpi_scale_release(wm, state.dpi_scale.get());
            state.dpi_scale.set(new_dpi_scale);
        }
    }));
}
