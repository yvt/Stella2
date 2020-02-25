//! Keyboard events
use arrayvec::ArrayVec;

use super::{HView, HViewRef, HWndRef, ViewFlags, WmExt, Wnd};

impl HWndRef<'_> {
    /// Focus the specified view.
    pub fn set_focused_view(self, view: Option<HView>) {
        // Assign `view` to `new_focused_view`. If it was empty, register an
        // update handler to call `flush_focused_view` later.
        if self.wnd.new_focused_view.replace(Some(view)).is_none() {
            let hwnd = self.cloned();
            self.wnd.wm.invoke_on_update(move |_| {
                hwnd.as_ref().flush_focused_view();
            });
        }
    }

    fn flush_focused_view(self) {
        let new_focused_view = self.wnd.new_focused_view.take().unwrap();
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

        // TODO: This section was copied from `mouse.rs`. De-duplicate.
        use super::MAX_VIEW_DEPTH;
        let mut path1 = ArrayVec::new();
        let mut path2 = ArrayVec::new();

        get_path(&focused_view_cell, &mut path1);
        get_path(&new_focused_view, &mut path2);

        fn get_path(hview: &Option<HView>, out_path: &mut ArrayVec<[HView; MAX_VIEW_DEPTH]>) {
            if let Some(hview) = hview {
                hview
                    .as_ref()
                    .for_each_ancestor(|hview| out_path.push(hview));
            }
        }

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

    /// Steal a keyboard focus from the specified view. Does *not* call
    /// `focus_(leave|lost)`.
    ///
    /// If `subview` is `true`, the subviews of `view` are also affected.
    pub(super) fn defocus_subviews(self, wnd: &Wnd, subview: bool) {
        let mut focused_view_cell = wnd.focused_view.borrow_mut();

        let cancel_drag = if let Some(view) = &*focused_view_cell {
            if subview {
                view.as_ref().is_improper_subview_of(self)
            } else {
                view.as_ref() == self
            }
        } else {
            false
        };

        if cancel_drag {
            *focused_view_cell = None;
        }
    }
}
