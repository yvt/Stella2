use alt_fp::FloatOrd;
use cggeom::{box2, Box2};
use cgmath::Vector2;
use flags_macro::flags;
use log::trace;
use momo::momo;
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::{Rc, Weak},
};

use super::{
    manager::{Elem, HElem, Manager, PropKindFlags},
    style::{ClassSet, Metrics, Prop, PropValue, Role, ROLE_COUNT},
    widget::Widget,
};
use crate::{
    pal,
    pal::prelude::*,
    uicore::{
        HView, HViewRef, HWndRef, Layout, LayoutCtx, SizeTraits, Sub, UpdateCtx, ViewFlags,
        ViewListener,
    },
};

/// A box styled based on styling properties.
///
/// The following [`Prop`]s are handled: `NumLayers`, `LayerImg`,
/// `LayerBgColor`, `LayerMetrics`, `LayerOpacity`, `LayerCenter`, `LayerXform`,
/// `SubviewMetrics`, `SubviewVisibility`, `ClipMetrics`, and `MinSize`.
///
/// [`Prop`]: crate::ui::theming::Prop
#[derive(Debug)]
pub struct StyledBox {
    view: HView,
    shared: Rc<Shared>,
}

/// Programmatically overrides [`StyledBox`]'s behavior for fine control.
pub trait StyledBoxOverride: 'static + as_any::AsAny {
    /// Modify the frame of a subview.
    fn modify_arrangement(&self, args: ModifyArrangementArgs<'_>) {
        let _ = args;
    }

    /// Compare this `StyledBoxOverride` against another to calculate dirty
    /// flags. `other` must not refer to the same object as `self`.
    ///
    /// The default implementation conservatively returns `PropKindFlags::all()`.
    /// Custom implementations may calculate and return more precise flags.
    fn dirty_flags(&self, _other: &dyn StyledBoxOverride) -> PropKindFlags {
        PropKindFlags::all()
    }
}

impl StyledBoxOverride for () {}

impl<T: StyledBoxOverride + 'static> From<T> for Box<dyn StyledBoxOverride> {
    fn from(x: T) -> Box<dyn StyledBoxOverride> {
        Box::new(x)
    }
}

/// A set of arguments for [`StyledBoxOverride::modify_arrangement`].
#[derive(Debug)]
pub struct ModifyArrangementArgs<'a> {
    pub role: Role,
    pub frame: &'a mut Box2<f32>,
    pub size_traits: &'a SizeTraits,
    pub size: &'a Vector2<f32>,
}

struct Shared {
    view: HView,

    auto_class_set: Cell<ClassSet>,

    style_elem: Elem,
    dirty: Cell<PropKindFlags>,

    subviews: RefCell<Vec<(Role, HView)>>,
    subelems: [Cell<Option<HElem>>; ROLE_COUNT],
    /// `override` is a reserved keyword, so `overrider` is used here
    overrider: RefCell<Rc<dyn StyledBoxOverride>>,

    has_layer_group: bool,
}

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Shared")
            .field("view", &self.view)
            .field("auto_class_set", &self.auto_class_set)
            .field("style_elem", &self.style_elem)
            .field("dirty", &self.dirty)
            .field("subviews", &self.subviews)
            .field("subelems", &self.subelems)
            .field("overrider", &())
            .field("has_layer_group", &self.has_layer_group)
            .finish()
    }
}

impl StyledBox {
    pub fn new(style_manager: &'static Manager, view_flags: ViewFlags) -> Self {
        // Create `Elem` based on the inital properties
        let style_elem = Elem::new(style_manager);

        // Create the initial `StyledBoxOverride`
        let overrider: Rc<dyn StyledBoxOverride> = Rc::new(());

        // Create the initial `Layout` based on the inital properties
        let subviews = Vec::new();
        let layout = SbLayout::new(&subviews, &style_elem, Rc::clone(&overrider));

        // Create and set up a `View`
        let view = HView::new(view_flags);

        let shared = Rc::new(Shared {
            view: view.clone(),
            auto_class_set: Cell::new(ClassSet::empty()),
            subviews: RefCell::new(subviews),
            subelems: Default::default(),
            overrider: RefCell::new(overrider),
            style_elem,
            // Already have an up-to-date `Layout`, so exclude it from
            // the dirty flags
            dirty: Cell::new(PropKindFlags::all() - PropKindFlags::LAYOUT),
            has_layer_group: view_flags.contains(ViewFlags::LAYER_GROUP),
        });

        view.set_listener(SbListener::new(Rc::downgrade(&shared)));
        view.set_layout(layout);

        // Get notified when the styling properties change
        {
            let shared_weak = Rc::downgrade(&shared);
            shared
                .style_elem
                .set_on_change(Box::new(move |_, kind_flags| {
                    if let Some(shared) = shared_weak.upgrade() {
                        shared.set_dirty(kind_flags);
                    }
                }));
        }

        Self { view, shared }
    }

    /// Set the class set of the styled element.
    pub fn set_class_set(&self, class_set: ClassSet) {
        let old_class_set = self.shared.style_elem.class_set();
        if class_set == old_class_set {
            return;
        }

        // Ignore changes to the auto class set
        let auto_class_set = self.shared.auto_class_set.get();
        let class_set = (class_set - auto_class_set) | (old_class_set & auto_class_set);

        trace!(
            "Updating the class set of {:?} from {:?} to {:?}",
            self.view(),
            self.shared.style_elem.class_set(),
            class_set
        );

        self.shared.style_elem.set_class_set(class_set);
    }

    /// Set a subview for the specified `Role`.
    pub fn set_subview(&self, role: Role, view: Option<HView>) {
        let mut subviews = self.shared.subviews.borrow_mut();

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

        drop(subviews);

        self.shared.set_dirty(PropKindFlags::LAYOUT);
    }

    /// Set a subelement for the specified `Role`.
    pub fn set_subelement(&self, role: Role, helem: Option<HElem>) {
        let elem_cell = &self.shared.subelems[role as usize];

        if let Some(e) = elem_cell.get() {
            self.shared.style_elem.remove_child(e);
        }
        if let Some(e) = helem {
            self.shared.style_elem.insert_child(e);
        }

        elem_cell.set(helem);
    }

    /// Set a child widget using `set_subview` and `set_subelement`.
    pub fn set_child(&self, role: Role, widget: Option<&dyn Widget>) {
        if let Some(widget) = widget {
            self.set_subview(role, Some(widget.view_ref().cloned()));
            self.set_subelement(role, widget.style_elem());
        } else {
            self.set_subview(role, None);
            self.set_subelement(role, None);
        }
    }

    /// Get the class set of the styled element.
    pub fn class_set(&self) -> ClassSet {
        self.shared.style_elem.class_set()
    }

    /// Set a new [`StyledBoxOverride`]  object.
    #[momo]
    pub fn set_override(&self, new_override: impl Into<Box<dyn StyledBoxOverride>>) {
        // `impl Into<Box<_>>` → `Box<_>` → `Rc<_>`
        // (We can't blanket-implement `From<impl Trait>` on `Rc<dyn Trait>`.
        // It looks like `Box` is special-cased to make this possible. This is
        // unfortunate because, as a result, every conversion here involves
        // dynamic memory allocation.)
        let new_override: Box<dyn StyledBoxOverride> = new_override.into();
        let new_override: Rc<dyn StyledBoxOverride> = Rc::from(new_override);

        let mut override_cell = self.shared.overrider.borrow_mut();

        // Calculate dirty flags
        let dirty_flags = new_override.dirty_flags(&**override_cell);

        // Replace `overrider`
        *override_cell = new_override;

        drop(override_cell);

        self.shared.set_dirty(dirty_flags);
    }

    /// Set the auto class set.
    ///
    /// The auto class set is a set of styling classes controlled by
    /// `StyledBox`. The following classes are supported: `HOVER` and `FOCUS`.
    ////
    /// The auto class set defaults to empty.
    pub fn set_auto_class_set(&self, class_set: ClassSet) {
        self.shared.auto_class_set.set(class_set);
    }

    /// Get the auto class set.
    pub fn auto_class_set(&self) -> ClassSet {
        self.shared.auto_class_set.get()
    }

    /// Get an owned handle to the view representing the styled box.
    pub fn view(&self) -> HView {
        self.view.clone()
    }

    /// Borrow the handle to the view representing the styled box.
    pub fn view_ref(&self) -> HViewRef<'_> {
        self.view.as_ref()
    }

    /// Get the styling element representing the styled box.
    pub fn style_elem(&self) -> HElem {
        self.shared.style_elem.helem()
    }
}

#[doc(hidden)]
/// Work-arounds the lack of indexed prop support in Designer.
impl StyledBox {
    pub fn set_subview_generic(&self, view: impl Into<Option<HView>>) {
        self.set_subview(Role::Generic, view.into());
    }

    pub fn set_subelement_generic(&self, element: impl Into<Option<HElem>>) {
        self.set_subelement(Role::Generic, element.into());
    }

    pub fn set_child_generic(&self, widget: &dyn Widget) {
        self.set_child(Role::Generic, Some(widget));
    }
}

impl Widget for StyledBox {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view_ref()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

impl Shared {
    /// Dispatch update methods based on a `PropKindFlags`
    fn set_dirty(&self, mut diff: PropKindFlags) {
        let dirty = &self.dirty;
        diff |= dirty.get();

        if diff.intersects(PropKindFlags::LAYOUT) {
            self.view.set_layout(SbLayout::new(
                &self.subviews.borrow(),
                &self.style_elem,
                Rc::clone(&self.overrider.borrow()),
            ));
        }

        if diff.intersects(flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}]) {
            self.view.pend_update();
        }

        dirty.set(diff - PropKindFlags::LAYOUT);
    }
}

struct SbLayout {
    subview_layout: Vec<(Role, Metrics)>,
    subviews: Vec<HView>,
    min_size: Vector2<f32>,
    overrider: Rc<dyn StyledBoxOverride>,
}

impl SbLayout {
    fn new(subviews: &[(Role, HView)], elem: &Elem, overrider: Rc<dyn StyledBoxOverride>) -> Self {
        // Evaluate the layout properties now
        let subviews_filtered = subviews.iter().filter(|&&(role, _)| {
            match elem.compute_prop(Prop::SubviewVisibility(role)) {
                PropValue::Bool(b) => b,
                _ => unreachable!(),
            }
        });

        Self {
            subview_layout: subviews_filtered
                .clone()
                .map(
                    |&(role, _)| match elem.compute_prop(Prop::SubviewMetrics(role)) {
                        PropValue::Metrics(m) => (role, m),
                        _ => unreachable!(),
                    },
                )
                .collect(),
            subviews: subviews_filtered.map(|x| x.1.clone()).collect(),
            min_size: match elem.compute_prop(Prop::MinSize) {
                PropValue::Vector2(v) => v,
                _ => unreachable!(),
            },
            overrider,
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

        for ((_, metrics), sv) in self.subview_layout.iter().zip(self.subviews.iter()) {
            let margin = &metrics.margin;
            let mut sv_traits = ctx.subview_size_traits(sv.as_ref());

            if !metrics.size.x.is_nan() {
                sv_traits.min.x = metrics.size.x;
                sv_traits.max.x = metrics.size.x;
            }

            if !metrics.size.y.is_nan() {
                sv_traits.min.y = metrics.size.y;
                sv_traits.max.y = metrics.size.y;
            }

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

        traits.preferred.x = traits.preferred.x.fmin(traits.max.x);
        traits.preferred.y = traits.preferred.y.fmin(traits.max.y);

        traits.preferred.x = traits.preferred.x.fmax(traits.min.x);
        traits.preferred.y = traits.preferred.y.fmax(traits.min.y);

        traits
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        for (&(role, ref metrics), sv) in self.subview_layout.iter().zip(self.subviews.iter()) {
            let sv_traits = ctx.subview_size_traits(sv.as_ref());
            let container = box2! {top_left: [0.0, 0.0].into(), size: size.into()};

            let mut frame = metrics.arrange(container, sv_traits.preferred);

            self.overrider.modify_arrangement(ModifyArrangementArgs {
                role,
                frame: &mut frame,
                size: &size,
                size_traits: &sv_traits,
            });

            ctx.set_subview_frame(sv.as_ref(), frame);
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
    shared: Weak<Shared>,
    layers: RefCell<Option<Layers>>,
}

#[derive(Default)]
struct Layers {
    clip: Option<pal::HLayer>,
    styled: Vec<pal::HLayer>,
    sub: Option<Sub>,
}

impl SbListener {
    fn new(shared: Weak<Shared>) -> Self {
        Self {
            shared,
            layers: RefCell::new(None),
        }
    }

    fn toggle_auto_class(&self, andn_mask: ClassSet, or_mask: ClassSet) {
        if let Some(shared) = self.shared.upgrade() {
            if shared.auto_class_set.get().contains(andn_mask) {
                trace!(
                    "Toggling the auto class {:?} of {:?} with OR mask {:?}",
                    andn_mask,
                    shared.view,
                    or_mask,
                );
                let elem = &shared.style_elem;
                elem.set_class_set((elem.class_set() - andn_mask) | or_mask);
            } else {
                trace!(
                    "Not toggling the auto class {:?} of {:?} because it's not in `auto_class_set`",
                    andn_mask,
                    shared.view,
                );
            }
        }
    }

    /// Add `class_set` if it's included in `auto_class_set`.
    #[inline]
    fn add_auto_class(&self, class_set: ClassSet) {
        self.toggle_auto_class(class_set, class_set);
    }

    /// Remove `class_set` if it's included in `auto_class_set`.
    #[inline]
    fn remove_auto_class(&self, class_set: ClassSet) {
        self.toggle_auto_class(class_set, ClassSet::empty());
    }
}

impl ViewListener for SbListener {
    fn mount(&self, wm: pal::Wm, _: HViewRef<'_>, wnd: HWndRef<'_>) {
        let mut layers = self.layers.borrow_mut();
        assert!(layers.is_none());

        if let Some(shared) = self.shared.upgrade() {
            // Insert fake dirty flags to set the inital layer properties
            let dirty = &shared.dirty;
            dirty.set(dirty.get() | flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}]);

            // Watch for DPI scale changes
            let sub = {
                let shared = self.shared.clone();
                wnd.subscribe_dpi_scale_changed(Box::new(move |_, _| {
                    if let Some(shared) = shared.upgrade() {
                        shared.set_dirty(PropKindFlags::LAYER_IMG);
                    }
                }))
            };

            // Create layers. Properties are set later in `update` (This happens
            // because of the fake dirty flags we inserted).
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

            shared.view.pend_update();
        } else {
            *layers = Some(Layers::default());
        }

        self.remove_auto_class(ClassSet::HOVER);
        self.remove_auto_class(ClassSet::FOCUS);
    }

    fn unmount(&self, wm: pal::Wm, _: HViewRef<'_>) {
        let layers = self.layers.borrow_mut().take().unwrap();

        if let Some(layer) = layers.clip {
            wm.remove_layer(&layer);
        }
        for layer in layers.styled {
            wm.remove_layer(&layer);
        }

        if let Some(sub) = layers.sub {
            sub.unsubscribe().unwrap();
        }
    }

    fn mouse_enter(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.add_auto_class(ClassSet::HOVER);
    }

    fn mouse_leave(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.remove_auto_class(ClassSet::HOVER);
    }

    fn focus_enter(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.add_auto_class(ClassSet::FOCUS);
    }

    fn focus_leave(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.remove_auto_class(ClassSet::FOCUS);
    }

    fn position(&self, _: pal::Wm, _: HViewRef<'_>) {
        if let Some(shared) = self.shared.upgrade() {
            shared.set_dirty(PropKindFlags::LAYER_BOUNDS);
        }
    }

    fn update(&self, wm: pal::Wm, view: HViewRef<'_>, ctx: &mut UpdateCtx<'_>) {
        let shared;
        if let Some(shared_rc) = self.shared.upgrade() {
            shared = shared_rc;
        } else {
            return;
        }

        let mut layers = self.layers.borrow_mut();
        let layers: &mut Layers = layers.as_mut().unwrap();

        let elem = &shared.style_elem;

        macro_rules! compute_prop {
            ($prop:expr, PropValue::$type:ident) => {
                match elem.compute_prop($prop) {
                    PropValue::$type(v) => v,
                    _ => unreachable!(),
                }
            };
        }

        let dirty = shared.dirty.get();
        shared
            .dirty
            .set(dirty - flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}]);

        // Adjust the layer count
        if dirty.intersects(PropKindFlags::NUM_LAYERS) {
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
        if dirty.intersects(prop_flags) {
            for (i, layer) in layers.styled.iter().enumerate() {
                let layer_id = i as u32;
                let mut layer_attrs = pal::LayerAttrs::default();

                if dirty.intersects(PropKindFlags::LAYER_BOUNDS) {
                    let met = compute_prop!(Prop::LayerMetrics(layer_id), PropValue::Metrics);
                    let bounds = met.arrange(container, Vector2::new(0.0, 0.0));
                    layer_attrs.bounds = Some(bounds);
                }

                if dirty.intersects(PropKindFlags::LAYER_IMG) {
                    let img = compute_prop!(Prop::LayerImg(layer_id), PropValue::Himg);

                    if let Some(img) = img {
                        let (bmp, content_scale) = img.new_bmp(wm, ctx.hwnd().dpi_scale());

                        layer_attrs.contents = Some(Some(bmp));
                        layer_attrs.contents_scale = Some(content_scale);
                    } else {
                        layer_attrs.contents = Some(None);
                    }
                }

                if dirty.intersects(PropKindFlags::LAYER_BG_COLOR) {
                    let value = compute_prop!(Prop::LayerBgColor(layer_id), PropValue::Rgbaf32);
                    layer_attrs.bg_color = Some(value);
                }

                if dirty.intersects(PropKindFlags::LAYER_OPACITY) {
                    let value = compute_prop!(Prop::LayerOpacity(layer_id), PropValue::Float);
                    layer_attrs.opacity = Some(value);
                }

                if dirty.intersects(PropKindFlags::LAYER_CENTER) {
                    let value = compute_prop!(Prop::LayerCenter(layer_id), PropValue::Box2);
                    layer_attrs.contents_center = Some(value);
                }

                if dirty.intersects(PropKindFlags::LAYER_FLAGS) {
                    let value = compute_prop!(Prop::LayerFlags(layer_id), PropValue::LayerFlags);
                    layer_attrs.flags = Some(value);
                }

                if dirty.intersects(PropKindFlags::LAYER_XFORM | PropKindFlags::LAYER_BOUNDS) {
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
            if dirty.intersects(PropKindFlags::CLIP_LAYER) {
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
