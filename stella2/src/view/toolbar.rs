use crate::{model, stylesheet::elem_id};
use tcw3::ui::{theming, AlignFlags};

stella2_meta::designer_impl! {
    crate::view::toolbar::ToolbarView
}

impl ToolbarView {
    /// Handle `init` event.
    fn init(&self) {
        // TODO: there is no way to get a weak reference at the moment
        //  self.toggle_sidebar_button()
        //      .set_on_activate(enclose!((this_weak) move |_| {
        //          if let Some(this) = this_weak.upgrade() {
        //              // Toggle the sidebar
        //              let visible = this.wnd_state.borrow().sidebar_visible;
        //              this.dispatch.borrow()(model::WndAction::ToggleSidebar(!visible));
        //          }
        //      }));
    }
}
