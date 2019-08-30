use cairo::{Context, ImageSurface};
use cgmath::{Matrix3, Point2};
use std::{cell::UnsafeCell, sync::Arc};

use super::super::{iface, swrast};
use super::TextLayout;

#[derive(Debug, Clone)]
pub struct Bitmap {
    inner: Arc<BitmapInner>,
}

#[derive(Debug)]
struct BitmapInner {
    data: Box<[u8]>,
    size: [u32; 2],
    stride: usize,
}

impl iface::Bitmap for Bitmap {
    fn size(&self) -> [u32; 2] {
        self.inner.size
    }
}

impl swrast::Bmp for Bitmap {
    fn data(&self) -> &[u8] {
        &self.inner.data
    }

    fn size(&self) -> [usize; 2] {
        let size = self.inner.size;
        // The convertibility already has been validated by `BitmapBuilder::new`
        [size[0] as usize, size[1] as usize]
    }

    fn stride(&self) -> usize {
        self.inner.stride
    }
}

#[derive(Debug)]
pub struct BitmapBuilder {
    cairo_surface: ImageSurface,
    cairo_ctx: Context,
    data: Arc<UnsafeCell<Box<[u8]>>>,
    size: [u32; 2],
    stride: usize,

    /// A singly-linked list representing the state stack.
    state_top: Box<StateStackEntry>,
}

#[derive(Debug)]
struct StateStackEntry {
    state: State,
    next: Option<Box<StateStackEntry>>,
}

#[derive(Debug, Clone, Copy)]
struct State {
    fill_col: [f64; 4],
    stroke_col: [f64; 4],
}

impl iface::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        use std::convert::TryInto;
        let size_sz: [usize; 2] = [
            size[0].try_into().expect("too large"),
            size[1].try_into().expect("too large"),
        ];
        let stride = size_sz[0].checked_mul(4).expect("too large");
        let num_bytes = stride.checked_mul(size_sz[1]).expect("too large");

        let size_i32: [i32; 2] = [
            size[0].try_into().expect("too large"),
            size[1].try_into().expect("too large"),
        ];
        let stride_i32: i32 = stride.try_into().expect("too large");

        let data = vec![0u8; num_bytes].into_boxed_slice();
        let data = Arc::new(UnsafeCell::new(data));

        // `cairo::ImageSurface` uses this as the backing store.
        struct CairoOwnedBmpData(Arc<UnsafeCell<Box<[u8]>>>);

        impl AsRef<[u8]> for CairoOwnedBmpData {
            fn as_ref(&self) -> &[u8] {
                unsafe { &*self.0.get() }
            }
        }

        impl AsMut<[u8]> for CairoOwnedBmpData {
            fn as_mut(&mut self) -> &mut [u8] {
                unsafe { &mut *self.0.get() }
            }
        }

        let cairo_surface = ImageSurface::create_for_data(
            CairoOwnedBmpData(Arc::clone(&data)),
            cairo::Format::ARgb32,
            size_i32[0],
            size_i32[1],
            stride_i32,
        )
        .expect("failed to create a Cairo surface");

        let cairo_ctx = Context::new(&cairo_surface);
        cairo_ctx.set_line_width(1.0);

        BitmapBuilder {
            cairo_surface,
            cairo_ctx,
            data,
            size,
            stride,

            state_top: Box::new(StateStackEntry {
                state: State {
                    fill_col: [1.0; 4],
                    stroke_col: [1.0; 4],
                },
                next: None,
            }),
        }
    }
}

impl iface::BitmapBuilder for BitmapBuilder {
    type Bitmap = Bitmap;

    fn into_bitmap(self) -> Self::Bitmap {
        drop(self.cairo_ctx);
        drop(self.cairo_surface);

        // Now that `ImageSurface` is dropped, we can get the backing store
        let data = Arc::try_unwrap(self.data).unwrap().into_inner();

        Bitmap {
            inner: Arc::new(BitmapInner {
                data,
                size: self.size,
                stride: self.stride,
            }),
        }
    }
}

impl iface::Canvas for BitmapBuilder {
    fn save(&mut self) {
        self.cairo_ctx.save();

        let new_top = Box::new(StateStackEntry {
            state: self.state_top.state.clone(),
            next: None,
        });
        let next = std::mem::replace(&mut self.state_top, new_top);
        self.state_top.next = Some(next);
    }
    fn restore(&mut self) {
        let next = self.state_top.next.take().expect("stack is emtpy");
        self.state_top = next;

        self.cairo_ctx.restore();
    }
    fn begin_path(&mut self) {
        self.cairo_ctx.new_path();
    }
    fn close_path(&mut self) {
        self.cairo_ctx.close_path();
    }
    fn move_to(&mut self, p: Point2<f32>) {
        self.cairo_ctx.move_to(p.x as f64, p.y as f64);
    }
    fn line_to(&mut self, p: Point2<f32>) {
        self.cairo_ctx.line_to(p.x as f64, p.y as f64);
    }
    fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>) {
        self.cairo_ctx.curve_to(
            cp1.x as f64,
            cp1.y as f64,
            cp2.x as f64,
            cp2.y as f64,
            p.x as f64,
            p.y as f64,
        );
    }
    fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>) {
        let (st_x, st_y) = self.cairo_ctx.get_current_point();
        let p1 = Point2::new(st_x, st_y);
        let cp = cp.cast::<f64>().unwrap();
        let p2 = p.cast::<f64>().unwrap();

        let cp1 = cp + (p1 - cp) * (1.0 / 3.0);
        let cp2 = cp + (p2 - cp) * (1.0 / 3.0);

        self.cairo_ctx
            .curve_to(cp1.x, cp1.y, cp2.x, cp2.y, p2.x, p2.y);
    }
    fn fill(&mut self) {
        let col = self.state_top.state.fill_col;
        self.cairo_ctx
            .set_source_rgba(col[0], col[1], col[2], col[3]);

        self.cairo_ctx.fill_preserve();
    }
    fn stroke(&mut self) {
        let col = self.state_top.state.stroke_col;
        self.cairo_ctx
            .set_source_rgba(col[0], col[1], col[2], col[3]);

        self.cairo_ctx.stroke_preserve();
    }
    fn clip(&mut self) {
        self.cairo_ctx.clip_preserve();
    }
    fn set_fill_rgb(&mut self, rgb: iface::RGBAF32) {
        self.state_top.state.fill_col = [rgb.r as f64, rgb.g as f64, rgb.b as f64, rgb.a as f64];
    }
    fn set_stroke_rgb(&mut self, rgb: iface::RGBAF32) {
        self.state_top.state.stroke_col = [rgb.r as f64, rgb.g as f64, rgb.b as f64, rgb.a as f64];
    }
    fn set_line_cap(&mut self, cap: iface::LineCap) {
        use cairo::LineCap;
        self.cairo_ctx.set_line_cap(match cap {
            iface::LineCap::Butt => LineCap::Butt,
            iface::LineCap::Round => LineCap::Round,
            iface::LineCap::Square => LineCap::Square,
        });
    }
    fn set_line_join(&mut self, join: iface::LineJoin) {
        use cairo::LineJoin;
        self.cairo_ctx.set_line_join(match join {
            iface::LineJoin::Miter => LineJoin::Miter,
            iface::LineJoin::Round => LineJoin::Round,
            iface::LineJoin::Bevel => LineJoin::Bevel,
        });
    }
    fn set_line_dash(&mut self, phase: f32, lengths: &[f32]) {
        let dashes: Vec<_> = lengths.iter().map(|&x| x as f64).collect();
        self.cairo_ctx.set_dash(&dashes, phase as f64);
    }
    fn set_line_width(&mut self, width: f32) {
        self.cairo_ctx.set_line_width(width as f64);
    }
    fn set_line_miter_limit(&mut self, miter_limit: f32) {
        self.cairo_ctx.set_miter_limit(miter_limit as f64);
    }
    fn mult_transform(&mut self, m: Matrix3<f32>) {
        let m = (m / m.z.z).cast::<f64>().unwrap();
        self.cairo_ctx
            .transform(cairo::Matrix::new(m.x.x, m.x.y, m.y.x, m.y.y, m.z.x, m.z.y));
    }
}

impl iface::CanvasText<TextLayout> for BitmapBuilder {
    fn draw_text(&mut self, _layout: &TextLayout, _origin: Point2<f32>, _color: iface::RGBAF32) {
        unimplemented!()
    }
}
