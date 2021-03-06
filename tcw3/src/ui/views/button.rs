use cgmath::Point2;
use std::{cell::RefCell, fmt, rc::Rc};
use subscriber_list::SubscriberList;

use crate::{
    pal,
    prelude::*,
    ui::{
        layouts::FillLayout,
        mixins::ButtonMixin,
        theming::{roles, ClassSet, HElem, Manager, StyledBox, Widget},
        views::Label,
    },
    uicore::{HView, HViewRef, KeyEvent, Sub, ViewFlags, ViewListener},
};

/// A push button widget.
#[derive(Debug)]
pub struct Button {
    view: HView,
    inner: Rc<Inner>,
}

struct Inner {
    button_mixin: ButtonMixin,
    styled_box: StyledBox,
    label: Label,
    activate_handlers: RefCell<SubscriberList<Box<dyn Fn(pal::Wm)>>>,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Inner")
            .field("button_mixin", &self.button_mixin)
            .field("styled_box", &self.styled_box)
            .field("label", &self.label)
            .field("activate_handlers", &())
            .finish()
    }
}

impl Button {
    pub fn new(style_manager: &'static Manager) -> Self {
        let label = Label::new(style_manager);

        let styled_box = StyledBox::new(
            style_manager,
            ViewFlags::ACCEPT_MOUSE_OVER | ViewFlags::TAB_STOP,
        );
        styled_box.set_child(roles::GENERIC, Some(&label));
        styled_box.set_class_set(ClassSet::BUTTON);
        styled_box.set_auto_class_set(ClassSet::HOVER | ClassSet::FOCUS);

        let view = HView::new(ViewFlags::ACCEPT_MOUSE_DRAG);

        view.set_layout(FillLayout::new(styled_box.view()));

        let inner = Rc::new(Inner {
            button_mixin: ButtonMixin::new(),
            styled_box,
            label,
            activate_handlers: RefCell::new(SubscriberList::new()),
        });

        view.set_listener(ButtonViewListener {
            inner: Rc::clone(&inner),
        });

        Self { view, inner }
    }

    /// Get an owned handle to the view representing a push button widget.
    pub fn view(&self) -> HView {
        self.view.clone()
    }

    /// Borrow the handle to the view representing a push button widget.
    pub fn view_ref(&self) -> HViewRef<'_> {
        self.view.as_ref()
    }

    /// Get the styling element representing the widget.
    pub fn style_elem(&self) -> HElem {
        self.inner.styled_box.style_elem()
    }

    /// Set the text displayed in a push button widget.
    pub fn set_caption(&self, value: impl Into<String>) {
        self.inner.label.set_text(value);
    }

    /// Set the class set of the inner `StyledBox`.
    ///
    /// It defaults to `ClassSet::BUTTON`. Some bits (e.g., `ACTIVE`) are
    /// internally enforced and cannot be modified.
    pub fn set_class_set(&self, mut class_set: ClassSet) {
        let styled_box = &self.inner.styled_box;

        // Protected bits
        let protected = ClassSet::ACTIVE | ClassSet::HOVER;
        class_set -= protected;
        class_set |= styled_box.class_set() & protected;
        styled_box.set_class_set(class_set);
    }

    /// Get the class set of the inner `StyledBox`.
    pub fn class_set(&self) -> ClassSet {
        self.inner.styled_box.class_set()
    }

    /// Add a function called when a push button widget is activated.
    ///
    /// The function is called via `Wm::invoke`, thus allowed to modify
    /// view hierarchy and view attributes. However, it's not allowed to call
    /// `subscribe_activate` when one of the handlers is being called.
    pub fn subscribe_activated(&self, cb: Box<dyn Fn(pal::Wm)>) -> Sub {
        self.inner
            .activate_handlers
            .borrow_mut()
            .insert(cb)
            .untype()
    }
}

impl Widget for Button {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view_ref()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

struct ButtonViewListener {
    inner: Rc<Inner>,
}

impl ButtonViewListener {
    fn build_button_mixin_listener(&self) -> Box<dyn crate::ui::mixins::button::ButtonListener> {
        Box::new(ButtonMixinListener {
            inner: Rc::clone(&self.inner),
        })
    }
}

impl ViewListener for ButtonViewListener {
    fn focus_leave(&self, wm: pal::Wm, view: HViewRef<'_>) {
        self.inner
            .button_mixin
            .focus_leave(wm, view, self.build_button_mixin_listener())
    }

    fn mouse_drag(
        &self,
        _: pal::Wm,
        _: HViewRef<'_>,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn crate::uicore::MouseDragListener> {
        self.inner
            .button_mixin
            .mouse_drag(self.build_button_mixin_listener())
    }

    fn key_down(&self, wm: pal::Wm, view: HViewRef<'_>, e: &KeyEvent<'_>) -> bool {
        self.inner
            .button_mixin
            .key_down(wm, view, e, self.build_button_mixin_listener())
    }

    fn key_up(&self, wm: pal::Wm, view: HViewRef<'_>, e: &KeyEvent<'_>) -> bool {
        self.inner
            .button_mixin
            .key_up(wm, view, e, self.build_button_mixin_listener())
    }
}

struct ButtonMixinListener {
    inner: Rc<Inner>,
}

impl crate::ui::mixins::button::ButtonListener for ButtonMixinListener {
    fn update(&self, _: pal::Wm, _: HViewRef<'_>) {
        let styled_box = &self.inner.styled_box;

        let mut class_set = styled_box.class_set();
        class_set.set(ClassSet::ACTIVE, self.inner.button_mixin.is_pressed());
        styled_box.set_class_set(class_set);
    }

    fn activate(&self, wm: pal::Wm, _: HViewRef<'_>) {
        let inner = Rc::clone(&self.inner);
        wm.invoke(move |wm| {
            let handlers = inner.activate_handlers.borrow();
            for handler in handlers.iter() {
                handler(wm);
            }
        });
    }
}
