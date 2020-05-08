use super::manager::HElem;
use crate::uicore::{HView, HViewRef};

pub trait Widget {
    fn view_ref(&self) -> HViewRef<'_>;
    fn style_elem(&self) -> Option<HElem>;
}

impl Widget for (HView, Option<HElem>) {
    fn view_ref(&self) -> HViewRef<'_> {
        self.0.as_ref()
    }
    fn style_elem(&self) -> Option<HElem> {
        self.1
    }
}

impl Widget for (HViewRef<'_>, Option<HElem>) {
    fn view_ref(&self) -> HViewRef<'_> {
        self.0
    }
    fn style_elem(&self) -> Option<HElem> {
        self.1
    }
}
