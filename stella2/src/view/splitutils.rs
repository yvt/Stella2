use std::{cell::RefCell, rc::Rc};
use subscriber_list::SubscriberList;
use tcw3::{
    ui::views::{split::SplitDragListener, Split},
    uicore::Sub,
};

pub type Cb = Box<dyn Fn()>;

pub struct SplitEventAdapter {
    shared: Rc<SplitEventAdapterShared>,
}

struct SplitEventAdapterShared {
    handlers: RefCell<SubscriberList<Cb>>,
}

impl SplitEventAdapter {
    pub fn new(view: &Split) -> Self {
        let shared = Rc::new(SplitEventAdapterShared {
            handlers: RefCell::new(SubscriberList::new()),
        });

        let shared_weak = Rc::downgrade(&shared);
        view.set_on_drag(move |_| {
            let shared_weak = shared_weak.clone();
            Box::new(OnDrop::new(move || {
                if let Some(shared) = shared_weak.upgrade() {
                    // Raise `drag_complete`
                    for cb in shared.handlers.borrow().iter() {
                        cb();
                    }
                }
            }))
        });

        Self { shared }
    }

    pub fn subscribe_drag_complete(&self, cb: Cb) -> Sub {
        self.shared.handlers.borrow_mut().insert(cb).untype()
    }
}

struct OnDrop<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> OnDrop<F> {
    fn new(x: F) -> Self {
        Self(Some(x))
    }
}

/// The inner function is called when `<Self as SplitDragListener>` is dropped,
/// i.e., a mouse drag gesture is finished
impl<F: FnOnce()> SplitDragListener for OnDrop<F> {}

impl<F: FnOnce()> Drop for OnDrop<F> {
    fn drop(&mut self) {
        (self.0.take().unwrap())();
    }
}
