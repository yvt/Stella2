use super::types::{LayerAttrs, WndAttrs, RGBAF32, LineCap, LineJoin};
use cggeom::Box2;
use cgmath::{Matrix3, Point2};

pub trait WM: Sized {
    /// A window handle type.
    type HWnd: Send + Sync + Clone;

    /// A layer handle type.
    type HLayer: Send + Sync + Clone;

    /// A bitmap type.
    type Bitmap: Bitmap;

    fn enter_main_loop(&self);
    fn terminate(&self);

    fn new_wnd(&self, attrs: &WndAttrs<Self, &str, Self::HLayer>) -> Self::HWnd;

    /// Set the attributes of a window.
    ///
    /// Panics if the window has already been closed.
    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &WndAttrs<Self, &str, Self::HLayer>);
    fn remove_wnd(&self, window: &Self::HWnd);
    /// Update a window's contents.
    ///
    /// Calling this method requests that the contents of a window is updated
    /// based on the attributes of the window and its all sublayers as soon as
    /// possible. Conversely, all attribute updates may be deferred until this
    /// method is called.
    fn update_wnd(&self, window: &Self::HWnd);

    fn new_layer(&self, attrs: &LayerAttrs<Self::Bitmap, Self::HLayer>) -> Self::HLayer;

    // FIXME: Maybe pass `LayerAttrs` by value to elide the costly copy?
    /// Set the attributes of a layer.
    ///
    /// The behavior is unspecified if the layer has already been removed.
    fn set_layer_attr(&self, layer: &Self::HLayer, attrs: &LayerAttrs<Self::Bitmap, Self::HLayer>);
    fn remove_layer(&self, layer: &Self::HLayer);
}

/// Window event handlers.
///
/// The receiver is immutable because event handlers may manipulate windows,
/// which in turn might cause other event handlers to be called.
pub trait WndListener<T: WM> {
    /// The user has attempted to close a window. Returns `true` if the window
    /// can be closed.
    fn close_requested(&self, _: &T, _: &T::HWnd) -> bool {
        true
    }

    /// A window has been closed.
    fn close(&self, _: &T, _: &T::HWnd) {}

    // TODO: more events
}

/// A immutable, ref-counted bitmap image.
pub trait Bitmap: Clone + Sized + Send + Sync {}

/// Types supporting drawing operations.
pub trait Canvas {
    /// Push a copy of the current graphics state onto the state stack.
    fn save(&mut self);
    /// Pop a graphics state from the state stack.
    fn restore(&mut self);

    /// Start a new empty path.
    fn begin_path(&mut self);
    /// Close the current figure of the current path.
    fn close_path(&mut self);

    /// Begin a new subpath at the specified point.
    fn move_to(&mut self, p: Point2<f32>);
    /// Append a straight line to the specified point.
    fn line_to(&mut self, p: Point2<f32>);
    /// Append a cubic Bézier curve to the specified point, using the provided
    /// control points.
    fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>);
    /// Append a quadratic Bézier curve to the specified point, using the
    /// provided control point.
    fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>);

    /// Fill the area within the current path, using the non-zero winding number
    /// rule.
    fn fill(&mut self);
    /// Draw a line along the current path.
    fn stroke(&mut self);
    /// Set the current clipping region to its intersection with the area within
    /// current path.
    fn clip(&mut self);

    /// Stroke the specified rectangle.
    ///
    /// The implementation of this method may invalidate the current path.
    fn stroke_rect(&mut self, bx: Box2<f32>) {
        self.begin_path();
        self.move_to(Point2::new(bx.min.x, bx.min.y));
        self.line_to(Point2::new(bx.max.x, bx.min.y));
        self.line_to(Point2::new(bx.max.x, bx.max.y));
        self.line_to(Point2::new(bx.min.x, bx.max.y));
        self.close_path();
        self.stroke();
    }
    /// Fill the specified rectangle.
    ///
    /// The implementation of this method may invalidate the current path.
    fn fill_rect(&mut self, bx: Box2<f32>) {
        self.begin_path();
        self.move_to(Point2::new(bx.min.x, bx.min.y));
        self.line_to(Point2::new(bx.max.x, bx.min.y));
        self.line_to(Point2::new(bx.max.x, bx.max.y));
        self.line_to(Point2::new(bx.min.x, bx.max.y));
        self.close_path();
        self.fill();
    }
    /// Set the current clipping region to its intersection with the specified
    /// rectangle.
    ///
    /// The implementation of this method may invalidate the current path.
    fn clip_rect(&mut self, bx: Box2<f32>) {
        self.begin_path();
        self.move_to(Point2::new(bx.min.x, bx.min.y));
        self.line_to(Point2::new(bx.max.x, bx.min.y));
        self.line_to(Point2::new(bx.max.x, bx.max.y));
        self.line_to(Point2::new(bx.min.x, bx.max.y));
        self.close_path();
        self.clip();
    }

    /// Set the current fill brush to a solid color.
    fn set_fill_rgb(&mut self, rgb: RGBAF32);
    // TODO: generic brush

    /// Set the current stroke brush to a solid color.
    fn set_stroke_rgb(&mut self, rgb: RGBAF32);
    // TODO: generic brush

    fn set_line_cap(&mut self, cap: LineCap);
    fn set_line_join(&mut self, join: LineJoin);
    fn set_line_dash(&mut self, phase: f32, lengths: &[f32]);
    fn set_line_width(&mut self, width: f32);
    fn set_line_miter_limit(&mut self, miter_limit: f32);

    /// Transform the local coordinate system.
    ///
    /// `m.x.z` and `m.y.z` is assumed to be zero. This means projective
    /// transformations are not supported and only affine transformations can
    /// be expressed. `m.z.z` must be positive.
    fn mult_transform(&mut self, m: Matrix3<f32>);

    // TODO: text rendering

    // TODO: image rendering
}

/// A builder type for [`Bitmap`] supporting 2D drawing operations via
/// [`Canvas`].
pub trait BitmapBuilder: Canvas {
    type Bitmap: Bitmap;

    fn into_bitmap(self) -> Self::Bitmap;
}

pub trait BitmapBuilderNew: BitmapBuilder + Sized {
    /// Create a [`BitmapBuilder`] with a R8G8B8A8 backing bitmap.
    fn new(size: [u32; 2]) -> Self;
}
