//! Keyboard events
use arrayvec::ArrayVec;
use log::trace;

use super::{ActionId, ActionStatus, HView, HViewRef, HWndRef, KeyEvent, ViewFlags, Wnd};
use crate::{pal, pal::Wm};

impl HWndRef<'_> {
    /// Focus the specified view.
    ///
    /// If `new_focused_view` does not have `TAB_STOP`, the method searches for
    /// a closest view with `TAB_STOP` from `new_focused_view`'s ancestors. If
    /// there isn't such a view, this method does nothing.
    pub fn set_focused_view(self, mut new_focused_view: Option<HView>) {
        let focused_view_cell = self.wnd.focused_view.borrow();

        if new_focused_view == *focused_view_cell {
            return;
        }

        trace!("{:?}: set_focused_view({:?})", self, new_focused_view);

        if let Some(view) = &mut new_focused_view {
            debug_assert_eq!(
                view.containing_wnd().as_ref().map(|hw| hw.as_ref()),
                Some(self),
                "the window does not contain `new_focused_view`"
            );

            // Find the closest view with `TAB_STOP`
            loop {
                if view.view.flags.get().contains(ViewFlags::TAB_STOP) {
                    break;
                }

                let maybe_superview = (view.view.superview.borrow())
                    .view()
                    .and_then(|weak| weak.upgrade());
                if let Some(superview) = maybe_superview {
                    *view = HView { view: superview };
                } else {
                    trace!(
                        "{:?}: Rejecting `set_focused_view` because the view \
                        doesn't have a focusable ancestor",
                        self
                    );
                    return;
                }
            }
        }

        if !self.is_focused() {
            trace!(
                "{:?}: The window is inactive, not raising view focus events",
                self
            );

            drop(focused_view_cell);
            let mut focused_view_cell = self.wnd.focused_view.borrow_mut();

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

        drop(focused_view_cell);
        let mut focused_view_cell = self.wnd.focused_view.borrow_mut();

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
                trace!(
                    "{:?}: Raising `focus_(got|enter)` for the ancestors of {:?} \
                    because the window became active",
                    self,
                    hview,
                );
                hview.invoke_focus_got_enter_for_ancestors(self.wnd.wm);
            } else {
                trace!(
                    "{:?}: Raising `focus_(lost|leave)` for the ancestors of {:?} \
                    because the window became inactive",
                    self,
                    hview,
                );
                hview.invoke_focus_lost_leave_for_ancestors(self.wnd.wm);
            }
        }
    }

    /// The core implementation of `pal::WndListener::{validate_action, perform_action}`.
    pub(super) fn handle_action(self, action: ActionId, perform: bool) -> ActionStatus {
        let mut focused_view = self.wnd.focused_view.borrow().clone();
        let wm = self.wnd.wm;

        while let Some(hview) = focused_view {
            let listener = hview.view.listener.borrow();

            // Does this view recognize the action?
            let status = listener.validate_action(wm, hview.as_ref(), action);
            if status.contains(ActionStatus::VALID) {
                if perform && status.contains(ActionStatus::ENABLED) {
                    listener.perform_action(wm, hview.as_ref(), action);
                }
                return status;
            }

            drop(listener);

            // Get the parent of the view
            focused_view = hview
                .view
                .superview
                .borrow()
                // If it's a superview...
                .view()
                // Get a strong reference to the view
                .and_then(|weak| weak.upgrade())
                // Form `HView`, the public handle type
                .map(|view| HView { view });
        }

        // Does this window recognize the action?
        let listener = self.wnd.listener.borrow();
        let status = listener.validate_action(wm, self, action);
        if status.contains(ActionStatus::VALID) {
            if perform && status.contains(ActionStatus::ENABLED) {
                listener.perform_action(wm, self, action);
            }
        }
        status
    }

    /// The core implementation of `pal::WndListener::{key_down, key_up}`.
    pub(super) fn handle_key(self, e: &KeyEvent<'_>, up: bool) -> bool {
        let mut focused_view = self.wnd.focused_view.borrow().clone();
        let wm = self.wnd.wm;

        while let Some(hview) = focused_view {
            let listener = hview.view.listener.borrow();

            let handled = if up {
                listener.key_up(wm, hview.as_ref(), e)
            } else {
                listener.key_down(wm, hview.as_ref(), e)
            };

            if handled {
                return true;
            }

            drop(listener);

            // Get the parent of the view
            focused_view = hview
                .view
                .superview
                .borrow()
                // If it's a superview...
                .view()
                // Get a strong reference to the view
                .and_then(|weak| weak.upgrade())
                // Form `HView`, the public handle type
                .map(|view| HView { view });
        }

        // Does this window recognize the event?
        let listener = self.wnd.listener.borrow();
        let handled = if up {
            listener.key_up(wm, self, e)
        } else {
            listener.key_down(wm, self, e)
        };
        drop(listener);
        if handled {
            return true;
        }

        // Check tab key only if it's a key-down event
        if up {
            return false;
        }

        // Check tab key
        const TAB_FORWARD: ActionId = 0;
        const TAB_BACKWARD: ActionId = 1;
        static TAB_ACCEL_TABLE: pal::AccelTable = pal::accel_table![
            (TAB_FORWARD, windows("Tab"), macos("Tab"), gtk("Tab")),
            (
                TAB_BACKWARD,
                windows("Shift+Tab"),
                macos("Shift+Tab"),
                gtk("Shift+Tab")
            ),
        ];
        if let Some(code) = e.translate_accel(&TAB_ACCEL_TABLE) {
            trace!(
                "Interpreted the unhandled key event as {}",
                ["TAB_FORWARD", "TAB_BACKWARD"][code as usize]
            );

            let mut focused_view = self.wnd.focused_view.borrow().clone();
            let root_view = self.content_view();

            trace!("... The currently focused view is {:?}", focused_view);

            match code {
                TAB_FORWARD => {
                    if let Some(view) = focused_view {
                        focused_view = view.tab_order_next_view();
                    }

                    // If there are no more views in the tab order or we didn't
                    // have a focused view in the first place, start over
                    if focused_view.is_none() {
                        focused_view = root_view.tab_order_first_view();
                    }
                }
                TAB_BACKWARD => {
                    if let Some(view) = focused_view {
                        focused_view = view.tab_order_prev_view();
                    }

                    // If there are no more views in the tab order or we didn't
                    // have a focused view in the first place, start over
                    if focused_view.is_none() {
                        focused_view = root_view.tab_order_last_view();
                    }
                }
                _ => unreachable!(),
            }

            if let Some(view) = focused_view {
                trace!("... Transferring the keyboard focus to {:?}", view);
                view.focus();
                return true;
            } else {
                trace!("... Couldn't find a view to transfer the keyboard focus to");
            }
        }

        false
    }
}

impl HViewRef<'_> {
    /// Focus the view.
    pub fn focus(self) {
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
