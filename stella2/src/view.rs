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
    ui::AlignFlags,
    uicore::{HWnd, HWndRef, WndListener, WndStyleFlags},
};

use crate::{
    config::{profile::Profile, viewpersistence},
    model,
    stylesheet::{self, elem_id},
};

mod channellist;
mod dpiscalewatcher;
mod logview;
mod splitutils;
mod toolbar;

pub struct AppView {
    wm: pal::Wm,
    profile: &'static Profile,
    state: RefCell<Elem<model::AppState>>,
    pending_actions: RefCell<Vec<model::AppAction>>,
    persist_sched: viewpersistence::PersistenceScheduler,
    main_wnd: Rc<WndView>,
}

impl AppView {
    pub fn new(wm: pal::Wm, profile: &'static Profile) -> Rc<Self> {
        let mut state = Elem::new(model::AppState::new());

        // Restore the app state from the user profile
        state = viewpersistence::restore_state(profile, state);

        let persist_sched = viewpersistence::PersistenceScheduler::new(&state);

        let main_wnd = WndView::new(wm, Elem::clone(&state.main_wnd));

        let this = Rc::new(Self {
            wm,
            profile,
            main_wnd,
            state: RefCell::new(state),
            pending_actions: RefCell::new(Vec::new()),
            persist_sched,
        });

        let this_weak = Rc::downgrade(&this);
        this.main_wnd.set_dispatch(move |wnd_action| {
            Self::dispatch_weak(&this_weak, model::AppAction::Wnd(wnd_action))
        });

        let this_weak = Rc::downgrade(&this);
        this.main_wnd.set_quit(move || {
            // Persist the state to disk before quitting
            if let Some(this) = this_weak.upgrade() {
                this.persist_sched.flush(wm, &this.state.borrow(), profile);
            }

            wm.terminate();
        });

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

            // Persist the app state
            self.persist_sched
                .handle_update(self.wm, &state, self.profile);
        }

        let state = self.state.borrow();

        self.main_wnd.poll(&state.main_wnd);
    }
}

struct WndView {
    hwnd: HWnd,
    dispatch: RefCell<Box<dyn Fn(model::WndAction)>>,
    quit: RefCell<Box<dyn Fn()>>,
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
        hwnd.set_visibility(true);

        let this = Rc::new(Self {
            hwnd,
            dispatch: RefCell::new(Box::new(|_| {})),
            quit: RefCell::new(Box::new(|| {})),
            main_view,
        });

        // Event handlers
        this.hwnd.set_listener(WndViewWndListener {
            owner: Rc::downgrade(&this),
        });

        let this_weak = Rc::downgrade(&this);
        this.main_view.subscribe_dispatch(Box::new(move |action| {
            if let Some(this) = this_weak.upgrade() {
                this.dispatch.borrow()(action);
            }
        }));

        let this_weak = Rc::downgrade(&this);
        this.hwnd.subscribe_focus(Box::new(move |_, _| {
            if let Some(this) = this_weak.upgrade() {
                this.update_focus();
            }
        }));

        this
    }

    fn set_dispatch(&self, cb: impl Fn(model::WndAction) + 'static) {
        *self.dispatch.borrow_mut() = Box::new(cb);
    }

    fn set_quit(&self, cb: impl Fn() + 'static) {
        *self.quit.borrow_mut() = Box::new(cb);
    }

    fn update_focus(&self) {
        let is_focused = self.hwnd.is_focused();
        if stylesheet::ENABLE_BACKDROP_BLUR {
            self.hwnd.set_style_flags(if is_focused {
                WndStyleFlags::default() | WndStyleFlags::TRANSPARENT_BACKDROP_BLUR
            } else {
                WndStyleFlags::default()
            });
        }
        self.main_view.set_wnd_focused(is_focused);
    }

    fn poll(&self, new_wnd_state: &Elem<model::WndState>) {
        self.main_view.set_wnd_state(new_wnd_state.clone());
    }
}

struct WndViewWndListener {
    owner: Weak<WndView>,
}

impl WndListener for WndViewWndListener {
    fn close(&self, _: pal::Wm, _: HWndRef<'_>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.quit.borrow()();
        }
    }
}

stella2_meta::designer_impl! {
    crate::view::MainView
}

impl MainView {
    /// Handle `init` event.
    fn init(&self) {}
}

stella2_meta::designer_impl! {
    crate::view::PlaceholderView
}
