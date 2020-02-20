use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use subscriber_list::SubscriberList;

use tcw3::{
    pal,
    ui::layouts::FillLayout,
    uicore::{HView, HWndRef, Sub, ViewListener, WndCb},
};

pub struct DpiScaleWatcher {
    shared: Rc<Shared>,
    view: HView,
}

struct Shared {
    handlers: RefCell<SubscriberList<WndCb>>,
}

impl DpiScaleWatcher {
    pub fn new(subview: HView) -> Self {
        let shared = Rc::new(Shared {
            handlers: RefCell::new(SubscriberList::new()),
        });

        let view = HView::new(Default::default());
        view.set_layout(FillLayout::new(subview));
        view.set_listener(DpiScaleWatcherViewListener {
            shared: Rc::clone(&shared),
            sub: Cell::default(),
        });

        Self { shared, view }
    }

    pub fn view(&self) -> &HView {
        &self.view
    }

    pub fn subscribe_dpi_scale_changed(&self, cb: WndCb) -> Sub {
        self.shared.handlers.borrow_mut().insert(cb).untype()
    }
}

struct DpiScaleWatcherViewListener {
    shared: Rc<Shared>,
    sub: Cell<Sub>,
}

impl ViewListener for DpiScaleWatcherViewListener {
    fn mount(&self, _: pal::Wm, _: &HView, wnd: HWndRef<'_>) {
        let shared = Rc::clone(&self.shared);
        self.sub
            .set(wnd.subscribe_dpi_scale_changed(Box::new(move |wm, wnd| {
                for cb in shared.handlers.borrow().iter() {
                    cb(wm, wnd);
                }
            })));
    }

    fn unmount(&self, _: pal::Wm, _: &HView) {
        self.sub.take().unsubscribe().unwrap();
    }
}
