use harmony::Elem;
use log::trace;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
use tcw3::{
    pal,
    pal::prelude::*,
    ui::layouts::{FillLayout, TableLayout},
    ui::theming::{self, ClassSet},
    ui::views::split::SplitDragListener,
    ui::AlignFlags,
    uicore::{HWnd, WndListener},
};

use crate::{model, stylesheet::elem_id};

mod toolbar;

pub struct AppView {
    wm: pal::Wm,
    state: RefCell<Elem<model::AppState>>,
    pending_actions: RefCell<Vec<model::AppAction>>,
    main_wnd: Rc<WndView>,
}

impl AppView {
    pub fn new(wm: pal::Wm) -> Rc<Self> {
        let state = model::AppState::new();

        let main_wnd = WndView::new(wm, Elem::clone(&state.main_wnd));

        let this = Rc::new(Self {
            wm,
            main_wnd,
            state: RefCell::new(Elem::new(state)),
            pending_actions: RefCell::new(Vec::new()),
        });

        {
            let this_weak = Rc::downgrade(&this);
            this.main_wnd.set_dispatch(move |wnd_action| {
                Self::dispatch_weak(&this_weak, model::AppAction::Wnd(wnd_action))
            });
        }

        this
    }

    fn dispatch_weak(this_weak: &Weak<Self>, action: model::AppAction) {
        if let Some(this) = this_weak.upgrade() {
            Self::dispatch(&this, action);
        }
    }

    fn dispatch(this: &Rc<Self>, action: model::AppAction) {
        trace!("Dispatching the action: {:?}", action);

        let mut pending_actions = this.pending_actions.borrow_mut();

        pending_actions.push(action);

        if pending_actions.len() == 1 {
            // Schedule polling
            let this = Rc::clone(this);
            this.wm.invoke(move |_| this.poll());
        }
    }

    fn poll(&self) {
        // Update the state
        {
            let mut state = self.state.borrow_mut();
            let mut pending_actions = self.pending_actions.borrow_mut();

            let mut new_state = Elem::clone(&*state);
            for action in pending_actions.drain(..) {
                new_state = model::AppState::reduce(new_state, &action);
            }
            *state = new_state;
        }

        let state = self.state.borrow();

        self.main_wnd.poll(&state.main_wnd);
    }
}

struct WndView {
    _hwnd: HWnd,
    dispatch: RefCell<Box<dyn Fn(model::WndAction)>>,
    main_view: MainView,
}

impl WndView {
    pub fn new(wm: pal::Wm, wnd_state: Elem<model::WndState>) -> Rc<Self> {
        let hwnd = HWnd::new(wm);
        let style_manager = theming::Manager::global(wm);

        let main_view = MainViewBuilder::new()
            .with_wm(wm)
            .with_wnd_state(Elem::clone(&wnd_state))
            .with_style_manager(style_manager)
            .build();

        hwnd.content_view()
            .set_layout(FillLayout::new(main_view.view().clone()));

        hwnd.set_caption("Stella 2");
        hwnd.set_listener(WndViewWndListener);
        hwnd.set_visibility(true);

        let this = Rc::new(Self {
            _hwnd: hwnd,
            dispatch: RefCell::new(Box::new(|_| {})),
            main_view,
        });

        // Event handlers
        let this_weak = Rc::downgrade(&this);
        this.main_view.subscribe_dispatch(Box::new(move |action| {
            if let Some(this) = this_weak.upgrade() {
                this.dispatch.borrow()(action);
            }
        }));

        this
    }

    fn set_dispatch(&self, cb: impl Fn(model::WndAction) + 'static) {
        *self.dispatch.borrow_mut() = Box::new(cb);
    }

    fn poll(&self, new_wnd_state: &Elem<model::WndState>) {
        self.main_view.set_wnd_state(new_wnd_state.clone());
    }
}

struct WndViewWndListener;

impl WndListener for WndViewWndListener {
    fn close(&self, wm: pal::Wm, _: &HWnd) {
        wm.terminate();
    }
}

stella2_meta::designer_impl! {
    crate::view::MainView
}

impl MainView {
    /// Handle `init` event.
    fn init(&self) {
        // TODO: there is no way to get a weak reference at the moment
        /*
        {
            let this_weak = Rc::downgrade(&this);
            this.split_editor.set_on_drag(move |_| {
                let this_weak = this_weak.clone();
                Box::new(OnDrop::new(move || {
                    if let Some(this) = this_weak.upgrade() {
                        let new_size = this.split_editor.value();
                        this.dispatch.borrow()(model::WndAction::SetEditorHeight(new_size));
                    }
                }))
            });
        }
        {
            let this_weak = Rc::downgrade(&this);
            this.split_side.set_on_drag(move |_| {
                let this_weak = this_weak.clone();
                Box::new(OnDrop::new(move || {
                    if let Some(this) = this_weak.upgrade() {
                        let new_size = this.split_side.value();
                        this.dispatch.borrow()(model::WndAction::SetSidebarWidth(new_size));
                    }
                }))
            });
        }
        */
    }
}

stella2_meta::designer_impl! {
    crate::view::PlaceholderView
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
