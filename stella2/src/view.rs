use harmony::Elem;
use log::trace;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use tcw3::{
    pal,
    pal::prelude::*,
    ui::layouts::FillLayout,
    ui::theming,
    uicore::{ActionId, ActionStatus, HWnd, HWndRef, WndListener, WndStyleFlags},
};

use crate::{
    config::{profile::Profile, viewpersistence},
    model, stylesheet,
};

mod channellist;
mod dpiscalewatcher;
mod global;
mod logview;
mod prefwnd;
mod radiolist;
mod splitutils;
mod tabbar;
mod toolbar;

pub struct AppView {
    wm: pal::Wm,
    profile: &'static Profile,
    state: RefCell<Elem<model::AppState>>,
    pending_actions: RefCell<Vec<model::AppAction>>,
    persist_sched: viewpersistence::PersistenceScheduler,
    main_wnd: Rc<WndView>,
    pref_wnd: Cell<Option<Rc<prefwnd::PrefWndView>>>,
}

impl AppView {
    pub fn new(wm: pal::Wm, profile: &'static Profile) -> Rc<Self> {
        let mut state = Elem::new(model::AppState::new());

        // Restore the app state from the user profile
        state = viewpersistence::restore_state(profile, state);

        let persist_sched = viewpersistence::PersistenceScheduler::new(&state);

        global::set_main_menu(wm);

        let main_wnd = WndView::new(wm, Elem::clone(&state.main_wnd));

        let this = Rc::new(Self {
            wm,
            profile,
            main_wnd,
            state: RefCell::new(state),
            pending_actions: RefCell::new(Vec::new()),
            persist_sched,
            pref_wnd: Cell::new(None),
        });

        let this_weak = Rc::downgrade(&this);
        this.main_wnd
            .set_dispatch(move |app_action| Self::dispatch_weak(&this_weak, app_action));

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

    fn poll(self: Rc<Self>) {
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

        match (cell_is_some(&self.pref_wnd), state.pref_visible) {
            (false, true) => {
                let pref_wnd = prefwnd::PrefWndView::new(self.wm);

                let this_weak = Rc::downgrade(&self);
                pref_wnd
                    .set_dispatch(move |app_action| Self::dispatch_weak(&this_weak, app_action));

                self.pref_wnd.set(Some(pref_wnd));
            }
            (true, false) => {
                self.pref_wnd.set(None);
            }
            _ => {}
        }
    }
}

struct WndView {
    hwnd: HWnd,
    dispatch: RefCell<Box<dyn Fn(model::AppAction)>>,
    quit: RefCell<Box<dyn Fn()>>,
    wnd_state: RefCell<Elem<model::WndState>>,
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
        Self::update_wnd_style_flags(hwnd.as_ref(), false);
        hwnd.set_visibility(true);

        let this = Rc::new(Self {
            hwnd,
            dispatch: RefCell::new(Box::new(|_| {})),
            quit: RefCell::new(Box::new(|| {})),
            wnd_state: RefCell::new(wnd_state),
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
        this.main_view.subscribe_close(Box::new(move || {
            if let Some(this) = this_weak.upgrade() {
                this.quit.borrow()();
            }
        }));

        let this_weak = Rc::downgrade(&this);
        this.hwnd.subscribe_focus(Box::new(move |_, _| {
            if let Some(this) = this_weak.upgrade() {
                this.update_focus();
            }
        }));

        this.update_focus();

        this
    }

    fn set_dispatch(&self, cb: impl Fn(model::AppAction) + 'static) {
        *self.dispatch.borrow_mut() = Box::new(cb);
    }

    fn set_quit(&self, cb: impl Fn() + 'static) {
        *self.quit.borrow_mut() = Box::new(cb);
    }

    fn update_wnd_style_flags(hwnd: HWndRef, is_focused: bool) {
        hwnd.set_style_flags(
            if stylesheet::ENABLE_BACKDROP_BLUR && is_focused {
                WndStyleFlags::default() | WndStyleFlags::TRANSPARENT_BACKDROP_BLUR
            } else {
                WndStyleFlags::default()
            } | WndStyleFlags::FULL_SIZE_CONTENT,
        );
    }

    fn update_focus(&self) {
        let is_focused = self.hwnd.is_focused();
        if stylesheet::ENABLE_BACKDROP_BLUR {
            Self::update_wnd_style_flags(self.hwnd.as_ref(), is_focused);
        }
        self.main_view.set_wnd_focused(is_focused);
    }

    fn poll(&self, new_wnd_state: &Elem<model::WndState>) {
        *self.wnd_state.borrow_mut() = new_wnd_state.clone();

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

    fn interpret_event(
        &self,
        _: pal::Wm,
        _: HWndRef<'_>,
        ctx: &mut tcw3::uicore::InterpretEventCtx<'_>,
    ) {
        global::interpret_event(ctx);
    }

    fn validate_action(&self, _: pal::Wm, _: HWndRef<'_>, action: ActionId) -> ActionStatus {
        let mut status = ActionStatus::empty();
        match action {
            global::QUIT | global::SHOW_PREF => {
                status = ActionStatus::VALID | ActionStatus::ENABLED;
            }
            global::TOGGLE_SIDEBAR => {
                status = ActionStatus::VALID | ActionStatus::ENABLED;
                if let Some(owner) = self.owner.upgrade() {
                    status.set(
                        ActionStatus::CHECKED,
                        owner.wnd_state.borrow().sidebar_visible,
                    );
                }
            }
            _ => {}
        }
        status
    }

    fn perform_action(&self, _: pal::Wm, _: HWndRef<'_>, action: ActionId) {
        let owner = if let Some(owner) = self.owner.upgrade() {
            owner
        } else {
            return;
        };

        match action {
            global::TOGGLE_SIDEBAR => {
                owner.dispatch.borrow()(model::AppAction::Wnd(model::WndAction::ToggleSidebar));
            }
            global::SHOW_PREF => {
                owner.dispatch.borrow()(model::AppAction::TogglePref);
            }
            global::QUIT => {
                owner.quit.borrow()();
            }
            _ => {}
        }
    }
}

stella2_meta::designer_impl! {
    crate::view::MainView
}

fn cell_is_some<T>(cell: &Cell<Option<T>>) -> bool {
    let inner = cell.take();
    let x = inner.is_some();
    cell.set(inner);
    x
}
