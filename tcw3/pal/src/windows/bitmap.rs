use arrayvec::ArrayVec;
use cggeom::Box2;
use cgmath::{Matrix3, Point2};
use std::{convert::TryInto, fmt, mem::MaybeUninit, ptr::null_mut, sync::Arc};
use winapi::um::{
    gdiplusenums::GraphicsState,
    gdiplusflat::{
        GdipCreateBitmapFromScan0, GdipDeleteGraphics, GdipDisposeImage,
        GdipGetImageGraphicsContext, GdipGetImageHeight, GdipGetImageWidth, GdipRestoreGraphics,
        GdipSaveGraphics,
    },
    gdiplusgpstubs::{GpBitmap, GpGraphics, GpStatus},
    gdiplusinit, gdipluspixelformats, gdiplustypes,
    winnt::CHAR,
};

use super::CharStyleAttrs;
use crate::iface;

#[cold]
fn panic_by_gp_status(st: GpStatus) -> ! {
    panic!("GDI+ error {:?}", st);
}

/// Panic if `st` is not `Ok`.
fn assert_gp_ok(st: GpStatus) {
    if st != gdiplustypes::Ok {
        panic_by_gp_status(st);
    }
}

unsafe fn create_gp_obj_with<T>(f: impl FnOnce(*mut T) -> GpStatus) -> T {
    let mut out = MaybeUninit::uninit();

    assert_gp_ok(f(out.as_mut_ptr()));

    out.assume_init()
}

/// Call `GdiplusStartup` if it hasn't been called yet.
fn ensure_gdip_inited() {
    lazy_static::lazy_static! {
        static ref GDIP_INIT: () = {
            let input = gdiplusinit::GdiplusStartupInput::new(
                if log::STATIC_MAX_LEVEL == log::LevelFilter::Off {
                    None
                } else {
                    Some(gdip_debug_event_handler)
                },
                0, // do not suppress the GDI+ background thread
                1, // suppress external codecs
            );

            unsafe {
                assert_gp_ok(gdiplusinit::GdiplusStartup(
                    // don't need a token, we won't call `GdiplusShutdown`
                    &mut 0,
                    &input,
                    // output is not necessary because we don't suppress the
                    // GDI+ background thread
                    null_mut(),
                ));
            }
        };
    }

    let () = &*GDIP_INIT;

    extern "system" fn gdip_debug_event_handler(
        level: gdiplusinit::DebugEventLevel,
        message: *mut CHAR,
    ) {
        let level = match level {
            gdiplusinit::DebugEventLevelFatal => log::Level::Error,
            gdiplusinit::DebugEventLevelWarning => log::Level::Warn,
            _ => log::Level::Error,
        };

        log::log!(level, "GDI+ debug event: {:?}", unsafe {
            std::ffi::CStr::from_ptr(message)
        });
    }
}

/// Implements `crate::iface::Bitmap`.
#[derive(Clone)]
pub struct Bitmap {
    inner: Arc<BitmapInner>,
}

impl fmt::Debug for Bitmap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bitmap")
            .field("gp_bmp", &self.inner.gp_bmp)
            .field("size", &iface::Bitmap::size(self))
            .finish()
    }
}

impl iface::Bitmap for Bitmap {
    fn size(&self) -> [u32; 2] {
        let mut out = [0, 0];
        let gp_bmp = self.inner.gp_bmp;
        unsafe {
            assert_gp_ok(GdipGetImageWidth(gp_bmp as _, &mut out[0]));
            assert_gp_ok(GdipGetImageHeight(gp_bmp as _, &mut out[1]));
        }
        [out[0] as u32, out[1] as u32]
    }
}

/// An owned pointer of `GpBitmap`.
#[derive(Debug)]
struct BitmapInner {
    gp_bmp: *mut GpBitmap,
}

// I just assume that GDI+ objects only require object-granular external
// synchronization
unsafe impl Send for BitmapInner {}
unsafe impl Sync for BitmapInner {}

impl BitmapInner {
    fn new(size: [u32; 2]) -> Self {
        let gp_bmp = unsafe {
            create_gp_obj_with(|out| {
                GdipCreateBitmapFromScan0(
                    size[0].try_into().expect("bitmap too large"),
                    size[1].try_into().expect("bitmap too large"),
                    0,
                    gdipluspixelformats::PixelFormat32bppPARGB, // pre-multiplied alpha
                    null_mut(),                                 // let GDI+ manage the memory
                    out,
                )
            })
        };

        Self { gp_bmp }
    }
}

impl Drop for BitmapInner {
    fn drop(&mut self) {
        unsafe {
            assert_gp_ok(GdipDisposeImage(self.gp_bmp as _));
        }
    }
}

/// An owned pointer of `GpGraphics`.
#[derive(Debug)]
struct UniqueGpGraphics {
    gp_gr: *mut GpGraphics,
}

impl Drop for UniqueGpGraphics {
    fn drop(&mut self) {
        unsafe {
            assert_gp_ok(GdipDeleteGraphics(self.gp_gr));
        }
    }
}

/// Implements `crate::iface::BitmapBuilder`.
#[derive(Debug)]
pub struct BitmapBuilder {
    bmp: BitmapInner,
    gr: UniqueGpGraphics,
    state_stack: ArrayVec<[GraphicsState; 16]>,
}

impl iface::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        ensure_gdip_inited();

        let bmp = BitmapInner::new(size);

        let gr = UniqueGpGraphics {
            gp_gr: unsafe {
                create_gp_obj_with(|out| GdipGetImageGraphicsContext(bmp.gp_bmp as _, out))
            },
        };

        Self {
            bmp,
            gr,
            state_stack: ArrayVec::new(),
        }
    }
}

impl iface::BitmapBuilder for BitmapBuilder {
    type Bitmap = Bitmap;

    fn into_bitmap(self) -> Self::Bitmap {
        Bitmap {
            inner: Arc::new(self.bmp),
        }
    }
}

impl iface::Canvas for BitmapBuilder {
    fn save(&mut self) {
        let st = unsafe { create_gp_obj_with(|out| GdipSaveGraphics(self.gr.gp_gr, out)) };
        self.state_stack.push(st);
    }
    fn restore(&mut self) {
        let st = self.state_stack.pop().unwrap();
        unsafe {
            assert_gp_ok(GdipRestoreGraphics(self.gr.gp_gr, st));
        }
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
