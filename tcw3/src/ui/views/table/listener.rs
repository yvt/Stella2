use std::{cell::RefCell, rc::Rc};

use super::Inner;
use crate::{
    pal,
    pal::prelude::*,
    uicore::{HViewRef, HWndRef, UpdateCtx, ViewListener},
};

#[derive(Debug)]
pub(super) struct TableViewListener {
    inner: Rc<Inner>,
    layer: RefCell<Option<pal::HLayer>>,
}

impl TableViewListener {
    pub(super) fn new(inner: Rc<Inner>) -> Self {
        Self {
            inner,
            layer: RefCell::new(None),
        }
    }
}

impl ViewListener for TableViewListener {
    fn mount(&self, wm: pal::Wm, _: HViewRef<'_>, _: HWndRef<'_>) {
        let layer = wm.new_layer(pal::LayerAttrs {
            flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
            ..Default::default()
        });

        let old_layer = self.layer.replace(Some(layer));

        assert!(old_layer.is_none());
    }

    fn unmount(&self, wm: pal::Wm, _: HViewRef<'_>) {
        if let Some(layer) = self.layer.replace(None) {
            wm.remove_layer(&layer);
        }
    }

    fn position(&self, _: pal::Wm, view: HViewRef<'_>) {
        view.pend_update();
    }

    fn update(&self, wm: pal::Wm, view: HViewRef<'_>, ctx: &mut UpdateCtx<'_>) {
        let layer = self.layer.borrow();
        let layer = layer.as_ref().expect("not mounted");

        let mut new_attrs = pal::LayerAttrs {
            bounds: Some(view.global_frame()),
            ..Default::default()
        };

        if let Some(sublayers) = ctx.sublayers().take() {
            new_attrs.sublayers = Some(sublayers);
        }
        wm.set_layer_attr(&layer, new_attrs);

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![(*layer).clone()]);
        }
    }
}
