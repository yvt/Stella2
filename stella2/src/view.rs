use harmony::Elem;
use log::trace;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
use tcw3::{
    pal,
    pal::prelude::*,
    ui::layouts::TableLayout,
    ui::theming,
    ui::views::{split::SplitDragListener, Label, Split},
    ui::AlignFlags,
    uicore::{HView, HWnd, ViewFlags, WndListener},
};

use crate::model;

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
    wnd_state: RefCell<Elem<model::WndState>>,
    dispatch: RefCell<Box<dyn Fn(model::WndAction)>>,
    split_editor: Split,
    split_side: Split,
    toolbar: Rc<toolbar::ToolbarView>,
}

impl WndView {
    pub fn new(wm: pal::Wm, wnd_state: Elem<model::WndState>) -> Rc<Self> {
        let hwnd = HWnd::new(wm);
        let style_manager = theming::Manager::global(wm);

        let toolbar = toolbar::ToolbarView::new(Elem::clone(&wnd_state), style_manager);

        let new_test_view = |text: &str| {
            let wrapper = HView::new(ViewFlags::default());
            wrapper.set_layout(
                TableLayout::new(Some((
                    Label::new(style_manager).with_text(text).into_view(),
                    [0, 0],
                    AlignFlags::TOP | AlignFlags::LEFT,
                )))
                .with_uniform_margin(4.0),
            );
            wrapper
        };

        let log_view = new_test_view("log: todo!");

        let editor_view = new_test_view("editor: todo!");

        let split_editor = Split::new(style_manager, true, Some(1));
        split_editor.set_value(wnd_state.editor_height);
        split_editor.set_subviews([log_view, editor_view]);

        // TODO: Toogle sidebar based on `WndState::sidebar_visible`
        let sidebar_view = new_test_view("sidebar: todo!");

        let split_side = Split::new(style_manager, false, Some(0));
        split_side.set_value(wnd_state.sidebar_width);
        split_side.set_subviews([sidebar_view, split_editor.view().clone()]);

        let main_layout = TableLayout::stack_vert(vec![
            (toolbar.view().clone(), AlignFlags::JUSTIFY),
            (split_side.view().clone(), AlignFlags::JUSTIFY),
        ]);

        hwnd.content_view().set_layout(main_layout);

        hwnd.set_listener(WndViewWndListener);
        hwnd.set_visibility(true);

        let this = Rc::new(Self {
            _hwnd: hwnd,
            wnd_state: RefCell::new(wnd_state),
            dispatch: RefCell::new(Box::new(|_| {})),
            split_editor,
            split_side,
            toolbar,
        });

        // Event handlers
        {
            let this_weak = Rc::downgrade(&this);
            this.toolbar.set_dispatch(move |wnd_action| {
                if let Some(this) = this_weak.upgrade() {
                    this.dispatch.borrow()(wnd_action);
                }
            });
        }
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

        this
    }

    fn set_dispatch(&self, cb: impl Fn(model::WndAction) + 'static) {
        *self.dispatch.borrow_mut() = Box::new(cb);
    }

    fn poll(&self, new_wnd_state: &Elem<model::WndState>) {
        let mut wnd_state = self.wnd_state.borrow_mut();

        if Elem::ptr_eq(&wnd_state, new_wnd_state) {
            return;
        }

        trace!("New window state: {:?}", new_wnd_state);

        *wnd_state = Elem::clone(new_wnd_state);

        self.split_editor.set_value(new_wnd_state.editor_height);
        self.split_side.set_value(new_wnd_state.sidebar_width);

        self.toolbar.poll(new_wnd_state);
    }
}

struct WndViewWndListener;

impl WndListener for WndViewWndListener {
    fn close(&self, wm: pal::Wm, _: &HWnd) {
        wm.terminate();
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
