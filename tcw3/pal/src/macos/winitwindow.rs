use cocoa::{
    base::{id, nil},
    quartzcore::transaction,
};
use objc::{msg_send, sel, sel_impl};
use winit::{platform::macos::WindowExtMacOS, window::Window};

use super::super::winit::{HWndCore, WinitWm, WinitWmCore, WndContent as WndContentTrait};
use super::{
    utils::{with_autorelease_pool, IdRef},
    HLayer, Wm, WndAttrs,
};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct HWnd {
    winit_hwnd: HWndCore,
}

pub(super) struct WndContent {
    layer: Option<HLayer>,
    view: IdRef,
}

impl WndContent {
    fn new(winit_wnd: &Window) -> Self {
        extern "C" {
            /// Return `[TCWWinitView class]`.
            fn tcw_winit_view_cls() -> id;
        }

        let view: id = unsafe { msg_send![tcw_winit_view_cls(), alloc] };
        let view = IdRef::new(unsafe { msg_send![view, init] })
            .non_nil()
            .unwrap();

        let root_view = winit_wnd.ns_view() as id;
        let () = unsafe { msg_send![root_view, addSubview:*view] };

        // Configure the subview's lqyout
        let () = unsafe { msg_send![*view, setupLayout] };

        Self { layer: None, view }
    }
}

impl WndContentTrait for WndContent {
    type Wm = Wm;
    type HLayer = HLayer;

    fn set_layer(
        &mut self,
        wm_core: &WinitWmCore<Self::Wm, Self>,
        _: &Window,
        layer: Option<Self::HLayer>,
    ) {
        let ca_layer = if let Some(hlayer) = &layer {
            hlayer.ca_layer(wm_core.wm())
        } else {
            nil
        };

        let () = unsafe { msg_send![*self.view, setContentLayer: ca_layer] };

        self.layer = layer;
    }

    fn update(&mut self, wm_core: &WinitWmCore<Self::Wm, Self>, _: &Window) -> bool {
        if let Some(layer) = &self.layer {
            with_autorelease_pool(|| {
                transaction::begin();
                transaction::set_animation_duration(0.0);
                layer.flush(wm_core.wm());
                transaction::commit();
            });
        }

        // This backend does not rely on `RedrawRequested`
        false
    }
}

impl WinitWm for Wm {
    fn hwnd_core_to_hwnd(self, hwnd: &HWndCore) -> Self::HWnd {
        HWnd {
            winit_hwnd: hwnd.clone(),
        }
    }
}

impl HWnd {
    pub(super) fn new(wm: Wm, attrs: WndAttrs<'_>) -> Self {
        let winit_hwnd = wm.winit_wm().new_wnd(attrs, |winit_wnd, layer| {
            let mut content = WndContent::new(winit_wnd);
            content.set_layer(wm.winit_wm(), winit_wnd, layer);
            content
        });

        Self { winit_hwnd }
    }

    pub(super) fn set_attrs(&self, wm: Wm, attrs: WndAttrs<'_>) {
        wm.winit_wm().set_wnd_attr(&self.winit_hwnd, attrs);
    }

    pub(super) fn remove(&self, wm: Wm) {
        wm.winit_wm().remove_wnd(&self.winit_hwnd);
    }

    pub(super) fn update(&self, wm: Wm) {
        wm.winit_wm().update_wnd(&self.winit_hwnd)
    }

    pub(super) fn request_update_ready(&self, wm: Wm) {
        wm.winit_wm().request_update_ready_wnd(&self.winit_hwnd);
    }

    pub(super) fn get_size(&self, wm: Wm) -> [u32; 2] {
        wm.winit_wm().get_wnd_size(&self.winit_hwnd)
    }

    pub(super) fn get_dpi_scale(&self, wm: Wm) -> f32 {
        wm.winit_wm().get_wnd_dpi_scale(&self.winit_hwnd)
    }
}
