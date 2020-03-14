use cggeom::{prelude::*, Box2};
use cgmath::Point2;
use std::{
    cell::{Cell, RefCell, RefMut},
    ops::Range,
    rc::Rc,
};

use crate::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::EmptyLayout,
        mixins::CanvasMixin,
        theming::{self, ClassSet, HElem, Prop, PropKindFlags, PropValue, Role, Widget},
    },
    uicore::{
        CursorShape, HView, HViewRef, HWndRef, SizeTraits, UpdateCtx, ViewFlags, ViewListener,
        WeakHView,
    },
};

/// A text entry widget.
#[derive(Debug)]
pub struct Entry {
    styled_box: theming::StyledBox,
    core: EntryCore,
}

impl Entry {
    pub fn new(style_manager: &'static theming::Manager) -> Self {
        let core = EntryCore::new(style_manager);

        let styled_box = theming::StyledBox::new(style_manager, ViewFlags::default());
        styled_box.set_class_set(ClassSet::ENTRY);
        styled_box.set_auto_class_set(ClassSet::HOVER | ClassSet::FOCUS);
        styled_box.set_subview(Role::Generic, Some(core.view()));
        styled_box.set_subelement(Role::Generic, Some(core.style_elem()));

        Self { styled_box, core }
    }

    /// Get an owned handle to the view representing the widget.
    pub fn view(&self) -> HView {
        self.styled_box.view()
    }

    /// Borrow the handle to the view representing the widget.
    pub fn view_ref(&self) -> HViewRef<'_> {
        self.styled_box.view_ref()
    }

    /// Get the styling element representing the widget.
    pub fn style_elem(&self) -> theming::HElem {
        self.styled_box.style_elem()
    }

    /// Get the inner `EntryCore`.
    pub fn core(&self) -> &EntryCore {
        &self.core
    }

    /// Set the class set of the inner `StyledBox`.
    ///
    /// It defaults to `ClassSet::ENTRY`. Some bits (e.g., `ACTIVE`) are
    /// internally enforced and cannot be modified.
    pub fn set_class_set(&self, class_set: ClassSet) {
        self.styled_box.set_class_set(class_set);
    }

    /// Get the class set of the inner `StyledBox`.
    pub fn class_set(&self) -> ClassSet {
        self.styled_box.class_set()
    }
}

impl Widget for Entry {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view_ref()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

/// A widget implementing the core functionality of a text input field.
#[derive(Debug)]
pub struct EntryCore {
    view: HView,
    inner: Rc<Inner>,
}

#[derive(Debug)]
struct Inner {
    view: WeakHView,
    state: RefCell<State>,
    style_elem: theming::Elem,
    tictx_event_mask: Cell<pal::TextInputCtxEventFlags>,
}

#[derive(Debug)]
struct State {
    text: String,
    text_layout_info: Option<TextLayoutInfo>,
    canvas: CanvasMixin,
    tictx: Option<pal::HTextInputCtx>,
    sel_range: [usize; 2],
}

#[derive(Debug)]
struct TextLayoutInfo {
    text_layout: pal::TextLayout,
    layout_bounds: Box2<f32>,
}

impl EntryCore {
    pub fn new(style_manager: &'static theming::Manager) -> Self {
        let style_elem = theming::Elem::new(style_manager);

        let view = HView::new(
            ViewFlags::default()
                | ViewFlags::ACCEPT_MOUSE_OVER
                | ViewFlags::ACCEPT_MOUSE_DRAG
                | ViewFlags::TAB_STOP,
        );
        let weak_view = view.downgrade();

        let this = Self {
            view,
            inner: Rc::new(Inner {
                view: weak_view,
                state: RefCell::new(State {
                    text: String::new(),
                    text_layout_info: None,
                    canvas: CanvasMixin::new(),
                    tictx: None,
                    sel_range: [0; 2],
                }),
                style_elem,
                tictx_event_mask: Cell::new(pal::TextInputCtxEventFlags::empty()),
            }),
        };

        this.view.set_cursor_shape(Some(CursorShape::Text));

        // Get notified when a styling property changes
        let view = this.view.downgrade();
        let inner = Rc::downgrade(&this.inner);
        this.inner
            .style_elem
            .set_on_change(Box::new(move |_, kind_flags| {
                if let (Some(inner), Some(view)) = (inner.upgrade(), view.upgrade()) {
                    reapply_style(&inner, view.as_ref(), kind_flags);
                }
            }));

        this.view
            .set_layout(EmptyLayout::new(SizeTraits::default()));
        this.view
            .set_listener(EntryCoreListener::new(Rc::clone(&this.inner)));

        this
    }

    /// Get an owned handle to the view representing the widget.
    pub fn view(&self) -> HView {
        self.view.clone()
    }

    /// Borrow the handle to the view representing the widget.
    pub fn view_ref(&self) -> HViewRef<'_> {
        self.view.as_ref()
    }

    /// Get the styling element representing the widget.
    pub fn style_elem(&self) -> theming::HElem {
        self.inner.style_elem.helem()
    }
}

impl State {
    fn ensure_text_layout(&mut self, elem: &theming::Elem) {
        if self.text_layout_info.is_none() {
            let font_type = match elem.compute_prop(Prop::Font) {
                PropValue::SysFontType(value) => value,
                _ => unreachable!(),
            };

            let char_style = pal::CharStyle::new(pal::CharStyleAttrs {
                sys: Some(font_type),
                ..Default::default()
            });
            let text_layout = pal::TextLayout::from_text(&self.text, &char_style, None);

            let layout_bounds = text_layout.layout_bounds();

            self.text_layout_info = Some(TextLayoutInfo {
                text_layout,
                layout_bounds,
            });
        }
    }

    /// Delete the cached `TextLayout` (if any).
    ///
    /// After calling this, you probably want to call `HView::set_layout` again
    /// because the API contract of `Layout` requires immutability.
    fn invalidate_text_layout(&mut self) {
        self.text_layout_info = None;
    }
}

impl Widget for EntryCore {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view_ref()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

fn reapply_style(inner: &Rc<Inner>, view: HViewRef<'_>, kind_flags: PropKindFlags) {
    let mut state = inner.state.borrow_mut();

    if kind_flags.intersects(PropKindFlags::FG_COLOR) {
        state.canvas.pend_draw(view);
    }

    if kind_flags.intersects(PropKindFlags::FONT) {
        state.invalidate_text_layout();
        state.canvas.pend_draw(view);
    }
}

/// Implements `ViewListener` and `TextInputCtxListener`.
#[derive(Clone)]
struct EntryCoreListener {
    inner: Rc<Inner>,
}

impl EntryCoreListener {
    fn new(inner: Rc<Inner>) -> Self {
        Self { inner }
    }
}

impl ViewListener for EntryCoreListener {
    fn mount(&self, wm: pal::Wm, view: HViewRef<'_>, wnd: HWndRef<'_>) {
        self.inner.state.borrow_mut().canvas.mount(wm, view, wnd);

        // TODO: Does `new_text_input_ctx` get a document lock? This should be
        //       documented
        let tictx = wm.new_text_input_ctx(&wnd.pal_hwnd().unwrap(), Box::new(self.clone()));

        self.inner.state.borrow_mut().tictx = Some(tictx);
    }

    fn unmount(&self, wm: pal::Wm, view: HViewRef<'_>) {
        self.inner.state.borrow_mut().canvas.unmount(wm, view);

        let tictx = self.inner.state.borrow_mut().tictx.take();
        if let Some(tictx) = tictx {
            wm.remove_text_input_ctx(&tictx);
        }
    }

    fn focus_enter(&self, wm: pal::Wm, _: HViewRef<'_>) {
        let tictx = self.inner.state.borrow().tictx.clone();
        if let Some(tictx) = tictx {
            wm.text_input_ctx_set_active(&tictx, true);
        }
    }

    fn focus_leave(&self, wm: pal::Wm, _: HViewRef<'_>) {
        let tictx = self.inner.state.borrow().tictx.clone();
        if let Some(tictx) = tictx {
            wm.text_input_ctx_set_active(&tictx, false);
        }
    }

    fn position(&self, wm: pal::Wm, view: HViewRef<'_>) {
        self.inner.state.borrow_mut().canvas.position(wm, view);
    }

    fn update(&self, wm: pal::Wm, view: HViewRef<'_>, ctx: &mut UpdateCtx<'_>) {
        let mut state = self.inner.state.borrow_mut();
        let state = &mut *state; // enable split borrow

        state.ensure_text_layout(&self.inner.style_elem);

        let color = match self.inner.style_elem.compute_prop(Prop::FgColor) {
            PropValue::Rgbaf32(value) => value,
            _ => unreachable!(),
        };

        let text_layout_info: &TextLayoutInfo = state.text_layout_info.as_ref().unwrap();

        let visual_bounds = Box2::with_size(Point2::new(0.0, 0.0), view.frame().size());

        state
            .canvas
            .update_layer(wm, view, ctx.hwnd(), visual_bounds, |draw_ctx| {
                let c = &mut draw_ctx.canvas;

                c.draw_text(&text_layout_info.text_layout, Point2::new(0.0, 0.0), color);
            });

        // TODO: Display selection and caret

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![state.canvas.layer().unwrap().clone()]);
        }
    }
}

impl pal::iface::TextInputCtxListener<pal::Wm> for EntryCoreListener {
    fn edit(
        &self,
        _: pal::Wm,
        _: &pal::HTextInputCtx,
        _mutating: bool,
    ) -> Box<dyn pal::iface::TextInputCtxEdit<pal::Wm> + '_> {
        Box::new(Edit {
            state: self.inner.state.borrow_mut(),
            view: self.inner.view.upgrade().unwrap(),
        })
    }

    fn set_event_mask(
        &self,
        _: pal::Wm,
        _: &pal::HTextInputCtx,
        flags: pal::TextInputCtxEventFlags,
    ) {
        self.inner.tictx_event_mask.set(flags);
    }
}

/// Implements `TextInputCtxEdit`.
struct Edit<'a> {
    state: RefMut<'a, State>,
    view: HView,
}

impl Edit<'_> {
    fn check_range(&self, range: &Range<usize>) {
        let len = self.state.text.len();
        debug_assert!(
            range.start <= len && range.end <= len,
            "{:?} is out of range (len=({:?})",
            range,
            len
        );
    }
}

impl pal::iface::TextInputCtxEdit<pal::Wm> for Edit<'_> {
    fn selected_range(&mut self) -> Range<usize> {
        let [i1, i2] = self.state.sel_range;
        i1..i2
    }

    fn set_selected_range(&mut self, range: Range<usize>) {
        self.check_range(&range);

        self.state.sel_range = [range.start, range.end];
        self.view.pend_update();
    }

    fn set_composition_range(&mut self, range: Option<Range<usize>>) {
        range.as_ref().map(|r| self.check_range(r));

        // TODO
        log::warn!("set_composition_range({:?}): stub!", range);
    }

    fn replace(&mut self, range: Range<usize>, text: &str) {
        self.check_range(&range);

        for i in self.state.sel_range.iter_mut() {
            if *i >= range.end {
                *i = *i - range.len() + text.len();
            } else if *i >= range.start {
                *i = range.start;
            }
        }

        self.state.text.replace_range(range, text);

        self.state.invalidate_text_layout();
        self.state.canvas.pend_draw(self.view.as_ref());
    }

    fn slice(&mut self, range: Range<usize>) -> String {
        self.check_range(&range);

        self.state.text[range].to_owned()
    }

    fn floor_index(&mut self, mut i: usize) -> usize {
        let text = &self.state.text[..];
        while i < text.len() && (text.as_bytes()[i] & 0xc0) == 0x80 {
            i -= 1;
        }
        i
    }

    fn ceil_index(&mut self, mut i: usize) -> usize {
        let text = &self.state.text[..];
        while i < text.len() && (text.as_bytes()[i] & 0xc0) == 0x80 {
            i += 1;
        }
        i
    }

    fn len(&mut self) -> usize {
        self.state.text.len()
    }

    fn index_from_point(
        &mut self,
        point: Point2<f32>,
        flags: pal::IndexFromPointFlags,
    ) -> Option<usize> {
        log::warn!("index_from_point{:?}: stub!", (point, flags));
        None
    }

    fn frame(&mut self) -> Box2<f32> {
        self.view.global_frame()
    }

    fn slice_bounds(&mut self, range: Range<usize>) -> (Box2<f32>, usize) {
        self.check_range(&range);

        // TODO
        log::warn!("slice_bounds({:?}): stub!", range);
        (self.frame(), range.end)
    }
}
