use std::{cell::RefCell, rc::Rc};

use super::Inner;
use crate::{
    pal,
    pal::prelude::*,
    uicore::{HView, HWnd, UpdateCtx, ViewListener},
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
    fn mount(&self, wm: pal::WM, _: &HView, _: &HWnd) {
        let layer = wm.new_layer(pal::LayerAttrs {
            flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
            ..Default::default()
        });

        let old_layer = self.layer.replace(Some(layer));

        assert!(old_layer.is_none());
    }

    fn unmount(&self, wm: pal::WM, _: &HView) {
        if let Some(layer) = self.layer.replace(None) {
            wm.remove_layer(&layer);
        }
    }

    fn position(&self, _: pal::WM, view: &HView) {
        view.pend_update();
    }

    fn update(&self, wm: pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        let layer = self.layer.borrow();
        let layer = layer.as_ref().unwrap();

        if let Some(sublayers) = ctx.sublayers().take() {
            wm.set_layer_attr(
                &layer,
                pal::LayerAttrs {
                    bounds: Some(view.global_frame()),
                    sublayers: Some(sublayers),
                    ..Default::default()
                },
            );
        }

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![(*layer).clone()]);
        }
    }
}
