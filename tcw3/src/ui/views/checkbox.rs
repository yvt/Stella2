use crate::{
    pal,
    ui::{
        theming::{ClassSet, HElem, Manager, Widget},
        views::Button,
    },
    uicore::{HView, HViewRef, Sub},
};

/// A checkbox widget (with a label).
#[derive(Debug)]
pub struct Checkbox {
    button: Button,
}

/// A radio button widget (with a label).
#[derive(Debug)]
pub struct RadioButton {
    button: Button,
}

impl Checkbox {
    pub fn new(style_manager: &'static Manager) -> Self {
        let button = Button::new(style_manager);

        button.set_class_set(ClassSet::CHECKBOX);

        Self { button }
    }
}

impl RadioButton {
    pub fn new(style_manager: &'static Manager) -> Self {
        let button = Button::new(style_manager);

        button.set_class_set(ClassSet::RADIO_BUTTON);

        Self { button }
    }
}

macro_rules! imp {
    ($t:ty) => {
        impl $t {
            /// Get an owned handle to the view representing the widget.
            pub fn view(&self) -> HView {
                self.button.view()
            }

            /// Borrow the handle to the view representing the widget.
            pub fn view_ref(&self) -> HViewRef<'_> {
                self.button.view_ref()
            }

            /// Get the styling element representing the widget.
            pub fn style_elem(&self) -> HElem {
                self.button.style_elem()
            }

            /// Set the text displayed in the widget.
            pub fn set_caption(&self, value: impl Into<String>) {
                self.button.set_caption(value);
            }

            /// Set the class set of the inner `StyledBox`.
            ///
            /// Some bits (e.g., `ACTIVE` and `CHECKED`) are internally enforced
            /// and cannot be modified.
            pub fn set_class_set(&self, mut class_set: ClassSet) {
                // Protected bits
                let protected = ClassSet::CHECKED;
                class_set -= protected;
                class_set |= self.button.class_set() & protected;

                self.button.set_class_set(class_set);
            }

            /// Get the class set of the inner `StyledBox`.
            pub fn class_set(&self) -> ClassSet {
                self.button.class_set()
            }

            /// Add a function called when the widget is activated.
            ///
            /// The function is called via `Wm::invoke`, thus allowed to modify
            /// view hierarchy and view attributes. However, it's not allowed to call
            /// `subscribe_activated` when one of the handlers is being called.
            pub fn subscribe_activated(&self, cb: Box<dyn Fn(pal::Wm)>) -> Sub {
                self.button.subscribe_activated(cb)
            }

            /// Check or uncheck the checkbox.
            pub fn set_checked(&self, value: bool) {
                let mut class_set = self.button.class_set();
                class_set.set(ClassSet::CHECKED, value);
                self.button.set_class_set(class_set);
            }

            /// Get a flag indicating whether the checkbox is checked.
            pub fn checked(&self) -> bool {
                self.button.class_set().contains(ClassSet::CHECKED)
            }
        }

        impl Widget for $t {
            fn view_ref(&self) -> HViewRef<'_> {
                self.view_ref()
            }

            fn style_elem(&self) -> Option<HElem> {
                Some(self.style_elem())
            }
        }
    };
}

imp!(Checkbox);
imp!(RadioButton);
