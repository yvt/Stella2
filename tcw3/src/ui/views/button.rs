use cgmath::Point2;
use std::{cell::RefCell, fmt, rc::Rc};

use crate::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::FillLayout,
        mixins::ButtonMixin,
        theming::{ClassSet, ElemClassPath, Manager, Role, StyledBox},
        views::Label,
    },
    uicore::{HView, ViewFlags, ViewListener},
};

/// A push button widget.
#[derive(Debug)]
pub struct Button {
    view: HView,
    inner: Rc<Inner>,
}

struct Inner {
    button_mixin: ButtonMixin,
    styled_box: RefCell<StyledBox>,
    label: RefCell<Label>,
    activate_handler: RefCell<Box<dyn Fn(pal::WM)>>,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Inner")
            .field("button_mixin", &self.button_mixin)
            .field("styled_box", &self.styled_box)
            .field("label", &self.label)
            .field("activate_handler", &())
            .finish()
    }
}

impl Button {
    pub fn new(style_manager: &'static Manager) -> Self {
        let label = Label::new(style_manager);

        let mut styled_box = StyledBox::new(style_manager, ViewFlags::default());
        styled_box.set_subview(Role::Generic, Some(label.view().clone()));
        styled_box.set_class_set(ClassSet::BUTTON);
        styled_box.reapply_style();

        let view = HView::new(ViewFlags::default() | ViewFlags::ACCEPT_MOUSE_DRAG);

        view.set_layout(FillLayout::new(styled_box.view().clone()));

        let inner = Rc::new(Inner {
            button_mixin: ButtonMixin::new(),
            styled_box: RefCell::new(styled_box),
            label: RefCell::new(label),
            activate_handler: RefCell::new(Box::new(|_| {})),
        });

        view.set_listener(ButtonViewListener {
            inner: Rc::clone(&inner),
        });

        Self { view, inner }
    }

    /// Get the view representing a push button widget.
    pub fn view(&self) -> &HView {
        &self.view
    }

    /// Set the text displayed in a push button widget.
    pub fn set_caption(&mut self, value: impl Into<String>) {
        self.inner.label.borrow_mut().set_text(value);
    }

    /// Set the parent class path.
    pub fn set_parent_class_path(&mut self, parent_class_path: Option<Rc<ElemClassPath>>) {
        let mut styled_box = self.inner.styled_box.borrow_mut();
        styled_box.set_parent_class_path(parent_class_path);
        styled_box.reapply_style();

        self.inner
            .label
            .borrow_mut()
            .set_parent_class_path(Some(styled_box.class_path().clone()));
    }

    /// Set the function called when a push button widget is activated.
    ///
    /// The function is called via `WM::invoke`, thus allowed to modify
    /// view hierarchy and view attributes. However, it's not allowed to call
    /// `set_on_activate` on the activated `Button`.
    pub fn set_on_activate(&mut self, cb: impl Fn(pal::WM) + 'static) {
        self.inner.activate_handler.replace(Box::new(cb));
    }
}

struct ButtonViewListener {
    inner: Rc<Inner>,
}

impl ViewListener for ButtonViewListener {
    fn mouse_drag(
        &self,
        _: pal::WM,
        _: &HView,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn crate::uicore::MouseDragListener> {
        self.inner
            .button_mixin
            .mouse_drag(Box::new(ButtonMixinListener {
                inner: Rc::clone(&self.inner),
            }))
    }
}

struct ButtonMixinListener {
    inner: Rc<Inner>,
}

impl crate::ui::mixins::button::ButtonListener for ButtonMixinListener {
    fn update(&self, _: pal::WM, _: &HView) {
        let mut styled_box = self.inner.styled_box.borrow_mut();
        styled_box.set_class_set(if self.inner.button_mixin.is_pressed() {
            ClassSet::BUTTON | ClassSet::ACTIVE
        } else {
            ClassSet::BUTTON
        });
        styled_box.reapply_style();

        self.inner
            .label
            .borrow_mut()
            .set_parent_class_path(Some(styled_box.class_path().clone()));
    }

    fn activate(&self, wm: pal::WM, _: &HView) {
        let inner = Rc::clone(&self.inner);
        wm.invoke(move |wm| {
            let handler = inner.activate_handler.borrow();
            handler(wm);
        });
    }
}
