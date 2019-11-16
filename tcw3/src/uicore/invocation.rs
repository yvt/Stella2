use crate::pal::{iface::Wm as WmTrait, MtSticky, Wm};
use std::{cell::RefCell, collections::LinkedList};

static ON_UPDATE_DISPATCHES: MtSticky<RefCell<LinkedList<Box<dyn FnOnce(Wm)>>>> = {
    // This is safe because the created value does not contain an actual
    // unsendable content (`Box<dyn FnOnce(Wm)>`) yet
    unsafe { MtSticky::new_unchecked(RefCell::new(LinkedList::new())) }
};

/// Implements `WmExt::invoke_on_update`.
pub fn invoke_on_update(wm: Wm, f: impl FnOnce(Wm) + 'static) {
    invoke_on_update_inner(wm, Box::new(f));
}

fn invoke_on_update_inner(wm: Wm, f: Box<dyn FnOnce(Wm)>) {
    let mut queue = ON_UPDATE_DISPATCHES.get_with_wm(wm).borrow_mut();
    if queue.is_empty() {
        wm.invoke(process_pending_invocations);
    }
    queue.push_back(f);
}

/// Process pending invocations.
pub fn process_pending_invocations(wm: Wm) {
    loop {
        let f = ON_UPDATE_DISPATCHES
            .get_with_wm(wm)
            .borrow_mut()
            .pop_front();
        if let Some(f) = f {
            f(wm);
        } else {
            break;
        }
    }
}
