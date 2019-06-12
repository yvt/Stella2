use harmony::Elem;
use std::{cell::RefCell, rc::Rc};
use tcw3::{
    ui::layouts::TableLayout,
    ui::theming,
    ui::views::{Button, Label, Spacer},
    ui::AlignFlags,
    uicore::{HView, ViewFlags},
};

use crate::{model, stylesheet::elem_id};

pub struct ToolbarView {
    container: HView,
    wnd_state: RefCell<Elem<model::WndState>>,
    dispatch: RefCell<Box<dyn Fn(model::WndAction)>>,
    go_back_button: RefCell<Button>,
    go_forward_button: RefCell<Button>,
}

impl ToolbarView {
    pub fn new(
        wnd_state: Elem<model::WndState>,
        style_manager: &'static theming::Manager,
    ) -> Rc<Self> {
        let container = HView::new(ViewFlags::default());

        // TODO: Show/hide the sidebar
        // TODO: Display a dropdown list when the sidebar is hidden

        // TODO: Use toolbar button style
        // TODO: Use icons
        let mut go_back_button = Button::new(style_manager);
        go_back_button.set_class_set(theming::ClassSet::BUTTON | elem_id::GO_BACK);

        let mut go_forward_button = Button::new(style_manager);
        go_forward_button.set_class_set(theming::ClassSet::BUTTON | elem_id::GO_FORWARD);

        // TODO: Search bar
        let search_bar = Label::new(style_manager).with_text("todo: search");

        const MARGIN: f32 = 5.0;

        let main_layout = TableLayout::stack_horz(vec![
            (go_back_button.view().clone(), AlignFlags::JUSTIFY),
            (
                Spacer::new().with_fixed([MARGIN, 0.0]).into_view(),
                AlignFlags::HORZ_JUSTIFY,
            ),
            (go_forward_button.view().clone(), AlignFlags::JUSTIFY),
            (Spacer::new().into_view(), AlignFlags::CENTER), // fill space
            (search_bar.into_view(), AlignFlags::HORZ_JUSTIFY),
        ])
        .with_uniform_margin(MARGIN);

        container.set_layout(main_layout);

        let this = Rc::new(Self {
            container,
            wnd_state: RefCell::new(wnd_state),
            dispatch: RefCell::new(Box::new(|_| {})),
            go_back_button: RefCell::new(go_back_button),
            go_forward_button: RefCell::new(go_forward_button),
        });

        this.go_back_button.borrow_mut().set_on_activate(|_| {
            dbg!();
        });
        this.go_forward_button.borrow_mut().set_on_activate(|_| {
            dbg!();
        });

        this
    }

    pub fn set_dispatch(&self, cb: impl Fn(model::WndAction) + 'static) {
        *self.dispatch.borrow_mut() = Box::new(cb);
    }

    pub fn view(&self) -> &HView {
        &self.container
    }

    pub fn poll(&self, new_wnd_state: &Elem<model::WndState>) {
        let mut wnd_state = self.wnd_state.borrow_mut();

        if Elem::ptr_eq(&wnd_state, new_wnd_state) {
            return;
        }

        *wnd_state = Elem::clone(new_wnd_state);

        // nothing to do for now
    }
}
