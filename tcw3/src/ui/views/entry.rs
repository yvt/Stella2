use array::{Array, Array2};
use arrayvec::ArrayVec;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{Matrix3, Point2, Vector2};
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
        CursorShape, HView, HViewRef, HWndRef, MouseDragListener, SizeTraits, UpdateCtx, ViewFlags,
        ViewListener, WeakHView,
    },
};

#[cfg(test)]
mod tests;

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
    comp_range: Option<[usize; 2]>,
    /// The cached caret location. Should be invalidated by assigning `None`
    /// whenever the selection range is updated.
    caret: Option<[pal::Beam; 2]>,
    caret_layers: Option<[pal::HLayer; 2]>,
}

#[derive(Debug)]
struct TextLayoutInfo {
    text_layout: pal::TextLayout,
    layout_bounds: Box2<f32>,

    line_height: f32,

    /// Cache of `text_layout.run_metrics_of_range()` Used to quickly respond to
    /// `slice_bounds`. Sorted by `runs[i].index.start`.
    runs: Vec<pal::RunMetrics>,
    /// Cache of `text_layout.line_vertical_bounds()` for the line containing
    /// `runs`.
    line_vertical_bounds: Range<f32>,
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
                    comp_range: None,
                    caret: None,
                    caret_layers: None,
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
    fn ensure_text_layout(&mut self, elem: &theming::Elem) -> &mut TextLayoutInfo {
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
                runs: Vec::new(),
                line_vertical_bounds: 0.0..0.0,
                line_height: char_style.size(),
            });
        }

        self.text_layout_info.as_mut().unwrap()
    }

    /// Delete the cached `TextLayout` (if any).
    fn invalidate_text_layout(&mut self) {
        self.text_layout_info = None;
        self.caret = None;
    }
}

impl TextLayoutInfo {
    fn text_origin(&self, view: HViewRef<'_>) -> Vector2<f32> {
        let baseline = self.text_layout.line_baseline(0);
        let height = view.frame().size().y;
        // TODO: Stop hard-coding the margin
        [3.0, (height + self.line_height) * 0.5 - baseline].into()
    }

    fn text_origin_global(&self, view: HViewRef<'_>) -> Vector2<f32> {
        let global_loc: [f32; 2] = view.global_frame().min.into();
        self.text_origin(view) + Vector2::from(global_loc)
    }

    fn cursor_index_from_global_point(&self, x: f32, view: HViewRef<'_>) -> usize {
        self.text_layout
            .cursor_index_from_point([x - self.text_origin_global(view).x, 0.0].into())
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
        let mut state = self.inner.state.borrow_mut();
        state.canvas.mount(wm, view, wnd);
        state.caret_layers = Some(Array::from_fn(|_| wm.new_layer(Default::default())));
        drop(state);

        // TODO: Does `new_text_input_ctx` get a document lock? This should be
        //       documented
        let tictx = wm.new_text_input_ctx(&wnd.pal_hwnd().unwrap(), Box::new(self.clone()));

        self.inner.state.borrow_mut().tictx = Some(tictx);
    }

    fn unmount(&self, wm: pal::Wm, view: HViewRef<'_>) {
        let mut state = self.inner.state.borrow_mut();
        state.canvas.unmount(wm, view);
        for layer in state.caret_layers.as_ref().unwrap() {
            wm.remove_layer(layer);
        }
        state.caret_layers = None;
        drop(state);

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

    fn mouse_drag(
        &self,
        _: pal::Wm,
        hview: HViewRef<'_>,
        _loc: Point2<f32>,
        button: u8,
    ) -> Box<dyn MouseDragListener> {
        if button == 0 {
            Box::new(EntryCoreDragListener::new(
                hview.cloned(),
                Rc::clone(&self.inner),
            ))
        } else {
            Box::new(())
        }
    }

    fn position(&self, wm: pal::Wm, view: HViewRef<'_>) {
        self.inner.state.borrow_mut().canvas.position(wm, view);

        if (self.inner.tictx_event_mask.get()).contains(pal::TextInputCtxEventFlags::LAYOUT_CHANGE)
        {
            let tictx = self.inner.state.borrow().tictx.clone();
            if let Some(tictx) = tictx {
                wm.text_input_ctx_on_layout_change(&tictx);
            }
        }
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
        let sel_range = &state.sel_range;
        let comp_range = &state.comp_range;
        let text_origin = text_layout_info.text_origin(view);

        let visual_bounds = Box2::with_size(Point2::new(0.0, 0.0), view.frame().size());

        state
            .canvas
            .update_layer(wm, view, ctx.hwnd(), visual_bounds, |draw_ctx| {
                let c = &mut draw_ctx.canvas;

                let mut sel_range = sel_range.clone();
                let text_layout = &text_layout_info.text_layout;

                c.save();
                c.mult_transform(Matrix3::from_translation(text_origin));

                if sel_range[0] != sel_range[1] {
                    if sel_range[1] < sel_range[0] {
                        sel_range.reverse();
                    }
                    // TODO: Make sure the text is really single-lined. Otherwise,
                    //       we might break the contract of `run_metrics_of_range`
                    //       when the range is in a different line
                    let line = 0;
                    let vert_bounds = text_layout.line_vertical_bounds(line);
                    let runs = text_layout.run_metrics_of_range(sel_range[0]..sel_range[1]);
                    log::trace!("sel_range = {:?}", sel_range[0]..sel_range[1]);
                    log::trace!("runs({:?}) = {:?}", sel_range[0]..sel_range[1], runs);

                    // Fill the selection
                    c.set_fill_rgb([0.3, 0.6, 1.0, 0.5].into()); // TODO: derive from stylesheet
                    for run in runs.iter() {
                        c.fill_rect(box2! {
                            min: [run.bounds.start, vert_bounds.start],
                            max: [run.bounds.end, vert_bounds.end],
                        });
                    }
                }

                c.draw_text(&text_layout, Point2::new(0.0, 0.0), color);

                if let Some(comp_range) = comp_range {
                    // Draw an underline below the preedit text
                    // TODO: The backend shouldn't give a zero-length composition range
                    if comp_range[1] > comp_range[0] {
                        // TODO: See the above TODO regarding `line`
                        let line = 0;
                        let y = text_layout.line_baseline(line);
                        let runs = text_layout.run_metrics_of_range(comp_range[0]..comp_range[1]);
                        log::trace!("comp_range = {:?}", comp_range[0]..comp_range[1]);
                        log::trace!("runs({:?}) = {:?}", comp_range[0]..comp_range[1], runs);

                        c.set_fill_rgb([color.r, color.g, color.b, color.a * 0.6].into());
                        for run in runs.iter() {
                            c.fill_rect(box2! {
                                min: [run.bounds.start, y + 1.0],
                                max: [run.bounds.end, y + 2.0],
                            });
                        }
                    }
                }

                c.restore();
            });

        // Display the caret
        // TODO: Make the caret blink
        let caret_layers = state.caret_layers.as_ref().unwrap();
        if sel_range[0] == sel_range[1] {
            // Calculate the location of the caret.
            let pos = state.caret.get_or_insert_with(|| {
                let pos = text_layout_info.text_layout.cursor_pos(sel_range[0]);
                log::trace!("cursor_pos({:?}) = {:?}", sel_range[0], pos);
                pos
            });

            let mut layer_attrs: ArrayVec<[_; 2]> = (0..2)
                .map(|_| pal::LayerAttrs {
                    opacity: Some(1.0),
                    bg_color: Some(color),
                    ..Default::default()
                })
                .collect();

            let global_frame = view.global_frame();
            let offset: [f32; 2] = global_frame.min.into();
            let mut offset: cgmath::Vector2<f32> = offset.into();
            offset += text_origin;

            let [mut rect0, mut rect1] = pos.map(|beam| beam.as_wide_box2(1.0).translate(offset));

            if pos[0].x != pos[1].x {
                // If there are a strong cursor and a weak cursor,
                // display the former in the upper half and the latter
                // in the lower half
                rect0.max.y = rect0.mid().y;
                rect1.min.y = rect0.max.y;
                layer_attrs[1].bounds = Some(rect1);
            } else {
                layer_attrs[1].opacity = Some(0.0);
            }
            layer_attrs[0].bounds = Some(rect0);

            // Hide the caret if it's out of view
            for i in 0..2 {
                if !(0.0..global_frame.size().x).contains(&pos[i].x) {
                    layer_attrs[i].opacity = Some(0.0);
                }
            }

            for (layer, attrs) in caret_layers.iter().zip(layer_attrs.drain(..)) {
                wm.set_layer_attr(layer, attrs);
            }
        } else {
            for layer in caret_layers.iter() {
                wm.set_layer_attr(
                    layer,
                    pal::LayerAttrs {
                        opacity: Some(0.0),
                        ..Default::default()
                    },
                );
            }
        }

        if ctx.layers().len() != 3 {
            ctx.set_layers(vec![
                state.canvas.layer().unwrap().clone(),
                caret_layers[0].clone(),
                caret_layers[1].clone(),
            ]);
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
            inner: &self.inner,
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
    inner: &'a Inner,
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

        let range = [range.start, range.end];
        if range == self.state.sel_range {
            return;
        }
        self.state.sel_range = range;
        self.state.canvas.pend_draw(self.view.as_ref());
        self.state.caret = None;
    }

    fn set_composition_range(&mut self, range: Option<Range<usize>>) {
        range.as_ref().map(|r| self.check_range(r));

        let range = range.map(|r| [r.start, r.end]);
        if range == self.state.comp_range {
            return;
        }
        self.state.comp_range = range;
        self.state.canvas.pend_draw(self.view.as_ref());
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

        let text_layout_info = self.state.ensure_text_layout(&self.inner.style_elem);
        let text_layout = &text_layout_info.text_layout;
        let text_origin = text_layout_info.text_origin(self.view.as_ref());

        let offset: [f32; 2] = self.view.global_frame().min.into();
        let mut offset: cgmath::Vector2<f32> = offset.into();
        offset += text_origin;

        // If `range.len() == 0`, return the caret position calculated by
        // `text_layout.cursor_pos`
        if range.len() == 0 {
            let strong_cursor = text_layout.cursor_pos(range.start)[0];
            return (strong_cursor.as_box2().translate(offset), range.start);
        }

        // Do we already have a run starting at `range.start`?
        let runs = &mut text_layout_info.runs;
        let line_vertical_bounds = &mut text_layout_info.line_vertical_bounds;
        let run_i = if let Ok(i) = runs.binary_search_by_key(&range.start, |r| r.index.start) {
            // `RunMetrics` doesn't have sufficient information for us to slice
            // them, so `runs[i].index` must be an improper subset of `range`.
            if runs[i].index.end <= range.end {
                Some(i)
            } else {
                None
            }
        } else {
            None
        };

        // If we don't have one, recalculate and cache `runs` (because the
        // backend may call `slice_bounds` repeatedly until all bounding boxes
        // for a given string range is known)
        let run_i: usize = run_i.unwrap_or_else(|| {
            // Find the line contianing `range.start`.
            // (Note: `EntryCore` is supposed to be a single-line input widget)
            let line = text_layout.line_from_index(range.start);
            let line_end = text_layout.line_index_range(line).end;

            *line_vertical_bounds = text_layout.line_vertical_bounds(line);

            // Recalculate `runs`
            *runs = text_layout.run_metrics_of_range(range.start..line_end.min(range.end));
            minisort::minisort_by_key(runs, |r| r.index.start);

            // Find the run starting at `range.start`. (This will always succeed
            // because of `run_metrics_of_range`'s postcondition)
            runs.binary_search_by_key(&range.start, |r| r.index.start)
                .unwrap()
        });

        // Return the found run
        let run = &runs[run_i];
        let bounds = box2! {
            min: [run.bounds.start, line_vertical_bounds.start],
            max: [run.bounds.end, line_vertical_bounds.end],
        };
        (bounds.translate(offset), run.index.end)
    }
}

struct EntryCoreDragListener {
    view: HView,
    inner: Rc<Inner>,
    orig_sel_range: [usize; 2],
}

impl EntryCoreDragListener {
    fn new(view: HView, inner: Rc<Inner>) -> Self {
        let orig_sel_range = inner.state.borrow().sel_range;
        Self {
            view,
            inner,
            orig_sel_range,
        }
    }

    fn update_selection(&self, wm: pal::Wm, f: &mut dyn FnMut(&mut State)) {
        let mut state = self.inner.state.borrow_mut();
        let old_sel_range = state.sel_range;

        // Call the given function
        f(&mut *state);

        // Return early if the selection did not change
        if old_sel_range == state.sel_range {
            return;
        }

        let tictx = state.tictx.clone();

        if (old_sel_range[0] != old_sel_range[1]) || (state.sel_range[0] != state.sel_range[1]) {
            // A ranged selection is rendered using the `CanvasMixin`, so we
            // have to set the redraw flag of `CanvasMixin` in addition to just
            // calling `pend_update` (which is implicitly called by `pend_draw`)
            state.canvas.pend_draw(self.view.as_ref());
        } else {
            self.view.pend_update();
        }

        state.caret = None;

        // Unborrow `state` before calling `text_input_ctx_on_selection_change`,
        // which might request a document lock
        drop(state);
        if let Some(tictx) = tictx {
            wm.text_input_ctx_on_selection_change(&tictx);
        }
    }
}

impl MouseDragListener for EntryCoreDragListener {
    fn mouse_down(&self, wm: pal::Wm, hview: HViewRef<'_>, loc: Point2<f32>, _button: u8) {
        self.update_selection(wm, &mut |state| {
            if let Some(text_layout_info) = &state.text_layout_info {
                let i = text_layout_info.cursor_index_from_global_point(loc.x, hview);
                state.sel_range = [i, i];
            }
        });
    }

    fn mouse_motion(&self, wm: pal::Wm, hview: HViewRef<'_>, loc: Point2<f32>) {
        self.update_selection(wm, &mut |state| {
            if let Some(text_layout_info) = &state.text_layout_info {
                let i = text_layout_info.cursor_index_from_global_point(loc.x, hview);
                state.sel_range[1] = i;
            }
        });
    }

    fn cancel(&self, wm: pal::Wm, _: HViewRef<'_>) {
        let orig_sel_range = &self.orig_sel_range;
        self.update_selection(wm, &mut |state| {
            // Before resetting the selection, make sure `orig_sel_range` is
            // still a valid selection range
            let start = orig_sel_range[0].min(orig_sel_range[0]);
            let end = orig_sel_range[0].max(orig_sel_range[0]);
            if state.text.get(start..end).is_some() {
                state.sel_range = *orig_sel_range;
            }
        });
    }
}
