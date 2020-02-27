//! Keyboard events
use arrayvec::ArrayVec;

use super::{HView, HViewRef, HWndRef, ViewFlags, Wnd};
use crate::pal::Wm;

impl HWndRef<'_> {
    /// Focus the specified view.
    pub fn set_focused_view(self, new_focused_view: Option<HView>) {
        let mut focused_view_cell = self.wnd.focused_view.borrow_mut();

        if new_focused_view == *focused_view_cell {
            return;
        }

        if let Some(view) = &new_focused_view {
            debug_assert_eq!(
                view.containing_wnd().as_ref().map(|hw| hw.as_ref()),
                Some(self),
                "the window does not contain `new_focused_view`"
            );
        }

        if !self.is_focused() {
            *focused_view_cell = new_focused_view;
            return;
        }

        let mut path1 = ArrayVec::new();
        let mut path2 = ArrayVec::new();

        HViewRef::get_path_if_some(focused_view_cell.as_ref().map(|hw| hw.as_ref()), &mut path1);
        HViewRef::get_path_if_some(new_focused_view.as_ref().map(|hw| hw.as_ref()), &mut path2);

        // Find the lowest common ancestor
        use itertools::izip;
        let lca_depth = izip!(path1.iter().rev(), path2.iter().rev())
            .take_while(|(v1, v2)| v1 == v2)
            .count();

        debug_assert!(if lca_depth == 0 {
            true
        } else {
            path1[path1.len() - lca_depth] == path2[path2.len() - lca_depth]
        });

        // Call the handlers
        if let Some(hview) = &*focused_view_cell {
            hview
                .view
                .listener
                .borrow()
                .focus_lost(self.wnd.wm, hview.as_ref());
        }
        for hview in path1[..path1.len() - lca_depth].iter() {
            hview
                .view
                .listener
                .borrow()
                .focus_leave(self.wnd.wm, hview.as_ref());
        }
        for hview in path2[..path2.len() - lca_depth].iter().rev() {
            hview
                .view
                .listener
                .borrow()
                .focus_enter(self.wnd.wm, hview.as_ref());
        }
        if let Some(hview) = &new_focused_view {
            hview
                .view
                .listener
                .borrow()
                .focus_got(self.wnd.wm, hview.as_ref());
        }

        *focused_view_cell = new_focused_view;
    }

    /// Get the currently focused view in the window.
    ///
    /// If the window is currently not focused, this method will return the view
    /// to be focused when the window receives a focus again.
    pub fn focused_view(self) -> Option<HView> {
        self.wnd.focused_view.borrow().clone()
    }

    /// Raise `focus_(lost|leave|enter|got)` events as response to a change in
    /// the window's focus state.
    pub(super) fn raise_view_focus_events_for_wnd_focus_state_change(self) {
        let focused_view_cell = self.wnd.focused_view.borrow();

        if let Some(hview) = &*focused_view_cell {
            let hview = hview.as_ref();

            if self.is_focused() {
                hview.invoke_focus_got_enter_for_ancestors(self.wnd.wm);
            } else {
                hview.invoke_focus_lost_leave_for_ancestors(self.wnd.wm);
            }
        }
    }
}

impl HViewRef<'_> {
    /// Focus the view.
    pub fn focus(self) {
        if !self.view.flags.get().contains(ViewFlags::TAB_STOP) {
            return;
        }

        if let Some(wnd) = self.containing_wnd() {
            wnd.as_ref().set_focused_view(Some(self.cloned()));
        }
    }

    /// Get a flag indicating whether the view is currently focused or not.
    ///
    /// If the containing window is not focused, this method returns `false`.
    pub fn is_focused(self) -> bool {
        if let Some(hwnd) = self.containing_wnd() {
            if !hwnd.is_focused() {
                false
            } else if let Some(view) = &*hwnd.wnd.focused_view.borrow() {
                view.as_ref() == self
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get a flag indicating whether the view or a subview of the view is
    /// currently focused or not.
    ///
    /// If the containing window is not focused, this method returns `false`.
    pub fn improper_subview_is_focused(self) -> bool {
        if let Some(hwnd) = self.containing_wnd() {
            if !hwnd.is_focused() {
                false
            } else if let Some(view) = &*hwnd.wnd.focused_view.borrow() {
                view.as_ref().is_improper_subview_of(self)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Steal a keyboard focus from the specified view.
    ///
    /// If `subview` is `true`, the subviews of `view` are also affected.
    pub(super) fn defocus_subviews(self, wnd: &Wnd, subview: bool, raise_events: bool) {
        let mut focused_view_cell = wnd.focused_view.borrow_mut();

        if let Some(view) = &*focused_view_cell {
            let cancel_drag = if subview {
                view.as_ref().is_improper_subview_of(self)
            } else {
                view.as_ref() == self
            };

            if cancel_drag {
                let view = std::mem::replace(&mut *focused_view_cell, None).unwrap();

                // Unborrow `focused_view`
                drop(focused_view_cell);

                if raise_events {
                    view.as_ref().invoke_focus_lost_leave_for_ancestors(wnd.wm);
                }
            }
        }
    }

    fn invoke_focus_lost_leave_for_ancestors(self, wm: Wm) {
        let mut path = ArrayVec::new();
        self.get_path(&mut path);

        self.view.listener.borrow().focus_lost(wm, self);

        for hview in path.iter() {
            hview.view.listener.borrow().focus_leave(wm, hview.as_ref());
        }
    }

    fn invoke_focus_got_enter_for_ancestors(self, wm: Wm) {
        let mut path = ArrayVec::new();
        self.get_path(&mut path);

        for hview in path.iter().rev() {
            hview.view.listener.borrow().focus_enter(wm, hview.as_ref());
        }

        self.view.listener.borrow().focus_got(wm, self);
    }
}
