use alt_fp::FloatOrd;
use cggeom::box2;
use cgmath::Vector2;
use flags_macro::flags;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use super::{
    manager::{Elem, Manager, PropKindFlags},
    style::{ClassSet, ElemClassPath, Metrics, Prop, PropValue, Role},
};
use crate::{
    pal,
    pal::prelude::*,
    uicore::{HView, HWnd, Layout, LayoutCtx, SizeTraits, Sub, UpdateCtx, ViewFlags, ViewListener},
};

/// A box styled based on styling properties.
///
/// The following [`Prop`]s are handled: `NumLayers`, `LayerImg`,
/// `LayerBgColor`, `LayerMetrics`, `LayerOpacity`, `LayerCenter`, `LayerXform`,
/// `SubviewMetrics`, `ClipMetrics`, and `MinSize`.
///
/// [`Prop`]: crate::ui::theming::Prop
#[derive(Debug)]
pub struct StyledBox {
    view: HView,
    shared: Rc<RefCell<Shared>>,
    sheet_set_change_sub: Option<Sub>,
}

#[derive(Debug)]
struct Shared {
    view: HView,

    style_manager: &'static Manager,

    class_path: Rc<ElemClassPath>,
    dirty_class_path: bool,
    style_elem: Elem,
    dirty_kind: PropKindFlags,

    subviews: Vec<(Role, HView)>,

    has_layer_group: bool,
}

impl StyledBox {
    pub fn new(style_manager: &'static Manager, view_flags: ViewFlags) -> Self {
        let class_path = Rc::new(ElemClassPath::default());

        // Create `Elem` based on the inital properties
        let mut style_elem = Elem::new();
        style_elem.set_class_path(&style_manager.sheet_set(), &class_path);

        // Create the initial `Layout` based on the inital properties
        let subviews = Vec::new();
        let layout = SbLayout::new(style_manager, &subviews, &style_elem);

        // Create and set up a `View`
        let view = HView::new(view_flags);

        let shared = Rc::new(RefCell::new(Shared {
            view: view.clone(),
            class_path,
            dirty_class_path: false,
            subviews,
            style_manager,
            style_elem,
            // Already have an up-to-date `Layout`, so exclude it from
            // the dirty flags
            dirty_kind: PropKindFlags::all() - PropKindFlags::LAYOUT,
            has_layer_group: view_flags.contains(ViewFlags::LAYER_GROUP),
        }));

        view.set_listener(SbListener::new(Rc::downgrade(&shared)));
        view.set_layout(layout);

        // Get notified when the sheet set changes
        let sheet_set_change_sub = {
            let shared = Rc::downgrade(&shared);
            style_manager.subscribe_sheet_set_changed(Box::new(move |_, _| {
                if let Some(shared) = shared.upgrade() {
                    shared.borrow_mut().reapply_style(true);
                }
            }))
        };

        Self {
            view,
            shared,
            sheet_set_change_sub: Some(sheet_set_change_sub),
        }
    }

    /// Set the class set of the styled element.
    ///
    /// Update is deferred until `reapply_style` is called.
    pub fn set_class_set(&mut self, class_set: ClassSet) {
        let mut shared = self.shared.borrow_mut();

        let mut class_path = Rc::make_mut(&mut shared.class_path);
        class_path.class_set = class_set;
        drop(class_path);

        // Pend the recalculation of the active rule set
        shared.dirty_class_path = true;
    }

    /// Set the parent class path.
    ///
    /// Update is deferred until `reapply_style` is called.
    pub fn set_parent_class_path(&mut self, parent_class_path: Option<Rc<ElemClassPath>>) {
        let mut shared = self.shared.borrow_mut();

        let mut class_path = Rc::make_mut(&mut shared.class_path);
        class_path.tail = parent_class_path;
        drop(class_path);

        // Pend the recalculation of the active rule set
        shared.dirty_class_path = true;
    }

    /// Set a subview for the specified `Role`.
    ///
    /// Update is deferred until `reapply_style` is called.
    pub fn set_subview(&mut self, role: Role, view: Option<HView>) {
        let mut shared = self.shared.borrow_mut();
        let subviews = &mut shared.subviews;

        if let Some(view) = view {
            // Assign a subview
            if let Some(ent) = subviews.iter_mut().find(|(r, _)| *r == role) {
                ent.1 = view;
            } else {
                subviews.push((role, view));
            }
        } else {
            // Remove a subview
            if let Some(i) = subviews.iter().position(|(r, _)| *r == role) {
                subviews.remove(i);
            }
        }

        // Pend layout update
        shared.dirty_kind |= PropKindFlags::LAYOUT;
    }

    /// Apply pending changes and recalculate the styling properties.
    pub fn reapply_style(&mut self) {
        self.shared.borrow_mut().reapply_style(false);
    }

    /// Get `Rc<ElemClassPath>` representing the class path of the styled
    /// element. The returned value can be set on subviews as a parent class
    /// path.
    pub fn class_path(&self) -> Rc<ElemClassPath> {
        Rc::clone(&self.shared.borrow().class_path)
    }

    /// Get the view representing a styled box.
    pub fn view(&self) -> &HView {
        &self.view
    }
}

impl Drop for StyledBox {
    fn drop(&mut self) {
        self.sheet_set_change_sub
            .take()
            .unwrap()
            .unsubscribe()
            .unwrap();
    }
}

impl Shared {
    /// Recalculate the styling properties.
    ///
    /// This is defined on `Shared` because it may be called when the active
    /// stylesheet set is changed.
    pub fn reapply_style(&mut self, sheet_set_changed: bool) {
        let style_elem = &mut self.style_elem;

        let sheet_set = self.style_manager.sheet_set();

        // TODO: `Label` has a similar internal function... Hopefully they could be merged

        // Recalculate the active rule set
        if sheet_set_changed {
            // The stylesheet set has changed, so do a full update
            style_elem.set_class_path(&sheet_set, &self.class_path);
            self.dirty_kind = PropKindFlags::all();
        } else if self.dirty_class_path {
            // The class path has changed but the stylesheet set didn't change.
            let kind_flags = style_elem.set_and_diff_class_path(&sheet_set, &self.class_path);
            self.dirty_kind |= kind_flags;
        }

        self.dirty_class_path = false;

        if self.dirty_kind.intersects(PropKindFlags::LAYOUT) {
            self.view.set_layout(SbLayout::new(
                self.style_manager,
                &self.subviews,
                &style_elem,
            ));
        }

        if self
            .dirty_kind
            .intersects(flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}])
        {
            self.view.pend_update();
        }

        self.dirty_kind -= PropKindFlags::LAYOUT;
    }
}

struct SbLayout {
    subview_layout: Vec<Metrics>,
    subviews: Vec<HView>,
    min_size: Vector2<f32>,
}

impl SbLayout {
    fn new(style_manager: &'static Manager, subviews: &Vec<(Role, HView)>, elem: &Elem) -> Self {
        // Evaluate the layout properties now
        let sheet_set = style_manager.sheet_set();
        Self {
            subview_layout: subviews
                .iter()
                .map(
                    |&(role, _)| match elem.compute_prop(&sheet_set, Prop::SubviewMetrics(role)) {
                        PropValue::Metrics(m) => m,
                        _ => unreachable!(),
                    },
                )
                .collect(),
            subviews: subviews.iter().map(|x| x.1.clone()).collect(),
            min_size: match elem.compute_prop(&sheet_set, Prop::MinSize) {
                PropValue::Vector2(v) => v,
                _ => unreachable!(),
            },
        }
    }
}

impl Layout for SbLayout {
    fn subviews(&self) -> &[HView] {
        &self.subviews
    }

    fn size_traits(&self, ctx: &LayoutCtx<'_>) -> SizeTraits {
        let mut traits = SizeTraits {
            min: self.min_size,
            ..SizeTraits::default()
        };

        let mut num_pref_x = 0;
        let mut num_pref_y = 0;

        for (metrics, sv) in self.subview_layout.iter().zip(self.subviews.iter()) {
            let margin = &metrics.margin;
            let sv_traits = ctx.subview_size_traits(sv);

            let margin_x = margin[1] + margin[3];
            let margin_y = margin[0] + margin[2];

            // For each axis, if two margins are fixed, the subview's `SizeTraits`
            // affects that of the superview
            if margin_x.is_finite() {
                traits.min.x = traits.min.x.fmax(sv_traits.min.x + margin_x);
                traits.max.x = traits.max.x.fmin(sv_traits.max.x + margin_x);
                traits.preferred.x += sv_traits.preferred.x + margin_x;
                num_pref_x += 1;
            }

            if margin_y.is_finite() {
                traits.min.y = traits.min.y.fmax(sv_traits.min.y + margin_y);
                traits.max.y = traits.max.y.fmin(sv_traits.max.y + margin_y);
                traits.preferred.y += sv_traits.preferred.y + margin_y;
                num_pref_y += 1;
            }
        }

        use std::cmp::max;

        traits.preferred.x /= max(num_pref_x, 1) as f32;
        traits.preferred.y /= max(num_pref_y, 1) as f32;

        traits
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        for (metrics, sv) in self.subview_layout.iter().zip(self.subviews.iter()) {
            let sv_traits = ctx.subview_size_traits(sv);
            let container = box2! {top_left: [0.0, 0.0], size: size};

            let frame = metrics.arrange(container, sv_traits.preferred);

            ctx.set_subview_frame(sv, frame);
        }
    }

    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        use as_any::Downcast;
        if let Some(other) = (*other).downcast_ref::<Self>() {
            self.subviews == other.subviews
        } else {
            false
        }
    }
}

struct SbListener {
    // Use a weak reference to break a cycle
    shared: Weak<RefCell<Shared>>,
    layers: RefCell<Option<Layers>>,
}

#[derive(Default)]
struct Layers {
    clip: Option<pal::HLayer>,
    styled: Vec<pal::HLayer>,
    sub: Option<Sub>,
}

impl SbListener {
    fn new(shared: Weak<RefCell<Shared>>) -> Self {
        Self {
            shared,
            layers: RefCell::new(None),
        }
    }
}

impl ViewListener for SbListener {
    fn mount(&self, wm: pal::WM, _: &HView, wnd: &HWnd) {
        let mut layers = self.layers.borrow_mut();
        assert!(layers.is_none());

        if let Some(shared) = self.shared.upgrade() {
            let mut shared = shared.borrow_mut();

            shared.dirty_kind |= flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}];

            // Watch for DPI scale changes
            let sub = {
                let shared = self.shared.clone();
                wnd.subscribe_dpi_scale_changed(Box::new(move |_, _| {
                    if let Some(shared) = shared.upgrade() {
                        let mut shared = shared.borrow_mut();
                        shared.dirty_kind |= PropKindFlags::LAYER_IMG;

                        shared.view.pend_update();
                    }
                }))
            };

            // Create layers. Properties are set later in `update`.
            *layers = Some(Layers {
                clip: if shared.has_layer_group {
                    Some(wm.new_layer(pal::LayerAttrs {
                        flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
                        ..pal::LayerAttrs::default()
                    }))
                } else {
                    None
                },
                styled: Vec::new(),
                sub: Some(sub),
            });
        } else {
            *layers = Some(Layers::default());
        }
    }

    fn unmount(&self, wm: pal::WM, _: &HView) {
        let layers = self.layers.borrow_mut().take().unwrap();

        for layer in layers.clip {
            wm.remove_layer(&layer);
        }
        for layer in layers.styled {
            wm.remove_layer(&layer);
        }

        if let Some(sub) = layers.sub {
            sub.unsubscribe().unwrap();
        }
    }

    fn position(&self, _: pal::WM, view: &HView) {
        if let Some(shared) = self.shared.upgrade() {
            let mut shared = shared.borrow_mut();
            shared.dirty_kind |= PropKindFlags::LAYER_BOUNDS;

            view.pend_update();
        }
    }

    fn update(&self, wm: pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        let shared;
        if let Some(shared_rc) = self.shared.upgrade() {
            shared = shared_rc;
        } else {
            return;
        }
        let mut shared = shared.borrow_mut();

        let mut layers = self.layers.borrow_mut();
        let layers: &mut Layers = layers.as_mut().unwrap();

        let shared: &mut Shared = &mut *shared; // enable split borrow
        let elem = &shared.style_elem;
        let sheet_set = shared.style_manager.sheet_set();

        macro_rules! compute_prop {
            ($prop:expr, PropValue::$type:ident) => {
                match elem.compute_prop(&sheet_set, $prop) {
                    PropValue::$type(v) => v,
                    _ => unreachable!(),
                }
            };
        }

        let dirty_kind = shared.dirty_kind;
        shared.dirty_kind -= flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}];

        // Adjust the layer count
        if dirty_kind.intersects(PropKindFlags::NUM_LAYERS) {
            let num_layers = compute_prop!(Prop::NumLayers, PropValue::Usize);
            let styled = &mut layers.styled;

            while num_layers < styled.len() {
                wm.remove_layer(&styled.pop().unwrap());
            }
            styled.resize_with(num_layers, || wm.new_layer(pal::LayerAttrs::default()));
        }

        let container = view.global_frame();

        // Update layer properties
        let prop_flags = PropKindFlags::LAYER_ALL - PropKindFlags::NUM_LAYERS;
        if dirty_kind.intersects(prop_flags) {
            for (i, layer) in layers.styled.iter().enumerate() {
                let layer_id = i as u32;
                let mut layer_attrs = pal::LayerAttrs::default();

                if dirty_kind.intersects(PropKindFlags::LAYER_BOUNDS) {
                    let met = compute_prop!(Prop::LayerMetrics(layer_id), PropValue::Metrics);
                    let bounds = met.arrange(container, Vector2::new(0.0, 0.0));
                    layer_attrs.bounds = Some(bounds);
                }

                if dirty_kind.intersects(PropKindFlags::LAYER_IMG) {
                    let img = compute_prop!(Prop::LayerImg(layer_id), PropValue::Himg);

                    if let Some(img) = img {
                        let (bmp, content_scale) = img.new_bmp(wm, ctx.hwnd().dpi_scale());

                        layer_attrs.contents = Some(Some(bmp));
                        layer_attrs.contents_scale = Some(content_scale);
                    } else {
                        layer_attrs.contents = Some(None);
                    }
                }

                if dirty_kind.intersects(PropKindFlags::LAYER_BG_COLOR) {
                    let value = compute_prop!(Prop::LayerBgColor(layer_id), PropValue::Rgbaf32);
                    layer_attrs.bg_color = Some(value);
                }

                if dirty_kind.intersects(PropKindFlags::LAYER_OPACITY) {
                    let value = compute_prop!(Prop::LayerOpacity(layer_id), PropValue::Float);
                    layer_attrs.opacity = Some(value);
                }

                if dirty_kind.intersects(PropKindFlags::LAYER_CENTER) {
                    let value = compute_prop!(Prop::LayerCenter(layer_id), PropValue::Box2);
                    layer_attrs.contents_center = Some(value);
                }

                if dirty_kind.intersects(PropKindFlags::LAYER_XFORM) {
                    let xform = compute_prop!(Prop::LayerXform(layer_id), PropValue::LayerXform);

                    let met = compute_prop!(Prop::LayerMetrics(layer_id), PropValue::Metrics);
                    let bounds = met.arrange(container, Vector2::new(0.0, 0.0));

                    let mat = xform.to_matrix3(bounds);

                    layer_attrs.transform = Some(mat);
                }

                wm.set_layer_attr(layer, layer_attrs);
            }
        }

        // Update the clip layer's properties
        if let Some(clip) = &layers.clip {
            if dirty_kind.intersects(PropKindFlags::CLIP_LAYER) {
                let met = compute_prop!(Prop::ClipMetrics, PropValue::Metrics);

                let bounds = met.arrange(container, Vector2::new(0.0, 0.0));

                wm.set_layer_attr(
                    clip,
                    pal::LayerAttrs {
                        bounds: Some(bounds),
                        ..pal::LayerAttrs::default()
                    },
                );
            }
        }

        // Tell the system the layers we have
        let new_len = layers.styled.len() + (layers.clip.is_some() as usize);
        if ctx.layers().len() != new_len {
            let mut new_layers = layers.styled.clone();

            new_layers.extend(layers.clip.iter().cloned());

            ctx.set_layers(new_layers);
        }
    }
}
