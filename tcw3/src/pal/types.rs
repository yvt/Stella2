use super::traits::WndListener;
use std::rc::Rc;

#[derive(Clone)]
pub struct WndAttrs<HWnd, TCaption> {
    pub size: Option<[u32; 2]>,
    pub caption: Option<TCaption>,
    pub visible: Option<bool>,
    pub listener: Option<Rc<dyn WndListener<HWnd>>>,
}

impl<HWnd, TCaption> Default for WndAttrs<HWnd, TCaption> {
    fn default() -> Self {
        Self {
            size: None,
            caption: None,
            visible: None,
            listener: None,
        }
    }
}

impl<HWnd, TCaption> WndAttrs<HWnd, TCaption>
where
    TCaption: AsRef<str>,
{
    pub fn as_ref(&self) -> WndAttrs<HWnd, &str> {
        WndAttrs {
            size: self.size,
            caption: self.caption.as_ref().map(AsRef::as_ref),
            visible: self.visible,
            listener: self.listener.clone(),
        }
    }
}
