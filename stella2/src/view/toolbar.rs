use tcw3::{ui::theming, uicore::HViewRef};

use crate::model;

stella2_meta::designer_impl! {
    crate::view::toolbar::ToolbarView
}

impl ToolbarView {
    /// Handle `toggle_sidebar_button.activate` event.
    fn toggle_sidebar(&self) {
        // Toggle the sidebar
        self.raise_dispatch(model::AppAction::Wnd(model::WndAction::ToggleSidebar));
    }

    /// Handle `menu_button.activate` event.
    fn toggle_pref(&self) {
        // Show or hide the Preferences window
        // TODO: Show a dropdown menu
        self.raise_dispatch(model::AppAction::TogglePref);
    }
}

impl theming::Widget for ToolbarView {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view().as_ref()
    }

    fn style_elem(&self) -> Option<theming::HElem> {
        Some(self.style_elem())
    }
}
