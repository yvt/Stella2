use super::manager::HElem;
use crate::uicore::HViewRef;

pub trait Widget {
    fn view_ref(&self) -> HViewRef<'_>;
    fn style_elem(&self) -> Option<HElem>;
}

impl Widget for (HViewRef<'_>, Option<HElem>) {
    fn view_ref(&self) -> HViewRef<'_> {
        self.0
    }
    fn style_elem(&self) -> Option<HElem> {
        self.1
    }
}
