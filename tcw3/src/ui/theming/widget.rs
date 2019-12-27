use super::manager::HElem;
use crate::uicore::HView;

pub trait Widget {
    fn view(&self) -> &HView;
    fn style_elem(&self) -> Option<HElem>;
}
