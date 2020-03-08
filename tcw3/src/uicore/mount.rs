use super::{HViewRef, HWndRef, ViewDirtyFlags};
use crate::pal::Wm;

impl HViewRef<'_> {
    /// Call `ViewListener::mount` as necessary.
    pub(super) fn call_pending_mount_if_dirty(self, wm: Wm, hwnd: HWndRef<'_>) {
        let dirty = &self.view.dirty;

        if dirty.get().contains(ViewDirtyFlags::MOUNTED) {
            if !dirty.get().contains(ViewDirtyFlags::MOUNT) {
                return;
            }
            dirty.set(dirty.get() - ViewDirtyFlags::MOUNT);

            // This view is mounted, but some of the subviews might not be.
            for subview in self.view.layout.borrow().subviews().iter() {
                subview.as_ref().call_pending_mount_if_dirty(wm, hwnd);
            }
        } else {
            // This view is not mounted yet. So are the subviews.
            self.call_pending_mount(wm, hwnd);
        }
    }

    /// Call `ViewListener::mount` as necessary. It ignores the `MOUNT` dirty bit.
    fn call_pending_mount(self, wm: Wm, hwnd: HWndRef<'_>) {
        let dirty = &self.view.dirty;
        dirty.set(dirty.get() - ViewDirtyFlags::MOUNT);

        if !dirty.get().contains(ViewDirtyFlags::MOUNTED) {
            dirty.set(dirty.get() | ViewDirtyFlags::MOUNTED);

            // `mount`'s precondition requires that the window is materialized
            // at this point
            debug_assert!(hwnd.pal_hwnd().is_some());

            self.view.listener.borrow().mount(wm, self, hwnd);
        }

        for subview in self.view.layout.borrow().subviews().iter() {
            subview.as_ref().call_pending_mount(wm, hwnd);
        }
    }

    /// Unmount this view and its all subviews.
    pub(super) fn call_unmount(self, wm: Wm) {
        let dirty = &self.view.dirty;

        if !dirty.get().contains(ViewDirtyFlags::MOUNTED) {
            return;
        }
        dirty.set(dirty.get() - ViewDirtyFlags::MOUNTED);

        // `unmount`'s precondition requires that, if the window is existent, the
        // window is still materialized at this point. However, it's costly to
        // assert that here because to do that `View` would have to have
        // another weak reference to the window. As asserted below, `superview`
        // no longer leads to the window:
        debug_assert!(self.containing_wnd().is_none());

        self.view.listener.borrow().unmount(wm, self);

        self.view.layers.borrow_mut().clear();

        for subview in self.view.layout.borrow().subviews().iter() {
            subview.as_ref().call_unmount(wm);
        }
    }
}
