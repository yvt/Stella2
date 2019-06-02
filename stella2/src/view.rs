use harmony::Elem;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
use tcw3::{
    pal,
    pal::prelude::*,
    ui::layouts::{FillLayout, TableLayout},
    ui::views::{split::SplitDragListener, Label, Split},
    ui::AlignFlags,
    uicore::{HView, HWnd, ViewFlags},
};

use crate::model;

pub struct AppView {
    wm: pal::WM,
    state: RefCell<Elem<model::AppState>>,
    pending_actions: RefCell<Vec<model::AppAction>>,
    main_wnd: Rc<WndView>,
}

impl AppView {
    pub fn new(wm: pal::WM) -> Rc<Self> {
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
            *this.main_wnd.dispatch.borrow_mut() = Box::new(move |wnd_action| {
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
        dbg!(&action);

        let mut pending_actions = this.pending_actions.borrow_mut();

        pending_actions.push(action);

        if pending_actions.len() == 0 {
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
    split_editor: RefCell<Split>,
    split_side: RefCell<Split>,
}

impl WndView {
    pub fn new(wm: pal::WM, wnd_state: Elem<model::WndState>) -> Rc<Self> {
        let hwnd = HWnd::new(wm);

        let new_test_view = |text: &str| {
            // Fortunately, `Label` can operate even if it outlives the
            // controller (`Label`)
            let mut label = Label::new();
            label.set_text(text);

            let wrapper = HView::new(ViewFlags::default());
            wrapper.set_layout(
                TableLayout::new(Some((
                    label.view().clone(),
                    [0, 0],
                    AlignFlags::TOP | AlignFlags::LEFT,
                )))
                .with_uniform_margin(4.0),
            );
            wrapper
        };

        let log_view = new_test_view("log: todo!");

        let editor_view = new_test_view("editor: todo!");

        let mut split_editor = Split::new(true, Some(1));
        split_editor.set_value(wnd_state.editor_height);
        split_editor.set_subviews([log_view, editor_view]);
        // TODO: call dispatch when the split is moved

        let sidebar_view = new_test_view("sidebar: todo!");

        let mut split_side = Split::new(false, Some(0));
        split_side.set_value(wnd_state.sidebar_width);
        split_side.set_subviews([sidebar_view, split_editor.view().clone()]);
        // TODO: call dispatch when the split is moved

        // TODO: tool bar

        hwnd.content_view()
            .set_layout(FillLayout::new(split_side.view().clone()));

        hwnd.set_visibility(true);

        let this = Rc::new(Self {
            _hwnd: hwnd,
            wnd_state: RefCell::new(wnd_state),
            dispatch: RefCell::new(Box::new(|_| {})),
            split_editor: RefCell::new(split_editor),
            split_side: RefCell::new(split_side),
        });

        // Event handlers
        {
            let this_weak = Rc::downgrade(&this);
            this.split_editor.borrow_mut().set_on_drag(move |_| {
                let this_weak = this_weak.clone();
                Box::new(OnDrop::new(move || {
                    if let Some(this) = this_weak.upgrade() {
                        let new_size = this.split_editor.borrow().value();
                        this.dispatch.borrow()(model::WndAction::SetEditorHeight(new_size));
                    }
                }))
            });
        }
        {
            let this_weak = Rc::downgrade(&this);
            this.split_side.borrow_mut().set_on_drag(move |_| {
                let this_weak = this_weak.clone();
                Box::new(OnDrop::new(move || {
                    if let Some(this) = this_weak.upgrade() {
                        let new_size = this.split_side.borrow().value();
                        this.dispatch.borrow()(model::WndAction::SetSidebarWidth(new_size));
                    }
                }))
            });
        }

        this
    }

    fn poll(&self, new_wnd_state: &Elem<model::WndState>) {
        let mut wnd_state = self.wnd_state.borrow_mut();

        if Elem::ptr_eq(&wnd_state, new_wnd_state) {
            return;
        }

        *wnd_state = Elem::clone(new_wnd_state);

        self.split_editor
            .borrow_mut()
            .set_value(new_wnd_state.editor_height);
        self.split_side
            .borrow_mut()
            .set_value(new_wnd_state.sidebar_width);
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
