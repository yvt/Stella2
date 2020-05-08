use tcw3::{ui::theming, uicore::HViewRef};

stella2_meta::designer_impl! {
    crate::view::tabbar::TabbarView
}

impl TabbarView {
    /// Handle `init` event.
    fn init(&self) {}
}

impl theming::Widget for TabbarView {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view().as_ref()
    }

    fn style_elem(&self) -> Option<theming::HElem> {
        Some(self.style_elem())
    }
}
