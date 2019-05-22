//! Provides the core UI service.
//!
//! # Layouting
//!
//! TCW3 implements a two-phase layouting algorithm. The algoritm consists of
//! the following steps:
//!
//!  - *Up phase*: `SizeTraits` (a triplet of min/max/preferred sizes) is
//!    calculated for each view in a top-down manner using the local properties
//!    and subviews' `SizeTraits`.
//!  - The window size is constrained based on the root view's `SizeTraits`. The
//!    root view's frame always matches the window size.
//!  - *Down phase*: The final frame (a bounding rectangle in the superview
//!    coordinate space) is calculated for each view in a bottom-up manner.
//!
use bitflags::bitflags;
use cggeom::{prelude::*, Box2};
use cgmath::Point2;
use derive_more::From;
use flags_macro::flags;
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::{Rc, Weak},
};
use subscriber_list::{SubscriberList, UntypedSubscription};

use crate::pal::{self, prelude::WM as _, WM};

mod layer;
mod layout;
mod mount;
mod mouse;
mod window;

pub use self::layer::{UpdateCtx, UpdateReason};
pub use self::layout::{Layout, LayoutCtx, SizeTraits};
pub use self::mouse::MouseDragListener;

pub use crate::pal::WndFlags as WndStyleFlags;

/// A window handle type.
///
/// A window is automatically closed when there is no longer a handle associated
/// with the window. The application must maintain a `HWnd` to keep a window
/// displayed, and drop it when [`WndListener::close`] is called.
#[derive(Debug, Clone)]
pub struct HWnd {
    wnd: Rc<Wnd>,
}

pub trait WndListener {
    /// The user has attempted to close a window. Returns `true` if the window
    /// can be closed.
    fn close_requested(&self, _: WM, _: &HWnd) -> bool {
        true
    }

    /// A window has been closed.
    fn close(&self, _: WM, _: &HWnd) {}
}

/// A no-op implementation of `WndListener`.
impl WndListener for () {}

/// The boxed function type for window events with no extra parameters.
pub type WndEvtHandler = Box<dyn Fn(WM, &HWnd)>;

/// Represents an event subscription.
///
/// This type is returned by a method such as
/// [`HWnd::subscribe_dpi_scale_changed`]. The client can unregister event
/// handlers by calling the `Sub::unsubscribe` method.
pub type Sub = UntypedSubscription;

struct Wnd {
    wm: WM,
    dirty: Cell<window::WndDirtyFlags>,
    pal_wnd: RefCell<Option<pal::HWnd>>,
    listener: RefCell<Box<dyn WndListener>>,
    /// A flag indicating whether the window has been closed.
    closed: Cell<bool>,
    /// The content view, which can be `None` only after the window is closed.
    content_view: RefCell<Option<HView>>,
    style_attrs: RefCell<window::WndStyleAttrs>,
    updating: Cell<bool>,
    dpi_scale_changed_handlers: RefCell<SubscriberList<WndEvtHandler>>,

    // Mouse inputs
    mouse_state: RefCell<mouse::WndMouseState>,
}

impl fmt::Debug for Wnd {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Wnd")
            .field("wm", &self.wm)
            .field("dirty", &self.dirty)
            .field("pal_wnd", &self.pal_wnd)
            .field(
                "listener",
                &self.listener.try_borrow().map(|x| &*x as *const _),
            )
            .field("closed", &self.closed)
            .field("content_view", &self.content_view)
            .field("style_attrs", &self.style_attrs)
            .field("updating", &self.updating)
            .field("dpi_scale_changed_handlers", &())
            .field("mouse_state", &self.mouse_state)
            .finish()
    }
}

impl Wnd {
    fn new(wm: WM) -> Self {
        let content_view = window::new_root_content_view();

        // Pend mount
        content_view.set_dirty_flags(ViewDirtyFlags::MOUNT);

        Self {
            wm,
            dirty: Cell::new(Default::default()),
            pal_wnd: RefCell::new(None),
            listener: RefCell::new(Box::new(())),
            closed: Cell::new(false),
            content_view: RefCell::new(Some(content_view)),
            style_attrs: RefCell::new(Default::default()),
            updating: Cell::new(false),
            dpi_scale_changed_handlers: RefCell::new(SubscriberList::new()),
            mouse_state: RefCell::new(mouse::WndMouseState::new()),
        }
    }
}

// TODO: scroll wheel events
// TODO: mouse motion events
// TODO: mouse enter/leave events
// TODO: keyboard events
// TODO: keyboard focus management

/// A view handle type.
#[derive(Debug, Clone)]
pub struct HView {
    view: Rc<View>,
}

bitflags! {
    pub struct ViewFlags: u8 {
        /// The sublayers are added to the view's associated layer.
        ///
        /// This makes it possible to clip subviews using the layer's border
        /// or apply group opacity.
        ///
        /// If this bit is set, the client should implement
        /// [`ViewListener::update`] and add [`UpdateCtx::sublayers`]`()` to
        /// a client-provided PAL layer as sublayers.
        ///
        /// This flag cannot be added or removed once a view is created.
        const LAYER_GROUP = 1 << 0;

        /// Clip hit testing (e.g., the one performed when the user presses
        /// a mouse button) by the view's frame.
        const CLIP_HITTEST = 1 << 1;

        /// Prevent the view and its subviews from accepting mouse events.
        const DENY_MOUSE = 1 << 2;

        /// The view accepts mouse drag events.
        const ACCEPT_MOUSE_DRAG = 1 << 3;
    }
}

impl Default for ViewFlags {
    fn default() -> Self {
        ViewFlags::CLIP_HITTEST
    }
}

impl ViewFlags {
    fn mutable_flags() -> Self {
        flags![ViewFlags::{CLIP_HITTEST | DENY_MOUSE | ACCEPT_MOUSE_DRAG}]
    }
}

/// View event handlers.
///
/// It's generally not safe to modify view properties and/or hierarchy from
/// these methods. Consider deferring modifications using `WM::invoke`.
pub trait ViewListener {
    /// A view was added to a window.
    ///
    /// If the view has an associated layer, it's advised to insert a call to
    /// [`HView::pend_update`] here.
    fn mount(&self, _: WM, _: &HView, _: &HWnd) {}

    /// A view was removed from a window.
    fn unmount(&self, _: WM, _: &HView) {}

    /// A view was repositioned, i.e., [`HView::global_frame`]`()` has been
    /// updated.
    ///
    /// If the view has an associated layer, it's advised to insert a call to
    /// [`HView::pend_update`] here.
    fn position(&self, _: WM, _: &HView) {}

    /// A view should be updated.
    ///
    /// This method is called after [`HView::pend_update`] is called or a view
    /// is added to a window for the first time.
    /// The system automatically flushes changes to the layers by calling
    /// [`WM::update_wnd`] after calling this method for all
    /// pending views, so this is the optimal place to update the properties of
    /// associated layers (if any).
    ///
    /// [`WM::update_wnd`]: crate::pal::iface::WM::update_wnd
    fn update(&self, _: WM, _: &HView, _: &mut UpdateCtx<'_>) {}

    /// Get event handlers for handling the mouse drag gesture initiated by
    /// a mouse down event described by `loc` and `button`.
    ///
    /// This method is called when a mouse button is pressed for the first time.
    /// The returned `MouseDragListener` will be used to handle subsequent
    /// mouse events (including the mouse down event that initiated the call)
    /// until all mouse buttons are released.
    ///
    /// You must set [`ViewFlags::ACCEPT_MOUSE_DRAG`] for this to be called.
    fn mouse_drag(
        &self,
        _: WM,
        _: &HView,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn MouseDragListener> {
        Box::new(())
    }
}

/// A no-op implementation of `ViewListener`.
impl ViewListener for () {}

struct View {
    dirty: Cell<ViewDirtyFlags>,
    flags: Cell<ViewFlags>,

    listener: RefCell<Box<dyn ViewListener>>,
    layout: RefCell<Box<dyn Layout>>,
    superview: RefCell<Superview>,

    // Layouting
    size_traits: Cell<SizeTraits>,
    frame: Cell<Box2<f32>>,
    global_frame: Cell<Box2<f32>>,

    // Layers
    layers: RefCell<Vec<pal::HLayer>>,
}

impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("View")
            .field("dirty", &self.dirty)
            .field("flags", &self.flags)
            .field(
                "listener",
                &self.listener.try_borrow().map(|x| &*x as *const _),
            )
            .field("layout", &self.layout)
            .field("superview", &self.superview)
            .field("size_traits", &self.size_traits)
            .field("frame", &self.frame)
            .field("global_frame", &self.global_frame)
            .field("layers", &self.layers)
            .finish()
    }
}

impl View {
    fn new(flags: ViewFlags) -> Self {
        let mut dirty = ViewDirtyFlags::UPDATE_EVENT;

        if flags.contains(ViewFlags::LAYER_GROUP) {
            dirty |= ViewDirtyFlags::SUBLAYERS;
        }

        Self {
            dirty: Cell::new(dirty),
            flags: Cell::new(flags),
            listener: RefCell::new(Box::new(())),
            layout: RefCell::new(Box::new(())),
            superview: RefCell::new(Superview::empty()),
            size_traits: Cell::new(SizeTraits::default()),
            frame: Cell::new(Box2::zero()),
            global_frame: Cell::new(Box2::zero()),
            layers: RefCell::new(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, From)]
enum Superview {
    View(Weak<View>),
    Window(Weak<Wnd>),
}

#[derive(Debug, Clone)]
enum SuperviewStrong {
    View(Rc<View>),
    Window(Rc<Wnd>),
}

impl Superview {
    fn empty() -> Self {
        Superview::View(Weak::new())
    }

    fn is_empty(&self) -> bool {
        match self {
            Superview::View(weak) => weak.strong_count() == 0,
            Superview::Window(weak) => weak.strong_count() == 0,
        }
    }

    fn upgrade(&self) -> Option<SuperviewStrong> {
        match self {
            Superview::View(weak) => weak.upgrade().map(SuperviewStrong::View),
            Superview::Window(weak) => weak.upgrade().map(SuperviewStrong::Window),
        }
    }

    fn view(&self) -> Option<&Weak<View>> {
        match self {
            Superview::View(weak) => Some(weak),
            Superview::Window(_) => None,
        }
    }

    fn wnd(&self) -> Option<&Weak<Wnd>> {
        match self {
            Superview::View(_) => None,
            Superview::Window(weak) => Some(weak),
        }
    }
}

impl PartialEq<Weak<View>> for Superview {
    fn eq(&self, other: &Weak<View>) -> bool {
        match self {
            Superview::View(weak) => Weak::ptr_eq(weak, other),
            Superview::Window(_) => false,
        }
    }
}

// =======================================================================
//                            Public methods
// =======================================================================

impl HWnd {
    /// Construct a window object and return a handle to it.
    pub fn new(wm: WM) -> Self {
        let hwnd = Self {
            wnd: Rc::new(Wnd::new(wm)),
        };

        // Now, set `superview` of the default content view.
        *hwnd
            .wnd
            .content_view
            .borrow()
            .as_ref()
            .unwrap()
            .view
            .superview
            .borrow_mut() = Superview::Window(Rc::downgrade(&hwnd.wnd));

        hwnd
    }

    /// Close a window.
    ///
    /// Closing a window ensures that all operating system resources associated
    /// with the window are released.
    pub fn close(&self) {
        self.wnd.close();
    }

    /// Get the DPI scaling factor for a window.
    ///
    /// This function returns `1.0` if the window is not materialized yet.
    pub fn dpi_scale(&self) -> f32 {
        if let Some(ref pal_wnd) = &*self.wnd.pal_wnd.borrow() {
            self.wnd.wm.get_wnd_dpi_scale(pal_wnd)
        } else {
            1.0
        }
    }

    /// Register a function that gets called whenever `dpi_scene` changes.
    ///
    /// Returns a [`subscriber_list::UntypedSubscription`], which can be used to
    /// unregister the function.
    pub fn subscribe_dpi_scale_changed(&self, cb: WndEvtHandler) -> Sub {
        self.wnd
            .dpi_scale_changed_handlers
            .borrow_mut()
            .insert(cb)
            .untype()
    }

    /// Get the content view of a window.
    pub fn content_view(&self) -> HView {
        self.wnd.content_view.borrow().clone().unwrap()
    }

    /// Set the content view of a window.
    ///
    /// `view` must have [`ViewFlags::LAYER_GROUP`].
    /// `view.listener.borrow().update` ([`ViewListener::update`]) must return
    /// *exactly one layer* as the view's associated layer.
    pub fn set_content_view(&self, view: HView) {
        assert!(
            view.view.flags.get().contains(ViewFlags::LAYER_GROUP),
            "the view must have LAYER_GROUP"
        );
        assert!(!self.wnd.closed.get(), "the window has been already closed");

        let old_content_view;
        {
            let mut content_view = self.wnd.content_view.borrow_mut();

            // Return early if there's no change. This simplifies the following
            // "already added to another view" test.
            if view == *content_view.as_ref().unwrap() {
                return;
            }

            debug_assert!(
                view.view.superview.borrow().is_empty(),
                "the view already has a parent"
            );

            // Pend a call to `ViewListener::mount`
            let dirty = &view.view.dirty;
            dirty.set(dirty.get() | ViewDirtyFlags::MOUNT);

            old_content_view = std::mem::replace(&mut *content_view, Some(view));

            // Pend a root layer change
            let dirty = &self.wnd.dirty;
            dirty.set(dirty.get() | window::WndDirtyFlags::LAYER);
        }

        // Unmount the old content view
        let old_content_view = old_content_view.unwrap();
        old_content_view.cancel_mouse_gestures_of_subviews(&self.wnd);
        old_content_view.call_unmount(self.wnd.wm);

        self.pend_update();
    }

    /// Set the window listener.
    pub fn set_listener(&self, listener: Box<dyn WndListener>) {
        *self.wnd.listener.borrow_mut() = listener;
    }

    /// Set the visibility of a window.
    ///
    /// The default value is `false`. Note that hiding a window doesn't release
    /// resources associated with it.
    pub fn set_visibility(&self, visible: bool) {
        let mut style_attrs = self.wnd.style_attrs.borrow_mut();
        if style_attrs.visible == visible {
            return;
        }
        style_attrs.visible = visible;
        self.wnd
            .set_dirty_flags(window::WndDirtyFlags::STYLE_VISIBLE);
        self.pend_update();
    }

    /// Get the visibility of a window.
    pub fn visibility(&self) -> bool {
        self.wnd.style_attrs.borrow().visible
    }

    /// Set the caption of a window.
    ///
    /// The default value is `false`.
    pub fn set_caption(&self, caption: impl Into<String>) {
        self.set_caption_inner(caption.into());
    }

    fn set_caption_inner(&self, caption: String) {
        let mut style_attrs = self.wnd.style_attrs.borrow_mut();
        if style_attrs.caption == caption {
            return;
        }
        style_attrs.caption = caption;
        self.wnd
            .set_dirty_flags(window::WndDirtyFlags::STYLE_CAPTION);
        self.pend_update();
    }

    /// Get the caption of a window.
    pub fn caption(&self) -> String {
        self.wnd.style_attrs.borrow().caption.clone()
    }

    /// Set the style flags of a window.
    ///
    /// The default value is `false`.
    pub fn set_style_flags(&self, flags: WndStyleFlags) {
        let mut style_attrs = self.wnd.style_attrs.borrow_mut();
        if style_attrs.flags == flags {
            return;
        }
        style_attrs.flags = flags;
        self.wnd.set_dirty_flags(window::WndDirtyFlags::STYLE_FLAGS);
        self.pend_update();
    }

    /// Get the style flags of a window.
    pub fn style_flags(&self) -> WndStyleFlags {
        self.wnd.style_attrs.borrow().flags
    }
}

impl PartialEq for HWnd {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.wnd, &other.wnd)
    }
}

impl Eq for HWnd {}

impl HView {
    /// Construct a view object and return a handle to it.
    pub fn new(flags: ViewFlags) -> Self {
        Self {
            view: Rc::new(View::new(flags)),
        }
    }

    /// Set a new [`ViewListener`].
    ///
    /// It's now allowed to call this method from `ViewListener`'s methods.
    pub fn set_listener(&self, listener: Box<dyn ViewListener>) {
        *self.view.listener.borrow_mut() = listener;
    }

    /// Set a new [`Layout`].
    ///
    /// It's not allowed to call this method from [`ViewListener::update`].
    pub fn set_layout(&self, layout: Box<dyn Layout>) {
        let mut cur_layout = self.view.layout.borrow_mut();
        let subviews_changed = !layout.has_same_subviews(&**cur_layout);

        let mut new_flags = ViewDirtyFlags::empty();

        if subviews_changed {
            // Disconnect old subviews
            for hview_sub in cur_layout.subviews().iter() {
                let mut sup_view = hview_sub.view.superview.borrow_mut();
                debug_assert_eq!(
                    *sup_view,
                    Rc::downgrade(&self.view),
                    "existing subview's superview is invalid"
                );
                *sup_view = Superview::empty();
            }

            // Connect new subviews
            for hview_sub in layout.subviews().iter() {
                let mut sup_view = hview_sub.view.superview.borrow_mut();
                debug_assert!(
                    sup_view.is_empty(),
                    "cannot add a subview already added to another view"
                );
                *sup_view = Rc::downgrade(&self.view).into();

                // Propagate dirty flags
                new_flags |= hview_sub.view.dirty.get();
            }

            new_flags = new_flags.raise_level();

            // Is there any unseen view?
            for hview_sub in layout.subviews().iter() {
                if !hview_sub.view.dirty.get().contains(ViewDirtyFlags::MOUNTED) {
                    new_flags |= ViewDirtyFlags::MOUNT;
                    break;
                }
            }

            // Pend the update of the containing layer's sublayer set
            if let Some(vwcl) = self.view_with_containing_layer() {
                vwcl.set_dirty_flags(ViewDirtyFlags::SUBLAYERS);
                vwcl.set_dirty_flags_on_superviews(ViewDirtyFlags::DESCENDANT_SUBLAYERS);
            }
        }

        self.set_dirty_flags(flags![ViewDirtyFlags::{SUBVIEWS_FRAME | SIZE_TRAITS}] | new_flags);
        self.set_dirty_flags_on_superviews(
            flags![ViewDirtyFlags::{DESCENDANT_SUBVIEWS_FRAME | DESCENDANT_SIZE_TRAITS}]
                | new_flags,
        );

        // Replace the layout
        let old_layout = std::mem::replace(&mut *cur_layout, layout);
        drop(cur_layout);

        if subviews_changed && self.view.dirty.get().contains(ViewDirtyFlags::MOUNTED) {
            // `MOUNTED` implies that the view is already added to some window
            let hwnd = self.containing_wnd().unwrap();

            // Check for disconnected views
            for hview_sub in old_layout.subviews().iter() {
                if hview_sub.view.superview.borrow().is_empty() {
                    hview_sub.cancel_mouse_gestures_of_subviews(&hwnd.wnd);
                    hview_sub.call_unmount(hwnd.wnd.wm);
                }
            }
        }
    }

    /// Set the flags of a view.
    ///
    /// Some flags cannot be added or removed once a view is created. Such flags
    /// only can be specified via [`HView::new`]. See [`ViewFlags`] for the list
    /// of immutable flags.
    pub fn set_flags(&self, value: ViewFlags) {
        let changed = value ^ self.view.flags.get();

        debug_assert_eq!(
            changed - ViewFlags::mutable_flags(),
            ViewFlags::empty(),
            "view flag(s) {:?} cannot be added or removed once a view is created",
            changed - ViewFlags::mutable_flags()
        );

        if (value & changed).contains(ViewFlags::DENY_MOUSE) {
            // The subviews are no longer allowed to have active mouse gestures, so
            // cancel them if they have any
            if let Some(hwnd) = self.containing_wnd() {
                self.cancel_mouse_gestures_of_subviews(&hwnd.wnd);
            }
        }

        if (!value & changed).contains(ViewFlags::ACCEPT_MOUSE_DRAG) {
            // The view is no longer allowed to have an active drag gesture so
            // cancel it if it has one
            if let Some(hwnd) = self.containing_wnd() {
                self.cancel_mouse_drag_gestures(&hwnd.wnd);
            }
        }

        self.view.flags.set(value);
    }

    /// Get the flags of a view.
    pub fn flags(&self) -> ViewFlags {
        self.view.flags.get()
    }

    /// Pend a call to [`ViewListener::update`].
    pub fn pend_update(&self) {
        self.set_dirty_flags(ViewDirtyFlags::UPDATE_EVENT);
        self.set_dirty_flags_on_superviews(ViewDirtyFlags::DESCENDANT_UPDATE_EVENT);
    }
}

impl PartialEq for HView {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.view, &other.view)
    }
}

impl Eq for HView {}

// =======================================================================
//                               Dirty flags
// =======================================================================

bitflags! {
    /// Indicates which property of a view should be recalculated.
    ///
    /// The flags are propagated to superviews. When some of them reach
    /// the root view, the window is scheduled to be updated (see
    /// `view_set_dirty_flags_on_superviews`). The propagation stops if they
    /// reach a view having no parent but restarts when the view is added to
    /// another view using `HView::set_layout`.
    ///
    /// They are cleared when their corresponding properties are updated.
    /// Updating properties may cause other dirty flags to be set. For example,
    /// a change in `size_traits` triggers the recalculation of subview frames,
    /// which in turn may cause `ViewListener::{position, update}` to be called.
    ///
    /// Some flags including `UPDATE_EVENT` represent calls to particular
    /// methods, not properties.
    struct ViewDirtyFlags: u16 {
        // For the implementation of `raise_level`, all `DESCENDANT` flags are
        // placed next to their non-`DESCENDANT` counterparts.

        /// `layout.size_traits()` of a view might have changed.
        const SIZE_TRAITS = 1 << 0;

        /// Some of the descendants have `SIZE_TRAITS`.
        const DESCENDANT_SIZE_TRAITS = 1 << 1;

        /// `frame` of subviews may be out-of-date.
        const SUBVIEWS_FRAME = 1 << 2;

        /// Some of the descendants have `SUBVIEWS_FRAME`.
        const DESCENDANT_SUBVIEWS_FRAME = 1 << 3;

        /// `ViewListener::position` needs to be called.
        const POSITION_EVENT = 1 << 4;

        /// Some of the descendants have `POSITION_EVENT`.
        const DESCENDANT_POSITION_EVENT = 1 << 5;

        /// `ViewListener::update` needs to be called.
        const UPDATE_EVENT = 1 << 6;

        /// Some of the descendants have `UPDATE_EVENT`.
        const DESCENDANT_UPDATE_EVENT = 1 << 7;

        /// The set of direct sublayers has changed.
        /// Only valid for layers with `ViewFlags::LAYER_GROUP`.
        const SUBLAYERS = 1 << 8;

        /// Some of the descendants have `SUBLAYERS`.
        const DESCENDANT_SUBLAYERS = 1 << 9;

        /// `ViewListener::mount` already has been called for this view.
        /// (Technically, this is not a dirty bit.)
        ///
        /// This flag implies that there is a connection to a window via
        /// `View::superview`. It also implies the superview (if any) has
        /// `MOUNTED`, too.
        const MOUNTED = 1 << 10;

        /// The view is added to a window, but `ViewListener::mount` hasn't yet
        /// been called for some of the view and its subviews.
        const MOUNT = 1 << 11;
    }
}

impl ViewDirtyFlags {
    /// Get a set of flags propagated to a superview.
    ///
    /// For example, `UPDATE_EVENT` is replaced with `DESCENDANT_UPDATE_EVENT`.
    /// On the other hand, `DESCENDANT_UPDATE_EVENT` and similar flags are
    /// left as they are.
    fn raise_level(self) -> Self {
        let thru = self
            & flags![ViewDirtyFlags::{
                DESCENDANT_SIZE_TRAITS |
                DESCENDANT_SUBVIEWS_FRAME |
                DESCENDANT_POSITION_EVENT |
                DESCENDANT_UPDATE_EVENT |
                DESCENDANT_SUBLAYERS |
                MOUNT
            }];

        let lowered = self
            & flags![ViewDirtyFlags::{
                SIZE_TRAITS |
                SUBVIEWS_FRAME |
                POSITION_EVENT |
                UPDATE_EVENT |
                SUBLAYERS
            }];

        thru | ViewDirtyFlags::from_bits_truncate(lowered.bits() << 1)
    }

    fn is_dirty(self) -> bool {
        !(self - ViewDirtyFlags::MOUNTED).is_empty()
    }
}

impl HView {
    /// Set dirty flags on a view.
    fn set_dirty_flags(&self, new_flags: ViewDirtyFlags) {
        let dirty = &self.view.dirty;
        dirty.set(dirty.get() | new_flags);
    }

    /// Set dirty flags on a view's superviews.
    fn set_dirty_flags_on_superviews(&self, new_flags: ViewDirtyFlags) {
        view_set_dirty_flags_on_superviews(&self.view, new_flags);
    }
}

/// Set dirty flags on a view and its superviews.
fn view_set_dirty_flags_on_superviews(this: &View, new_flags: ViewDirtyFlags) {
    match this.superview.borrow().upgrade() {
        None => {}
        Some(SuperviewStrong::View(sv)) => {
            let dirty = &sv.dirty;
            if dirty.get().contains(new_flags) {
                return;
            }
            dirty.set(dirty.get() | new_flags);

            view_set_dirty_flags_on_superviews(&sv, new_flags);
        }
        Some(SuperviewStrong::Window(wnd)) => {
            if new_flags.intersects(flags![ViewDirtyFlags::{
                DESCENDANT_UPDATE_EVENT | DESCENDANT_SUBLAYERS
            }]) {
                HWnd { wnd }.pend_update();
            }
        }
    }
}

// =======================================================================
//                            Helper methods
// =======================================================================

impl HView {
    /// Return `true` if `self` is an improper subview of `of_view`.
    ///
    /// The word "improper" means `x.is_improper_subview_of(x)` returns `true`.
    fn is_improper_subview_of(&self, of_view: &HView) -> bool {
        if Rc::ptr_eq(&self.view, &of_view.view) {
            true
        } else if let Some(sv) = self
            .view
            .superview
            .borrow()
            .view()
            .and_then(|weak| weak.upgrade())
        {
            HView { view: sv }.is_improper_subview_of(of_view)
        } else {
            false
        }
    }
}
