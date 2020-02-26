//! Keyboard events
use arrayvec::ArrayVec;

use super::{HView, HViewRef, HWndRef, ViewFlags, Wnd};

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

    pub fn focused_view(self) -> Option<HView> {
        self.wnd.focused_view.borrow().clone()
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
    pub fn has_focus(self) -> bool {
        if let Some(hwnd) = self.containing_wnd() {
            if let Some(view) = &*hwnd.wnd.focused_view.borrow() {
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
    pub fn improper_subview_has_focus(self) -> bool {
        if let Some(hwnd) = self.containing_wnd() {
            if let Some(view) = &*hwnd.wnd.focused_view.borrow() {
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
                    let mut path = ArrayVec::new();
                    view.as_ref().get_path(&mut path);

                    view.view
                        .listener
                        .borrow()
                        .focus_lost(wnd.wm, view.as_ref());

                    for hview in path.iter() {
                        hview
                            .view
                            .listener
                            .borrow()
                            .focus_leave(wnd.wm, hview.as_ref());
                    }
                }
            }
        }
    }
}
