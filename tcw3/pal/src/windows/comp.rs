//! Compositor
use cggeom::{box2, prelude::*, Box2};
use cgmath::{prelude::*, Matrix3, Matrix4};
use std::{cell::RefCell, fmt, mem::MaybeUninit, rc::Rc};
use winapi::{
    shared::{ntdef::HRESULT, windef::HWND},
    um::winuser,
};
use winrt::{
    windows::foundation::numerics::{Matrix3x2, Matrix4x4, Vector2, Vector3},
    windows::ui::composition::{
        desktop::IDesktopWindowTarget, CompositionBrush, CompositionClip, CompositionColorBrush,
        CompositionGeometry, CompositionNineGridBrush, CompositionRectangleGeometry,
        CompositionStretch, CompositionSurfaceBrush, Compositor, ContainerVisual,
        ICompositionClip2, ICompositionTarget, ICompositor2, ICompositor5, ICompositor6, Visual,
    },
    ComPtr, RtDefaultConstructible, RtType,
};

use super::{
    drawutils::{
        extend_matrix3_with_identity_z, winrt_color_from_rgbaf32, winrt_m3x2_from_cgmath,
        winrt_m4x4_from_cgmath, winrt_v2_from_cgmath_pt, winrt_v2_from_cgmath_vec,
        winrt_v3_from_cgmath_pt, ExtendExt,
    },
    surface,
    utils::{assert_hresult_ok, assert_win32_ok, ComPtr as MyComPtr},
    winapiext::ICompositorDesktopInterop,
    LayerAttrs, Wm,
};
use crate::{iface::LayerFlags, prelude::MtLazyStatic};

struct CompState {
    comp: ComPtr<Compositor>,
    comp2: ComPtr<ICompositor2>,
    comp5: ComPtr<ICompositor5>,
    comp6: ComPtr<ICompositor6>,
    comp_desktop: MyComPtr<ICompositorDesktopInterop>,
    surface_map: surface::SurfaceMap,
}

impl CompState {
    fn new(_: Wm) -> Self {
        // Create a dispatch queue for the main thread
        unsafe {
            assert_hresult_ok(tcw_comp_init());
        }

        let comp = Compositor::new();

        let comp_desktop: MyComPtr<ICompositorDesktopInterop> =
            MyComPtr::iunknown_from_winrt_comptr(comp.clone())
                .query_interface()
                .unwrap();

        let surface_map = surface::SurfaceMap::new(&comp);

        // We need `ICompositor2` for `CreateLayerVisual` and
        // `CreateNineGridBrush`
        let comp2: ComPtr<ICompositor2> = comp
            .query_interface()
            .expect("Could not obtain ICompositor2");

        // We need `ICompositor5` for `CreateRectangleGeometry`
        let comp5: ComPtr<ICompositor5> = comp
            .query_interface()
            .expect("Could not obtain ICompositor5");

        // We need `ICompositor6` for `CreateGeometricClip`
        let comp6: ComPtr<ICompositor6> = comp
            .query_interface()
            .expect("Could not obtain ICompositor6");

        CompState {
            comp,
            comp2,
            comp5,
            comp6,
            comp_desktop,
            surface_map,
        }
    }
}

mt_lazy_static! {
    static <Wm> ref CS: CompState => CompState::new;
}

// Defined in `comp.cpp`
extern "C" {
    fn tcw_comp_init() -> HRESULT;
}

pub(super) struct CompWnd {
    target: ComPtr<ICompositionTarget>,
    root_vis: ComPtr<Visual>,
    root_cvis: ComPtr<ContainerVisual>,
}

impl fmt::Debug for CompWnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompWnd")
            .field("target", &(&*self.target as *const _))
            .finish()
    }
}

impl CompWnd {
    pub(super) fn new(wm: Wm, hwnd: HWND) -> Self {
        let cs = CS.get_with_wm(wm);

        let desktop_target = unsafe {
            let mut out = MaybeUninit::uninit();
            assert_hresult_ok(
                cs.comp_desktop
                    .CreateDesktopWindowTarget(hwnd, 0, out.as_mut_ptr()),
            );
            IDesktopWindowTarget::wrap(out.assume_init()).unwrap()
        };

        let target: ComPtr<ICompositionTarget> = desktop_target.query_interface().unwrap();

        let root_cvis = cs.comp.create_container_visual().unwrap().unwrap();
        let root_vis: ComPtr<Visual> = root_cvis.query_interface().unwrap();

        target.set_root(&root_vis).unwrap();

        let this = Self {
            target,
            root_vis,
            root_cvis,
        };

        this.handle_dpi_change(hwnd);

        this
    }

    pub(super) fn set_layer(&self, hlayer: Option<HLayer>) {
        let children = self.root_cvis.get_children().unwrap().unwrap();

        children.remove_all().unwrap();

        if let Some(hlayer) = &hlayer {
            children.insert_at_top(&hlayer.layer.container_vis).unwrap();
        }
    }

    pub(super) fn handle_dpi_change(&self, hwnd: HWND) {
        let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
        assert_win32_ok(dpi);

        let scale = dpi as f32 / 96.0;
        self.root_vis
            .set_scale(Vector3 {
                X: scale,
                Y: scale,
                Z: 1.0,
            })
            .unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct HLayer {
    layer: Rc<Layer>,
}

impl PartialEq for HLayer {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.layer, &other.layer)
    }
}

impl Eq for HLayer {}

impl std::hash::Hash for HLayer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (&*self.layer as *const Layer).hash(state);
    }
}

struct Layer {
    // container_vis ~ clip, opacity
    // |
    // +-- layer_cvis (optional)
    //     |
    //     +-- solid.0 (optional) ~ bg_color
    //	   |
    //	   +-- image.0 (optional) ~ contents
    //     |
    //	   +-- (sublayers)
    //
    // - transform is applied to clip, bg_color, and contents
    container_cvis: ComPtr<ContainerVisual>,
    container_vis: ComPtr<Visual>,
    state: RefCell<LayerState>,
}

impl fmt::Debug for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Layer")
            .field(&(&*self.container_cvis as *const _))
            .finish()
    }
}

/// The changing part of `Layer`
struct LayerState {
    layer_cvis: Option<ComPtr<ContainerVisual>>,
    solid: Option<(ComPtr<Visual>, ComPtr<CompositionColorBrush>)>,
    image: Option<(
        ComPtr<Visual>,
        ComPtr<CompositionNineGridBrush>,
        ComPtr<CompositionSurfaceBrush>,
    )>,
    clip: Option<(
        ComPtr<ICompositionClip2>,
        ComPtr<CompositionRectangleGeometry>,
    )>,
    nonopaque: bool,
    sublayers: Vec<HLayer>,
    xform4x4: Matrix4x4,
    xform3x2: Matrix3x2,
    bounds: Box2<f32>,
    /// The pixel size of `LayerAttrs::contents`.
    contents_size: [f32; 2],
    /// `LayerAttrs::contents_center`
    contents_center: Box2<f32>,
    /// `LayerAttrs::contents_scale`
    contents_scale: f32,
}

pub fn new_layer(wm: Wm, attrs: LayerAttrs) -> HLayer {
    let cs = CS.get_with_wm(wm);

    let container_cvis = cs.comp.create_container_visual().unwrap().unwrap();
    let container_vis: ComPtr<Visual> = container_cvis.query_interface().unwrap();

    let layer = Layer {
        container_cvis,
        container_vis,
        state: RefCell::new(LayerState {
            layer_cvis: None,
            solid: None,
            image: None,
            clip: None,
            nonopaque: false,
            sublayers: Vec::new(),
            xform4x4: winrt_m4x4_from_cgmath(Matrix4::identity()),
            xform3x2: winrt_m3x2_from_cgmath(Matrix3::identity()),
            bounds: box2! { min: [0.0; 2], max: [0.0; 2] },
            contents_size: [0.0; 2],
            contents_center: box2! { min: [0.0; 2], max: [1.0; 2] },
            contents_scale: 1.0,
        }),
    };

    let hlayer = HLayer {
        layer: Rc::new(layer),
    };

    set_layer_attr(wm, &hlayer, attrs);

    hlayer
}

pub fn set_layer_attr(wm: Wm, hlayer: &HLayer, attrs: LayerAttrs) {
    let cs = CS.get_with_wm(wm);

    let layer = &*hlayer.layer;

    let mut state = layer.state.borrow_mut();
    let state = &mut *state; // enable split borrow

    if let Some(op) = attrs.opacity {
        if op < 1.0 {
            state.nonopaque = true;
        }
        layer.container_vis.set_opacity(op).unwrap();
    }

    // Insert `layer_cvis`
    if state.layer_cvis.is_none() {
        let needs_layer = state.nonopaque && {
            let has_solid = state.solid.is_some() | attrs.bg_color.is_some();
            let has_image = matches!(attrs.contents, Some(Some(_)));
            let num_sublayers = if let Some(sublayers) = &attrs.sublayers {
                sublayers.len()
            } else {
                state.sublayers.len()
            };

            has_solid as usize + has_image as usize + num_sublayers > 1
        };

        if needs_layer {
            // Construct a `LayerVisual`
            let layer_lvis = cs.comp2.create_layer_visual().unwrap().unwrap();
            let layer_cvis: ComPtr<ContainerVisual> = layer_lvis.query_interface().unwrap();
            let layer_vis: ComPtr<Visual> = layer_lvis.query_interface().unwrap();

            // Move everything from `container_cvis` to `layer_lvis`.
            let container_cvis: &ComPtr<ContainerVisual> = &layer.container_cvis;

            let layer_children = layer_cvis.get_children().unwrap().unwrap();
            let container_children = container_cvis.get_children().unwrap().unwrap();

            container_children.remove_all().unwrap();
            container_children.insert_at_top(&layer_vis).unwrap();

            if let Some((vis, _)) = &state.solid {
                layer_children.insert_at_top(&vis).unwrap();
            }
            if let Some((vis, _, _)) = &state.image {
                layer_children.insert_at_top(&vis).unwrap();
            }
            for sublayer in state.sublayers.iter() {
                layer_children
                    .insert_at_top(&sublayer.layer.container_vis)
                    .unwrap();
            }
            state.layer_cvis = Some(layer_cvis);
        }
    }

    // The existence or lack of `state.layer_cvis` is immutable beyond this
    // point. This means that from this point on, child visuals can be just
    // inserted to or removed from `visuals_container_cvis` defined here.
    let visuals_container_cvis = state
        .layer_cvis
        .as_deref()
        .unwrap_or(&*layer.container_cvis);

    if let Some(mat) = attrs.transform {
        state.xform4x4 = winrt_m4x4_from_cgmath(extend_matrix3_with_identity_z(mat));
        state.xform3x2 = winrt_m3x2_from_cgmath(mat);
        if let Some((clip, _)) = &state.clip {
            clip.set_transform_matrix(state.xform3x2).unwrap();
        }
        if let Some((vis, _)) = &state.solid {
            vis.set_transform_matrix(state.xform4x4).unwrap();
        }
        if let Some((vis, _, _)) = &state.image {
            vis.set_transform_matrix(state.xform4x4).unwrap();
        }
    }

    let bounds_to_anchor = |b: Box2<f32>| Vector2 {
        X: -b.min.x / b.size().x,
        Y: -b.min.y / b.size().y,
    };

    if let Some(bounds) = attrs.bounds {
        state.bounds = bounds;
        if let Some((_, rect)) = &state.clip {
            rect.set_offset(winrt_v2_from_cgmath_pt(state.bounds.min))
                .unwrap();
            rect.set_size(winrt_v2_from_cgmath_vec(state.bounds.size()))
                .unwrap();
        }
        if let Some((vis, _)) = &state.solid {
            vis.set_size(winrt_v2_from_cgmath_vec(state.bounds.size()))
                .unwrap();
            vis.set_anchor_point(bounds_to_anchor(state.bounds))
                .unwrap();
        }
        if let Some((vis, _, _)) = &state.image {
            vis.set_size(winrt_v2_from_cgmath_vec(state.bounds.size()))
                .unwrap();
            vis.set_anchor_point(bounds_to_anchor(state.bounds))
                .unwrap();
        }
    }

    // The parameters for 9-grid scaling are dependent on various inputs
    let update_slicing =
        attrs.contents.is_some() | attrs.contents_center.is_some() | attrs.contents_scale.is_some();

    if let Some(contents) = attrs.contents {
        let (_, _, sbrush) = if let Some(x) = &state.image {
            x
        } else {
            // Create `state.image` and set properties
            let sbrush = cs.comp.create_surface_brush().unwrap().unwrap();
            sbrush.set_stretch(CompositionStretch::Fill).unwrap();

            let nbrush = cs.comp2.create_nine_grid_brush().unwrap().unwrap();
            nbrush
                .set_source(&sbrush.query_interface::<CompositionBrush>().unwrap())
                .unwrap();

            let svis = cs.comp.create_sprite_visual().unwrap().unwrap();
            let vis: ComPtr<Visual> = svis.query_interface().unwrap();

            svis.set_brush(&nbrush.query_interface::<CompositionBrush>().unwrap())
                .unwrap();

            vis.set_transform_matrix(state.xform4x4).unwrap();
            vis.set_size(winrt_v2_from_cgmath_vec(state.bounds.size()))
                .unwrap();
            vis.set_anchor_point(bounds_to_anchor(state.bounds))
                .unwrap();

            // Insert the newly created visual to the correct position
            let children = visuals_container_cvis.get_children().unwrap().unwrap();
            if let Some((solid_vis, _)) = &state.solid {
                children.insert_above(&vis, solid_vis).unwrap();
            } else {
                children.insert_at_bottom(&vis).unwrap();
            }

            state.image = Some((vis, nbrush, sbrush));
            state.image.as_ref().unwrap()
        };

        if let Some(bitmap) = &contents {
            let surface = cs.surface_map.get_surface_for_bitmap(wm, bitmap);
            sbrush.set_surface(&surface).unwrap();

            use crate::iface::Bitmap;
            use array::Array2;
            state.contents_size = bitmap.size().map(|i| i as f32);
        } else {
            // TODO: Clear the contents
        }
    }

    if let Some(center) = attrs.contents_center {
        state.contents_center = center;
    }

    if let Some(scale) = attrs.contents_scale {
        state.contents_scale = scale;
    }

    if let (Some((_, nbrush, _)), true) = (&state.image, update_slicing) {
        // Update the 9-grid slicing parameters if any of relevant
        // inputs have changed
        let scale = 1.0 / state.contents_scale;
        nbrush.set_top_inset_scale(scale).unwrap();
        nbrush.set_right_inset_scale(scale).unwrap();
        nbrush.set_bottom_inset_scale(scale).unwrap();
        nbrush.set_left_inset_scale(scale).unwrap();

        let center = state.contents_center;
        let csize = state.contents_size;
        let margins = [
            center.min.y * csize[1],
            (1.0 - center.max.x) * csize[0],
            (1.0 - center.max.y) * csize[1],
            center.min.x * csize[0],
        ];
        nbrush.set_top_inset(margins[0]).unwrap();
        nbrush.set_right_inset(margins[1]).unwrap();
        nbrush.set_bottom_inset(margins[2]).unwrap();
        nbrush.set_left_inset(margins[3]).unwrap();
    }

    if let Some(color) = attrs.bg_color {
        let (_, brush) = if let Some(x) = &state.solid {
            x
        } else {
            // Create `state.solid` and set properties
            let brush = cs.comp.create_color_brush().unwrap().unwrap();

            let svis = cs.comp.create_sprite_visual().unwrap().unwrap();
            let vis: ComPtr<Visual> = svis.query_interface().unwrap();

            svis.set_brush(&brush.query_interface::<CompositionBrush>().unwrap())
                .unwrap();

            vis.set_transform_matrix(state.xform4x4).unwrap();
            vis.set_size(winrt_v2_from_cgmath_vec(state.bounds.size()))
                .unwrap();
            vis.set_anchor_point(bounds_to_anchor(state.bounds))
                .unwrap();

            // Insert the newly created visual to the correct position
            let children = visuals_container_cvis.get_children().unwrap().unwrap();
            children.insert_at_bottom(&vis).unwrap();

            state.solid = Some((vis, brush));
            state.solid.as_ref().unwrap()
        };

        brush.set_color(winrt_color_from_rgbaf32(color)).unwrap();
    }

    if let Some(sublayers) = attrs.sublayers {
        state.sublayers = sublayers;

        // TODO: calculate the difference and only reconcile the changed portions
        let children = visuals_container_cvis.get_children().unwrap().unwrap();
        children.remove_all().unwrap();

        if let Some((vis, _)) = &state.solid {
            children.insert_at_top(&vis).unwrap();
        }
        if let Some((vis, _, _)) = &state.image {
            children.insert_at_top(&vis).unwrap();
        }
        for sublayer in state.sublayers.iter() {
            children
                .insert_at_top(&sublayer.layer.container_vis)
                .unwrap();
        }
    }

    if let Some(flags) = attrs.flags {
        if flags.contains(LayerFlags::MASK_TO_BOUNDS) {
            let (clip, _) = if let Some(x) = &state.clip {
                x
            } else {
                // Create `state.clip` and set properties
                let rect = cs.comp5.create_rectangle_geometry().unwrap().unwrap();

                rect.set_offset(winrt_v2_from_cgmath_pt(state.bounds.min))
                    .unwrap();
                rect.set_size(winrt_v2_from_cgmath_vec(state.bounds.size()))
                    .unwrap();

                let gclip = cs.comp6.create_geometric_clip().unwrap().unwrap();
                gclip
                    .set_geometry(&rect.query_interface::<CompositionGeometry>().unwrap())
                    .unwrap();

                let clip: ComPtr<ICompositionClip2> = gclip.query_interface().unwrap();
                clip.set_transform_matrix(state.xform3x2).unwrap();

                state.clip = Some((clip, rect));
                state.clip.as_ref().unwrap()
            };

            layer
                .container_vis
                .set_clip(&clip.query_interface::<CompositionClip>().unwrap())
                .unwrap();
        } else {
            // TODO: layer.container_vis.set_clip(None);
        }
    }
}

pub fn remove_layer(_: Wm, _: &HLayer) {
    // `Layer` is ref-counted, there's nothing to do here
}
