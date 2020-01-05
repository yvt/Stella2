use arrayvec::ArrayVec;
use cggeom::Box2;
use cgmath::{Matrix3, Point2};
use std::{convert::TryInto, fmt, mem::MaybeUninit, ptr::null_mut, sync::Arc};
use winapi::{
    shared::minwindef::INT,
    um::{
        gdipluscolor, gdiplusenums,
        gdiplusenums::GraphicsState,
        gdiplusflat as gp,
        gdiplusgpstubs::{GpBitmap, GpGraphics, GpPath, GpPen, GpSolidFill, GpStatus},
        gdiplusinit, gdipluspixelformats,
        gdipluspixelformats::ARGB,
        gdiplustypes,
        gdiplustypes::REAL,
        winnt::CHAR,
    },
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
            assert_gp_ok(gp::GdipGetImageWidth(gp_bmp as _, &mut out[0]));
            assert_gp_ok(gp::GdipGetImageHeight(gp_bmp as _, &mut out[1]));
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
                gp::GdipCreateBitmapFromScan0(
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
            assert_gp_ok(gp::GdipDisposeImage(self.gp_bmp as _));
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
            assert_gp_ok(gp::GdipDeleteGraphics(self.gp_gr));
        }
    }
}

/// An owned pointer of `GpPath`.
#[derive(Debug)]
struct UniqueGpPath {
    gp_path: *mut GpPath,
}

impl Drop for UniqueGpPath {
    fn drop(&mut self) {
        unsafe {
            assert_gp_ok(gp::GdipDeletePath(self.gp_path));
        }
    }
}

/// An owned pointer of `GpSolidFill`.
#[derive(Debug)]
struct UniqueGpSolidFill {
    gp_solid_fill: *mut GpSolidFill,
}

impl Drop for UniqueGpSolidFill {
    fn drop(&mut self) {
        unsafe {
            assert_gp_ok(gp::GdipDeleteBrush(self.gp_solid_fill as _));
        }
    }
}

/// An owned pointer of `GpPen`.
#[derive(Debug)]
struct UniqueGpPen {
    gp_pen: *mut GpPen,
}

impl Drop for UniqueGpPen {
    fn drop(&mut self) {
        unsafe {
            assert_gp_ok(gp::GdipDeletePen(self.gp_pen));
        }
    }
}

fn rgbaf32_to_argb(c: iface::RGBAF32) -> ARGB {
    use alt_fp::FloatOrd;
    let cvt = |x: f32| (x.fmin(1.0).fmax(0.0) * 255.0) as u8;

    let c = c.map_rgb(cvt).map_alpha(cvt);
    gdipluscolor::Color::MakeARGB(c.a, c.r, c.g, c.b)
}

/// Implements `crate::iface::BitmapBuilder`.
#[derive(Debug)]
pub struct BitmapBuilder {
    bmp: BitmapInner,
    gr: UniqueGpGraphics,
    path: UniqueGpPath,
    brush: UniqueGpSolidFill,
    pen: UniqueGpPen,
    state_stack: ArrayVec<[GraphicsState; 16]>,
    cur_pt: [REAL; 2],
}

impl iface::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        ensure_gdip_inited();

        let bmp = BitmapInner::new(size);

        let gr = UniqueGpGraphics {
            gp_gr: unsafe {
                create_gp_obj_with(|out| gp::GdipGetImageGraphicsContext(bmp.gp_bmp as _, out))
            },
        };

        let path = UniqueGpPath {
            gp_path: unsafe {
                create_gp_obj_with(|out| gp::GdipCreatePath(gdiplusenums::FillModeWinding, out))
            },
        };

        let brush = UniqueGpSolidFill {
            gp_solid_fill: unsafe {
                create_gp_obj_with(|out| gp::GdipCreateSolidFill(0xffffffff, out))
            },
        };

        let pen = UniqueGpPen {
            gp_pen: unsafe {
                create_gp_obj_with(|out| {
                    gp::GdipCreatePen1(0xff000000, 1.0, gdiplusenums::UnitPixel, out)
                })
            },
        };

        Self {
            bmp,
            gr,
            path,
            brush,
            pen,
            state_stack: ArrayVec::new(),
            cur_pt: [0.0; 2],
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
        let st = unsafe { create_gp_obj_with(|out| gp::GdipSaveGraphics(self.gr.gp_gr, out)) };
        self.state_stack.push(st);
    }
    fn restore(&mut self) {
        let st = self.state_stack.pop().unwrap();
        unsafe {
            assert_gp_ok(gp::GdipRestoreGraphics(self.gr.gp_gr, st));
        }
    }
    fn begin_path(&mut self) {
        unsafe {
            assert_gp_ok(gp::GdipResetPath(self.path.gp_path));
            assert_gp_ok(gp::GdipSetPathFillMode(
                self.path.gp_path,
                gdiplusenums::FillModeWinding,
            ));
        }
    }
    fn close_path(&mut self) {
        unsafe {
            assert_gp_ok(gp::GdipClosePathFigure(self.path.gp_path));
        }
    }
    fn move_to(&mut self, p: Point2<f32>) {
        unsafe {
            assert_gp_ok(gp::GdipStartPathFigure(self.path.gp_path));
        }
        self.cur_pt = p.into();
    }
    fn line_to(&mut self, p: Point2<f32>) {
        unsafe {
            assert_gp_ok(gp::GdipAddPathLine(
                self.path.gp_path,
                self.cur_pt[0],
                self.cur_pt[1],
                p.x,
                p.y,
            ));
        }
        self.cur_pt = p.into();
    }
    fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>) {
        unsafe {
            assert_gp_ok(gp::GdipAddPathBezier(
                self.path.gp_path,
                self.cur_pt[0],
                self.cur_pt[1],
                cp1.x,
                cp1.y,
                cp2.x,
                cp2.y,
                p.x,
                p.y,
            ));
        }
        self.cur_pt = p.into();
    }
    fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>) {
        let p1: Point2<f32> = self.cur_pt.into();

        let cp1 = cp + (p1 - cp) * (1.0 / 3.0);
        let cp2 = cp + (p - cp) * (1.0 / 3.0);

        self.cubic_bezier_to(cp1, cp2, p);
    }
    fn fill(&mut self) {
        unsafe {
            assert_gp_ok(gp::GdipFillPath(
                self.gr.gp_gr,
                self.brush.gp_solid_fill as _,
                self.path.gp_path,
            ));
        }
    }
    fn stroke(&mut self) {
        unsafe {
            assert_gp_ok(gp::GdipDrawPath(
                self.gr.gp_gr,
                self.pen.gp_pen,
                self.path.gp_path,
            ));
        }
    }
    fn clip(&mut self) {
        unsafe {
            assert_gp_ok(gp::GdipSetClipPath(
                self.gr.gp_gr,
                self.path.gp_path,
                gdiplusenums::CombineModeIntersect,
            ));
        }
    }
    fn set_fill_rgb(&mut self, rgb: iface::RGBAF32) {
        unsafe {
            assert_gp_ok(gp::GdipSetSolidFillColor(
                self.brush.gp_solid_fill,
                rgbaf32_to_argb(rgb),
            ));
        }
    }
    fn set_stroke_rgb(&mut self, rgb: iface::RGBAF32) {
        unsafe {
            assert_gp_ok(gp::GdipSetPenColor(self.pen.gp_pen, rgbaf32_to_argb(rgb)));
        }
    }
    fn set_line_cap(&mut self, cap: iface::LineCap) {
        let cap = match cap {
            iface::LineCap::Butt => gdiplusenums::LineCapFlat,
            iface::LineCap::Round => gdiplusenums::LineCapRound,
            iface::LineCap::Square => gdiplusenums::LineCapSquare,
        };

        unsafe {
            assert_gp_ok(gp::GdipSetPenEndCap(self.pen.gp_pen, cap));
        }
    }
    fn set_line_join(&mut self, join: iface::LineJoin) {
        let join = match join {
            iface::LineJoin::Miter => gdiplusenums::LineJoinMiter,
            iface::LineJoin::Bevel => gdiplusenums::LineJoinBevel,
            iface::LineJoin::Round => gdiplusenums::LineJoinRound,
        };

        unsafe {
            assert_gp_ok(gp::GdipSetPenLineJoin(self.pen.gp_pen, join));
        }
    }
    fn set_line_dash(&mut self, phase: f32, lengths: &[f32]) {
        unsafe {
            if lengths.len() == 0 {
                assert_gp_ok(gp::GdipSetPenDashStyle(
                    self.pen.gp_pen,
                    gdiplusenums::DashStyleSolid,
                ));
            } else {
                assert_gp_ok(gp::GdipSetPenDashArray(
                    self.pen.gp_pen,
                    lengths.as_ptr(),
                    lengths.len() as INT,
                ));
            }
            assert_gp_ok(gp::GdipSetPenDashOffset(self.pen.gp_pen, phase));
        }
    }
    fn set_line_width(&mut self, width: f32) {
        unsafe {
            assert_gp_ok(gp::GdipSetPenWidth(self.pen.gp_pen, width));
        }
    }
    fn set_line_miter_limit(&mut self, miter_limit: f32) {
        unsafe {
            assert_gp_ok(gp::GdipSetPenMiterLimit(self.pen.gp_pen, miter_limit));
        }
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
