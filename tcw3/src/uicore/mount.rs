use super::{HView, HWnd, ViewDirtyFlags};
use crate::pal::WM;

impl HView {
    /// Call `ViewListener::mount` as necessary.
    pub(super) fn call_pending_mount_if_dirty(&self, wm: &WM, hwnd: &HWnd) {
        let dirty = &self.view.dirty;

        if !dirty.get().contains(ViewDirtyFlags::MOUNT) {
            return;
        }
        dirty.set(dirty.get() - ViewDirtyFlags::MOUNT);

        if dirty.get().contains(ViewDirtyFlags::MOUNTED) {
            // This view is mounted, but some of the subviews might not be.
            for subview in self.view.layout.borrow().subviews().iter() {
                subview.call_pending_mount_if_dirty(wm, hwnd);
            }
        } else {
            // This view is not mounted yet. So are the subviews.
            self.call_pending_mount(wm, hwnd);
        }
    }

    /// Call `ViewListener::mount` as necessary. It ignores the `MOUNT` dirty bit.
    fn call_pending_mount(&self, wm: &WM, hwnd: &HWnd) {
        let dirty = &self.view.dirty;
        dirty.set(dirty.get() - ViewDirtyFlags::MOUNT);

        if !dirty.get().contains(ViewDirtyFlags::MOUNTED) {
            dirty.set(dirty.get() | ViewDirtyFlags::MOUNTED);

            self.view.listener.borrow().mount(wm, self, hwnd);
        }

        for subview in self.view.layout.borrow().subviews().iter() {
            subview.call_pending_mount(wm, hwnd);
        }
    }

    /// Unmount this view and its all subviews.
    pub(super) fn call_unmount(&self, wm: &WM) {
        let dirty = &self.view.dirty;

        if !dirty.get().contains(ViewDirtyFlags::MOUNTED) {
            return;
        }
        dirty.set(dirty.get() - ViewDirtyFlags::MOUNTED);

        self.view.listener.borrow().unmount(wm, self);

        for subview in self.view.layout.borrow().subviews().iter() {
            subview.call_unmount(wm);
        }
    }
}
