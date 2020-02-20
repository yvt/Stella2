use super::manager::HElem;
use crate::uicore::HViewRef;

pub trait Widget {
    fn view_ref(&self) -> HViewRef<'_>;
    fn style_elem(&self) -> Option<HElem>;
}
