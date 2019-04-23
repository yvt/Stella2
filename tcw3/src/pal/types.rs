#[derive(Debug, Clone)]
pub struct WndAttrs<TCaption> {
    pub size: Option<[u32; 2]>,
    pub caption: Option<TCaption>,
    pub visible: Option<bool>,
}

impl<TCaption> Default for WndAttrs<TCaption> {
    fn default() -> Self {
        Self {
            size: None,
            caption: None,
            visible: None,
        }
    }
}

impl<TCaption> WndAttrs<TCaption>
where
    TCaption: AsRef<str>,
{
    pub fn as_ref(&self) -> WndAttrs<&str> {
        WndAttrs {
            size: self.size,
            caption: self.caption.as_ref().map(AsRef::as_ref),
            visible: self.visible,
        }
    }
}
