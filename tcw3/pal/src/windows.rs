//! The Windows backend.
use super::iface;
use cggeom::Box2;
use cgmath::{Matrix3, Point2};
use std::{marker::PhantomData, ops::Range, time::Duration};

pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;
pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

#[derive(Debug, Clone, Copy)]
pub struct Wm {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
}

impl iface::Wm for Wm {
    type HWnd = HWnd;
    type HLayer = HLayer;
    type HInvoke = HInvoke;
    type Bitmap = Bitmap;

    unsafe fn global_unchecked() -> Wm {
        Wm {
            _no_send_sync: PhantomData,
        }
    }

    fn is_main_thread() -> bool {
        unimplemented!()
    }

    fn invoke_on_main_thread(f: impl FnOnce(Wm) + Send + 'static) {
        unimplemented!()
    }

    fn invoke(self, f: impl FnOnce(Self) + 'static) {
        unimplemented!()
    }

    fn invoke_after(self, delay: Range<Duration>, f: impl FnOnce(Self) + 'static) -> Self::HInvoke {
        unimplemented!()
    }

    fn cancel_invoke(self, hinv: &Self::HInvoke) {
        unimplemented!()
    }

    fn enter_main_loop(self) -> ! {
        unimplemented!()
    }

    fn terminate(self) {
        unimplemented!()
    }

    fn new_wnd(self, attrs: WndAttrs<'_>) -> Self::HWnd {
        unimplemented!()
    }

    fn set_wnd_attr(self, window: &Self::HWnd, attrs: WndAttrs<'_>) {
        unimplemented!()
    }

    fn remove_wnd(self, window: &Self::HWnd) {
        unimplemented!()
    }

    fn update_wnd(self, window: &Self::HWnd) {
        unimplemented!()
    }

    fn get_wnd_size(self, window: &Self::HWnd) -> [u32; 2] {
        unimplemented!()
    }

    fn get_wnd_dpi_scale(self, window: &Self::HWnd) -> f32 {
        unimplemented!()
    }

    fn request_update_ready_wnd(self, window: &Self::HWnd) {
        unimplemented!()
    }

    fn new_layer(self, attrs: LayerAttrs) -> Self::HLayer {
        unimplemented!()
    }
    fn set_layer_attr(self, layer: &Self::HLayer, attrs: LayerAttrs) {
        unimplemented!()
    }
    fn remove_layer(self, layer: &Self::HLayer) {
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HWnd;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HLayer;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HInvoke;

#[derive(Debug, Clone)]
pub struct Bitmap;

impl iface::Bitmap for Bitmap {
    fn size(&self) -> [u32; 2] {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct BitmapBuilder;

impl iface::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        unimplemented!()
    }
}

impl iface::BitmapBuilder for BitmapBuilder {
    type Bitmap = Bitmap;

    fn into_bitmap(self) -> Self::Bitmap {
        unimplemented!()
    }
}

impl iface::Canvas for BitmapBuilder {
    fn save(&mut self) {
        unimplemented!()
    }
    fn restore(&mut self) {
        unimplemented!()
    }
    fn begin_path(&mut self) {
        unimplemented!()
    }
    fn close_path(&mut self) {
        unimplemented!()
    }
    fn move_to(&mut self, p: Point2<f32>) {
        unimplemented!()
    }
    fn line_to(&mut self, p: Point2<f32>) {
        unimplemented!()
    }
    fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>) {
        unimplemented!()
    }
    fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>) {
        unimplemented!()
    }
    fn fill(&mut self) {
        unimplemented!()
    }
    fn stroke(&mut self) {
        unimplemented!()
    }
    fn clip(&mut self) {
        unimplemented!()
    }
    fn set_fill_rgb(&mut self, rgb: iface::RGBAF32) {
        unimplemented!()
    }
    fn set_stroke_rgb(&mut self, rgb: iface::RGBAF32) {
        unimplemented!()
    }
    fn set_line_cap(&mut self, cap: iface::LineCap) {
        unimplemented!()
    }
    fn set_line_join(&mut self, join: iface::LineJoin) {
        unimplemented!()
    }
    fn set_line_dash(&mut self, phase: f32, lengths: &[f32]) {
        unimplemented!()
    }
    fn set_line_width(&mut self, width: f32) {
        unimplemented!()
    }
    fn set_line_miter_limit(&mut self, miter_limit: f32) {
        unimplemented!()
    }
    fn mult_transform(&mut self, m: Matrix3<f32>) {
        unimplemented!()
    }
}

impl iface::CanvasText<TextLayout> for BitmapBuilder {
    fn draw_text(&mut self, layout: &TextLayout, origin: Point2<f32>, color: iface::RGBAF32) {
        unimplemented!()
    }
}

#[derive(Debug, Clone)]
pub struct CharStyle;

impl iface::CharStyle for CharStyle {
    fn new(attrs: CharStyleAttrs) -> Self {
        unimplemented!()
    }

    fn size(&self) -> f32 {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct TextLayout;

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        unimplemented!()
    }

    fn visual_bounds(&self) -> Box2<f32> {
        unimplemented!()
    }

    fn layout_bounds(&self) -> Box2<f32> {
        unimplemented!()
    }
}
