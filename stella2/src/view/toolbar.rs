use enclose::enclose;
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
    toggle_sidebar_button: Button,
    go_back_button: Button,
    go_forward_button: Button,
}

impl ToolbarView {
    pub fn new(
        wnd_state: Elem<model::WndState>,
        style_manager: &'static theming::Manager,
    ) -> Rc<Self> {
        let container = HView::new(ViewFlags::default());

        let toggle_sidebar_button = Button::new(style_manager);
        toggle_sidebar_button.set_class_set(
            theming::ClassSet::BUTTON
                | [elem_id::SIDEBAR_SHOW, elem_id::SIDEBAR_HIDE]
                    [wnd_state.sidebar_visible as usize],
        );
        // TODO: Display a dropdown list when the sidebar is hidden

        // TODO: Use toolbar button style
        let go_back_button = Button::new(style_manager);
        go_back_button.set_class_set(theming::ClassSet::BUTTON | elem_id::GO_BACK);

        let go_forward_button = Button::new(style_manager);
        go_forward_button.set_class_set(theming::ClassSet::BUTTON | elem_id::GO_FORWARD);

        // TODO: Search bar
        let search_bar = Label::new(style_manager).with_text("todo: search");

        const MARGIN: f32 = 5.0;

        let main_layout = TableLayout::stack_horz(vec![
            (toggle_sidebar_button.view().clone(), AlignFlags::JUSTIFY),
            (
                Spacer::new().with_fixed([MARGIN * 2.0, 0.0]).into_view(),
                AlignFlags::HORZ_JUSTIFY,
            ),
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
            toggle_sidebar_button,
            go_back_button,
            go_forward_button,
        });

        // Register event handlers
        let this_weak = Rc::downgrade(&this);

        this.toggle_sidebar_button
            .set_on_activate(enclose!((this_weak) move |_| {
                if let Some(this) = this_weak.upgrade() {
                    // Toggle the sidebar
                    let visible = this.wnd_state.borrow().sidebar_visible;
                    this.dispatch.borrow()(model::WndAction::ToggleSidebar(!visible));
                }
            }));
        this.go_back_button.set_on_activate(|_| {
            dbg!();
        });
        this.go_forward_button.set_on_activate(|_| {
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

        // Update the appearance of the "toggle sidebar" button
        {
            let button = &self.toggle_sidebar_button;
            let class_set = button.class_set();
            let new_class_set = (class_set - theming::ClassSet::ID_MASK)
                | [elem_id::SIDEBAR_SHOW, elem_id::SIDEBAR_HIDE]
                    [wnd_state.sidebar_visible as usize];
            if class_set != new_class_set {
                button.set_class_set(new_class_set);
            }
        }
    }
}
