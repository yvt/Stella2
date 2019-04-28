use bitflags::bitflags;
use cggeom::Box2;
use cgmath::Matrix4;
use rgb::RGBA;
use std::rc::Rc;

use super::traits::{WndListener, WM};

pub type RGBAF32 = RGBA<f32>;

#[derive(Clone)]
pub struct WndAttrs<T: WM, TCaption, TLayer> {
    pub size: Option<[u32; 2]>,
    pub caption: Option<TCaption>,
    pub visible: Option<bool>,
    pub listener: Option<Option<Rc<dyn WndListener<T>>>>,
    pub layer: Option<Option<TLayer>>,
}

impl<T: WM, TCaption, TLayer> Default for WndAttrs<T, TCaption, TLayer> {
    fn default() -> Self {
        Self {
            size: None,
            caption: None,
            visible: None,
            listener: None,
            layer: None,
        }
    }
}

impl<T: WM, TCaption, TLayer> WndAttrs<T, TCaption, TLayer>
where
    TCaption: AsRef<str>,
    TLayer: Clone,
{
    pub fn as_ref(&self) -> WndAttrs<T, &str, TLayer> {
        WndAttrs {
            size: self.size,
            caption: self.caption.as_ref().map(AsRef::as_ref),
            visible: self.visible,
            listener: self.listener.clone(),
            layer: self.layer.clone(),
        }
    }
}

#[derive(Clone)]
pub struct LayerAttrs<TBitmap, TLayer> {
    /// The transformation applied to the contents of the layer.
    /// It doesn't have an effect on sublayers.
    ///
    /// The input coordinate space is based on `bounds`. The output coordinate
    /// space is virtual pixel coordinates with `(0,0)` at the top left corner
    /// of a window's client region.
    pub transform: Option<Matrix4<f32>>,

    /// Specifies the content image of the layer.
    pub contents: Option<Option<TBitmap>>,
    /// Specifies the bounds of the content image.
    pub bounds: Option<Box2<f32>>,
    /// Specifies the flexible region of the content image.
    ///
    /// Defaults to `(0,0)-(1,1)`, indicating entire the image is scaled in
    /// both directions to match the content bounds.
    pub contents_center: Option<Box2<f32>>,
    /// Specifies the natural scaling ratio of the content image.
    ///
    /// This is used only when `contents_center` has a non-default value.
    /// Defaults to `1.0`.
    pub contents_scale: Option<f32>,
    /// Specifies the solid color underlaid to the content image.
    pub bg_color: Option<RGBAF32>,

    pub sublayers: Option<Vec<TLayer>>,

    /// Specifies the opacity value.
    ///
    /// Defaults to `1.0`. Sublayers are affected as well. The opacity value
    /// is applied after the sublayers are composited, thus it has a different
    /// effect than applying the value on the sublayers individually.
    pub opacity: Option<f32>,

    /// Specifies additional options on the layer.
    pub flags: Option<LayerFlags>,
}

impl<TBitmap, TLayer> LayerAttrs<TBitmap, TLayer> {
    /// Replace the fields with values from `o` if they are `Some(_)`.
    pub fn override_with(&mut self, o: Self) {
        macro_rules! process_one {
            ($i:ident) => {
                if let Some(x) = o.$i {
                    self.$i = Some(x);
                }
            };
        }
        process_one!(transform);
        process_one!(contents);
        process_one!(bounds);
        process_one!(contents_center);
        process_one!(contents_scale);
        process_one!(bg_color);
        process_one!(sublayers);
        process_one!(opacity);
        process_one!(flags);
    }
}

impl<TBitmap, TLayer> Default for LayerAttrs<TBitmap, TLayer> {
    fn default() -> Self {
        Self {
            transform: None,
            contents: None,
            bounds: None,
            contents_center: None,
            contents_scale: None,
            sublayers: None,
            bg_color: None,
            opacity: None,
            flags: None,
        }
    }
}

bitflags! {
    pub struct LayerFlags: u32 {
        /// Clip sublayers to the content bounds.
        const MASK_TO_BOUNDS = 1;
    }
}
