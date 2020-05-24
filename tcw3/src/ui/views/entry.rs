use alt_fp::FloatOrd;
use array::{Array, Array2};
use arrayvec::ArrayVec;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{Matrix3, Point2, Vector2};
use flags_macro::flags;
use momo::momo;
use rc_borrow::RcBorrow;
use std::{
    cell::{Cell, RefCell, RefMut},
    fmt,
    ops::Range,
    rc::Rc,
};
use subscriber_list::SubscriberList;
use unicount::{str_ceil, str_floor, str_prev};

use crate::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::EmptyLayout,
        mixins::CanvasMixin,
        theming::{
            self, elem_id, roles, ClassSet, GetPropValue, HElem, Prop, PropKindFlags, Widget,
        },
    },
    uicore::{
        actions, ActionId, ActionStatus, CursorShape, HView, HViewRef, HWndRef, MouseDragListener,
        SizeTraits, Sub, UpdateCtx, ViewFlags, ViewListener, WeakHView, WmExt,
    },
};

mod history;
#[cfg(test)]
mod tests;

/// A text entry widget.
#[derive(Debug)]
pub struct Entry {
    styled_box: theming::StyledBox,
    core: EntryCore,
}

impl Entry {
    pub fn new(wm: pal::Wm, style_manager: &'static theming::Manager) -> Self {
        let core = EntryCore::new(wm, style_manager);

        let styled_box = theming::StyledBox::new(style_manager, ViewFlags::default());
        styled_box.set_class_set(ClassSet::ENTRY);
        styled_box.set_auto_class_set(ClassSet::HOVER | ClassSet::FOCUS);
        styled_box.set_subview(roles::GENERIC, Some(core.view()));
        styled_box.set_subelement(roles::GENERIC, Some(core.style_elem()));

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

    /// Get the text content.
    pub fn text(&self) -> String {
        self.core.text()
    }

    /// Set the text content.
    ///
    /// If the new value is different from the current one, it resets various
    /// internal states such as an undo history. Otherwise, it does nothing.
    pub fn set_text(&self, value: impl Into<String>) {
        self.core.set_text(value)
    }

    /// Add a function called after the text content is modified.
    ///
    /// See [`EntryCore::subscribe_changed`].
    pub fn subscribe_changed(&self, cb: Box<dyn Fn(pal::Wm)>) -> Sub {
        self.core.subscribe_changed(cb)
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
///
/// # Styling
///
///  - `style_elem` - `FgColor`, `Padding`
///  - `style_elem > #TEXT_SELECTION` - `BgColor`
///
#[derive(Debug)]
pub struct EntryCore {
    view: HView,
    inner: Rc<Inner>,
}

struct Inner {
    wm: pal::Wm,
    view: WeakHView,
    state: RefCell<State>,
    style_elem: theming::Elem,
    style_sel_elem: theming::Elem,
    tictx_event_mask: Cell<pal::TextInputCtxEventFlags>,

    /// The list of subscribers of the `change` event.
    change_handlers: RefCell<SubscriberList<Box<dyn Fn(pal::Wm)>>>,
    /// `true` means the calls to `change_handlers` are pended.
    pending_change_handler: Cell<bool>,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Inner")
            .field("wm", &self.wm)
            .field("view", &self.view)
            .field("state", &self.state)
            .field("style_elem", &self.style_elem)
            .field("style_sel_elem", &self.style_sel_elem)
            .field("tictx_event_mask", &self.tictx_event_mask)
            .field("pending_change_handler", &self.pending_change_handler)
            .finish()
    }
}

#[derive(Debug)]
struct State {
    text: String,
    text_layout_info: Option<TextLayoutInfo>,
    scroll: f32,
    canvas: CanvasMixin,
    tictx: Option<pal::HTextInputCtx>,
    sel_range: [usize; 2],
    comp_range: Option<[usize; 2]>,
    /// The cached caret location. Should be invalidated by assigning `None`
    /// whenever the selection range is updated.
    caret: Option<[pal::Beam; 2]>,
    caret_layers: Option<[pal::HLayer; 2]>,
    caret_blink: bool,
    caret_blink_timer: Option<pal::HInvoke>,
    history: history::History,
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
    pub fn new(wm: pal::Wm, style_manager: &'static theming::Manager) -> Self {
        let style_elem = theming::Elem::new(style_manager);
        let style_sel_elem = theming::Elem::new(style_manager);
        style_sel_elem.set_class_set(elem_id::TEXT_SELECTION);
        style_elem.insert_child(style_sel_elem.helem());

        let view = HView::new(
            ViewFlags::default()
                | ViewFlags::ACCEPT_MOUSE_OVER
                | ViewFlags::ACCEPT_MOUSE_DRAG
                | ViewFlags::TAB_STOP
                | ViewFlags::STRONG_FOCUS,
        );
        let weak_view = view.downgrade();

        let this = Self {
            view,
            inner: Rc::new(Inner {
                wm,
                view: weak_view,
                state: RefCell::new(State {
                    text: String::new(),
                    text_layout_info: None,
                    scroll: 0.0,
                    canvas: CanvasMixin::new(),
                    tictx: None,
                    sel_range: [0; 2],
                    comp_range: None,
                    caret: None,
                    caret_layers: None,
                    caret_blink: true,
                    caret_blink_timer: None,
                    history: history::History::new(),
                }),
                style_elem,
                style_sel_elem,
                tictx_event_mask: Cell::new(pal::TextInputCtxEventFlags::empty()),
                change_handlers: RefCell::new(SubscriberList::new()),
                pending_change_handler: Cell::new(false),
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

        let view = this.view.downgrade();
        let inner = Rc::downgrade(&this.inner);
        this.inner
            .style_sel_elem
            .set_on_change(Box::new(move |_, kind_flags| {
                if let (Some(inner), Some(view)) = (inner.upgrade(), view.upgrade()) {
                    reapply_style_sel(&inner, view.as_ref(), kind_flags);
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

    /// Get the text content.
    pub fn text(&self) -> String {
        self.inner.state.borrow().text.clone()
    }

    /// Set the text content.
    ///
    /// If the new value is different from the current one, it resets various
    /// internal states such as an undo history. Otherwise, it does nothing.
    #[momo]
    pub fn set_text(&self, value: impl Into<String>) {
        if self.inner.state.borrow().text == value {
            return;
        }

        let mut value = Some(value);
        update_state(
            self.view.as_ref(),
            RcBorrow::from(&self.inner),
            &mut |state| {
                state.text = value.take().unwrap();
                state.sel_range = [0, 0];
                state.history = history::History::new();

                UpdateStateFlags::ANY
            },
        );
    }

    /// Add a function called when the text content is modified.
    ///
    /// The function may be called spuriously, i.e., even when the text content
    /// is not actually modified.
    ///
    /// The function is called via `Wm::invoke`, thus allowed to modify
    /// view hierarchy and view attributes. However, it's not allowed to call
    /// `subscribe_changed` when one of the handlers is being called.
    pub fn subscribe_changed(&self, cb: Box<dyn Fn(pal::Wm)>) -> Sub {
        self.inner.change_handlers.borrow_mut().insert(cb).untype()
    }
}

impl State {
    fn ensure_text_layout(&mut self, elem: &theming::Elem) -> &mut TextLayoutInfo {
        if self.text_layout_info.is_none() {
            let font_type = elem.computed_values().font();

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

    fn pend_update_after_focus_event(&mut self, hview: HViewRef<'_>) {
        if self.sel_range[0] != self.sel_range[1] {
            // A ranged selection is rendered using the `CanvasMixin`, so we
            // have to set the redraw flag of `CanvasMixin` in addition to just
            // calling `pend_update` (which is implicitly called by `pend_draw`)
            self.canvas.pend_draw(hview);
        } else {
            hview.pend_update();
        }
    }

    /// Reset the timer used for making the caret blink. This method is also
    /// responsible for starting or stopping the timer as needed by inspecting
    /// the current state.
    ///
    /// `override_focus` overrides the result of `improper_subview_is_focused`
    /// used while deciding whether the timer should be running or not.
    fn reset_timer(
        &mut self,
        hview: HViewRef<'_>,
        inner: RcBorrow<'_, Inner>,
        override_focus: Option<bool>,
    ) {
        let wm = inner.wm;
        if let Some(hinv) = self.caret_blink_timer.take() {
            wm.cancel_invoke(&hinv);
        }

        let should_start_timer = override_focus
            .unwrap_or_else(|| hview.improper_subview_is_focused())
            && self.sel_range[0] == self.sel_range[1];

        if should_start_timer {
            self.caret_blink_timer = Some(State::schedule_timer(RcBorrow::upgrade(inner)));
        } else {
            log::trace!("Not scheduling a deferred invocation because the caret is invisible now");
        }
    }

    /// Schedule a deferred invocation which toggles `caret_blink` and get the
    /// handle representing the invocation.
    ///
    /// This is implemented as a free function to allow recursive calls.
    fn schedule_timer(inner: Rc<Inner>) -> pal::HInvoke {
        use std::time::Duration;

        log::trace!("Scheduling a deferred invocation for blinking the caret");

        // TODO: Retrieve the preferred period from the operating system
        inner.wm.invoke_after(
            Duration::from_millis(400)..Duration::from_millis(700),
            move |_| {
                if let Some(hview) = inner.view.upgrade() {
                    // Toggle the caret's visibility
                    let mut state = inner.state.borrow_mut();
                    state.caret_blink = !state.caret_blink;
                    hview.pend_update();

                    // Schedule the next invocation
                    state.caret_blink_timer = Some(Self::schedule_timer(Rc::clone(&inner)));
                }
            },
        )
    }

    fn scroll_cursor_into_view(&mut self, hview: HViewRef<'_>, elem: &theming::Elem) -> bool {
        let cursor_i = self.sel_range[1];
        let layout_info = self.ensure_text_layout(elem);
        let cursor_x = layout_info.text_layout.cursor_pos(cursor_i)[0].x;
        let [_, padding_right, _, padding_left] = elem.computed_values().padding();
        let text_width = layout_info.layout_bounds.max.x;
        let viewport_width = hview.frame().size().x - (padding_right + padding_left);

        let new_scroll = self
            .scroll
            .fmax(cursor_x - viewport_width)
            .fmin(cursor_x)
            .fmin((text_width - viewport_width).fmax(0.0));

        if new_scroll != self.scroll {
            self.scroll = new_scroll;
            true
        } else {
            false
        }
    }
}

impl TextLayoutInfo {
    fn text_origin(&self, view: HViewRef<'_>, scroll: f32, elem: &theming::Elem) -> Vector2<f32> {
        let baseline = self.text_layout.line_baseline(0);
        let height = view.frame().size().y;
        let [padding_top, _, padding_bottom, padding_left] = elem.computed_values().padding();
        [
            padding_left - scroll,
            (height + self.line_height + padding_top - padding_bottom) * 0.5 - baseline,
        ]
        .into()
    }

    fn text_origin_global(
        &self,
        view: HViewRef<'_>,
        scroll: f32,
        elem: &theming::Elem,
    ) -> Vector2<f32> {
        let global_loc: [f32; 2] = view.global_frame().min.into();
        self.text_origin(view, scroll, elem) + Vector2::from(global_loc)
    }

    fn cursor_index_from_global_point(
        &self,
        view: HViewRef<'_>,
        scroll: f32,
        elem: &theming::Elem,
        x: f32,
    ) -> usize {
        self.text_layout.cursor_index_from_point(
            [x - self.text_origin_global(view, scroll, elem).x, 0.0].into(),
        )
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

    if kind_flags.intersects(Prop::FgColor.kind_flags() | Prop::Padding.kind_flags()) {
        state.canvas.pend_draw(view);
    }

    if kind_flags.intersects(Prop::Font.kind_flags()) {
        state.invalidate_text_layout();
        state.canvas.pend_draw(view);
    }
}

fn reapply_style_sel(inner: &Rc<Inner>, view: HViewRef<'_>, kind_flags: PropKindFlags) {
    if kind_flags.intersects(Prop::BgColor.kind_flags()) {
        reapply_style(inner, view, PropKindFlags::FG_COLOR);
    }
}

/// Implements `ViewListener` and `TextInputCtxListener`.
#[derive(Clone)]
struct EntryCoreListener {
    inner: Rc<Inner>,
}

type MoveHandler = fn([usize; 2], &pal::TextLayout, &str) -> usize;

impl EntryCoreListener {
    fn new(inner: Rc<Inner>) -> Self {
        Self { inner }
    }

    fn handle_delete(
        &self,
        view: HViewRef<'_>,
        get_range: fn(usize, &pal::TextLayout, &str) -> usize,
    ) {
        update_state(view, RcBorrow::from(&self.inner), &mut |state| {
            state.ensure_text_layout(&self.inner.style_elem);
            let layout = &state.text_layout_info.as_ref().unwrap().text_layout;
            let [mut start, mut end] = state.sel_range;

            if start == end {
                log::trace!(
                    "... there's no selection text. Deriving the deletion \
                    range based on the cursor position ({:?})",
                    start,
                );

                // If nothing is selected, derive the deletion range using
                // the given function
                end = get_range(start, layout, &state.text);
            } else {
                log::trace!("... deleting the selection at {:?}", start..end);
            }

            log::trace!("... deletion range = {:?}", start..end);

            if start == end {
                // There's nothing to delete
                return UpdateStateFlags::empty();
            }

            if start > end {
                std::mem::swap(&mut start, &mut end);
            }

            // Record the change to the undo history
            {
                let mut tx = state.history.start_transaction();
                tx.replace_range(&mut state.history, &state.text, start..end, String::new());
                tx.finish(&mut state.history, &state.text);
            }

            // Update `text`
            state.text.replace_range(start..end, "");
            state.sel_range = [start, start];

            UpdateStateFlags::ANY
        });
    }

    fn handle_move(&self, view: HViewRef<'_>, selecting: bool, get_new_pos: MoveHandler) {
        update_state(view, RcBorrow::from(&self.inner), &mut |state| {
            log::trace!("... original sel_range = {:?}", state.sel_range);

            state.ensure_text_layout(&self.inner.style_elem);
            let layout = &state.text_layout_info.as_ref().unwrap().text_layout;

            if selecting {
                // Move `state.sel_range[1]`.
                state.sel_range[1] = get_new_pos([state.sel_range[1]; 2], layout, &state.text);
            } else {
                // Pass the current selection to `get_new_pos`. If the range is
                // empty, the behavior is obvious (just move it around). If the
                // range has a non-zero length, how to handle it is up to
                // `get_new_pos`.
                let [mut start, mut end] = state.sel_range;
                if start > end {
                    std::mem::swap(&mut start, &mut end);
                }

                state.sel_range = [get_new_pos([start, end], layout, &state.text); 2];
            }

            state.history.mark_logical_op_break();

            log::trace!("... new sel_range = {:?}", state.sel_range);
            UpdateStateFlags::SEL
        });
    }

    fn handle_undo(&self, view: HViewRef<'_>) {
        update_state(view, RcBorrow::from(&self.inner), &mut |state| {
            if let Some(edit) = state.history.undo() {
                log::debug!("Undoing {:?}", edit);

                // Revert `edit`
                debug_assert_eq!(state.text[edit.range_new()], edit.new[..]);
                state.text.replace_range(edit.range_new(), &edit.old);

                let sel_range = edit.range_old();
                state.sel_range = [sel_range.start, sel_range.end];

                UpdateStateFlags::ANY
            } else {
                UpdateStateFlags::empty()
            }
        });
    }

    fn handle_redo(&self, view: HViewRef<'_>) {
        update_state(view, RcBorrow::from(&self.inner), &mut |state| {
            if let Some(edit) = state.history.redo() {
                log::debug!("Redoing {:?}", edit);

                // Re-apply `edit`
                debug_assert_eq!(state.text[edit.range_old()], edit.old[..]);
                state.text.replace_range(edit.range_old(), &edit.new);

                let sel_range = edit.range_new();
                state.sel_range = [sel_range.start, sel_range.end];

                UpdateStateFlags::ANY
            } else {
                UpdateStateFlags::empty()
            }
        });
    }
}

impl ViewListener for EntryCoreListener {
    fn mount(&self, wm: pal::Wm, view: HViewRef<'_>, wnd: HWndRef<'_>) {
        let mut state = self.inner.state.borrow_mut();
        state.canvas.mount(wm, view, wnd);
        state.caret_layers = Some(Array::from_fn(|_| wm.new_layer(Default::default())));

        // `new_text_input_ctx` may get a document lock, so
        // unborrow `state` first
        drop(state);

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

        // Stop the caret-blinking timer by specifying
        // `override_focus = Some(false)`.
        state.reset_timer(view, RcBorrow::from(&self.inner), Some(false));

        drop(state);

        let tictx = self.inner.state.borrow_mut().tictx.take();
        if let Some(tictx) = tictx {
            wm.remove_text_input_ctx(&tictx);
        }
    }

    fn focus_enter(&self, wm: pal::Wm, hview: HViewRef<'_>) {
        let tictx = self.inner.state.borrow().tictx.clone();
        if let Some(tictx) = tictx {
            wm.text_input_ctx_set_active(&tictx, true);
        }

        let mut state = self.inner.state.borrow_mut();
        state.caret_blink = true;
        state.pend_update_after_focus_event(hview);

        // Start the caret-blinking timer if needed.
        // `hview.is_focused() returns `false` at this point, so `reset_timer`
        // would think the view is not focused yet. Override this behavior by
        // specifying `override_focus = Some(true)`.
        state.reset_timer(hview, RcBorrow::from(&self.inner), Some(true));

        // Introduce a breakpoint in history coalescing
        state.history.mark_logical_op_break();
    }

    fn focus_leave(&self, wm: pal::Wm, hview: HViewRef<'_>) {
        let tictx = self.inner.state.borrow().tictx.clone();
        if let Some(tictx) = tictx {
            wm.text_input_ctx_set_active(&tictx, false);
        }

        let mut state = self.inner.state.borrow_mut();
        state.pend_update_after_focus_event(hview);

        // Stop the caret-blinking timer.
        // `hview.is_focused() returns `true` at this point, so `reset_timer`
        // would think the view is still focused. Override this behavior by
        // specifying `override_focus = Some(false)`.
        state.reset_timer(hview, RcBorrow::from(&self.inner), Some(false));
    }

    fn validate_action(&self, _: pal::Wm, _: HViewRef<'_>, action: ActionId) -> ActionStatus {
        let mut status = ActionStatus::empty();
        match action {
            actions::SELECT_ALL
            | actions::SELECT_LINE
            | actions::SELECT_PARAGRAPH
            | actions::SELECT_WORD
            | actions::DELETE_BACKWARD
            | actions::DELETE_BACKWARD_DECOMPOSING
            | actions::DELETE_BACKWARD_WORD
            | actions::DELETE_FORWARD
            | actions::DELETE_FORWARD_WORD
            | actions::MOVE_BACKWARD
            | actions::MOVE_FORWARD
            | actions::MOVE_LEFT
            | actions::MOVE_RIGHT
            | actions::MOVE_BACKWARD_WORD
            | actions::MOVE_FORWARD_WORD
            | actions::MOVE_LEFT_WORD
            | actions::MOVE_RIGHT_WORD
            | actions::MOVE_START_OF_LINE
            | actions::MOVE_END_OF_LINE
            | actions::MOVE_LEFT_END_OF_LINE
            | actions::MOVE_RIGHT_END_OF_LINE
            | actions::MOVE_START_OF_PARAGRAPH
            | actions::MOVE_END_OF_PARAGRAPH
            | actions::MOVE_START_OF_DOCUMENT
            | actions::MOVE_END_OF_DOCUMENT
            | actions::MOVE_UP
            | actions::MOVE_DOWN
            | actions::MOVE_UP_PAGE
            | actions::MOVE_DOWN_PAGE
            | actions::MOVE_BACKWARD_SELECTING
            | actions::MOVE_FORWARD_SELECTING
            | actions::MOVE_LEFT_SELECTING
            | actions::MOVE_RIGHT_SELECTING
            | actions::MOVE_BACKWARD_WORD_SELECTING
            | actions::MOVE_FORWARD_WORD_SELECTING
            | actions::MOVE_LEFT_WORD_SELECTING
            | actions::MOVE_RIGHT_WORD_SELECTING
            | actions::MOVE_START_OF_LINE_SELECTING
            | actions::MOVE_END_OF_LINE_SELECTING
            | actions::MOVE_LEFT_END_OF_LINE_SELECTING
            | actions::MOVE_RIGHT_END_OF_LINE_SELECTING
            | actions::MOVE_START_OF_PARAGRAPH_SELECTING
            | actions::MOVE_END_OF_PARAGRAPH_SELECTING
            | actions::MOVE_START_OF_DOCUMENT_SELECTING
            | actions::MOVE_END_OF_DOCUMENT_SELECTING
            | actions::MOVE_UP_SELECTING
            | actions::MOVE_DOWN_SELECTING
            | actions::MOVE_UP_PAGE_SELECTING
            | actions::MOVE_DOWN_PAGE_SELECTING => {
                status |= ActionStatus::VALID | ActionStatus::ENABLED;
            }
            actions::COPY | actions::CUT => {
                let state = self.inner.state.borrow();
                if state.sel_range[0] != state.sel_range[1] {
                    status |= ActionStatus::ENABLED;
                }
                status |= ActionStatus::VALID;
            }
            actions::PASTE => {
                // TODO: Check if the clipboard contains a text
                status |= ActionStatus::VALID;
            }
            actions::UNDO => {
                if self.inner.state.borrow().history.can_undo() {
                    status |= ActionStatus::ENABLED;
                }
                status |= ActionStatus::VALID;
            }
            actions::REDO => {
                if self.inner.state.borrow().history.can_redo() {
                    status |= ActionStatus::ENABLED;
                }
                status |= ActionStatus::VALID;
            }
            _ => {}
        }
        status
    }

    fn perform_action(&self, _: pal::Wm, view: HViewRef<'_>, action: ActionId) {
        let move_backward: MoveHandler = |sel, layout, _| {
            if sel[0] == sel[1] {
                layout.next_char(sel[0], false)
            } else {
                sel[0]
            }
        };
        let move_forward: MoveHandler = |sel, layout, _| {
            if sel[0] == sel[1] {
                layout.next_char(sel[1], true)
            } else {
                sel[1]
            }
        };
        let move_forward_word: MoveHandler = |sel, layout, _| layout.next_word(sel[1], true);
        let move_backward_word: MoveHandler = |sel, layout, _| layout.next_word(sel[0], false);

        let move_start: MoveHandler = |_, _, _| 0;
        let move_end: MoveHandler = |_, _, text| text.len();

        // TODO: Use the primary writing direction
        let move_left = move_backward;
        let move_right = move_forward;
        let move_left_word = move_backward_word;
        let move_right_word = move_forward_word;
        let move_left_end = move_start;
        let move_right_end = move_end;

        match action {
            actions::SELECT_ALL | actions::SELECT_LINE | actions::SELECT_PARAGRAPH => {
                log::trace!("Handling a 'select all' command (SELECT_ALL, etc.)");
                update_state(view, RcBorrow::from(&self.inner), &mut |state| {
                    log::trace!("... original sel_range = {:?}", state.sel_range);
                    state.sel_range = [0, state.text.len()];
                    log::trace!("... new sel_range = {:?}", state.sel_range);
                    UpdateStateFlags::SEL
                });
            }
            actions::SELECT_WORD => {
                log::trace!("Handling SELECT_WORD");
                update_state(view, RcBorrow::from(&self.inner), &mut |state| {
                    state.ensure_text_layout(&self.inner.style_elem);
                    let layout = &state.text_layout_info.as_ref().unwrap().text_layout;
                    let [mut start, mut end] = state.sel_range;
                    log::trace!("... original sel_range = {:?}", state.sel_range);
                    if start > end {
                        std::mem::swap(&mut start, &mut end);
                    }

                    // Expand the selection to a word
                    let start = layout.next_word(layout.next_char(start, true), false);
                    let end = layout.next_word(layout.next_char(end, false), true);

                    state.sel_range = [start, end];
                    log::trace!("... new sel_range = {:?}", state.sel_range);
                    UpdateStateFlags::SEL
                });
            }
            actions::COPY => {
                log::warn!("TODO: Copy");
            }
            actions::CUT => {
                log::warn!("TODO: Cut");
            }
            actions::PASTE => {
                log::warn!("TODO: Paste");
            }
            actions::DELETE_BACKWARD => {
                log::trace!("Handling DELETE_BACKWARD");
                self.handle_delete(view, |i, layout, _| layout.next_char(i, false));
            }
            actions::DELETE_BACKWARD_DECOMPOSING => {
                log::trace!("Handling DELETE_BACKWARD_DECOMPOSING");
                self.handle_delete(view, |i, _, text| str_prev(text, i));
            }
            actions::DELETE_BACKWARD_WORD => {
                log::trace!("Handling DELETE_BACKWARD_WORD");
                self.handle_delete(view, |i, layout, _| layout.next_word(i, false));
            }
            actions::DELETE_FORWARD => {
                log::trace!("Handling DELETE_FORWARD");
                self.handle_delete(view, |i, layout, _| layout.next_char(i, true));
            }
            actions::DELETE_FORWARD_WORD => {
                log::trace!("Handling DELETE_FORWARD_WORD");
                self.handle_delete(view, |i, layout, _| layout.next_word(i, true));
            }

            actions::MOVE_BACKWARD => {
                log::trace!("Handling MOVE_BACKWARD");
                self.handle_move(view, false, move_backward);
            }
            actions::MOVE_BACKWARD_SELECTING => {
                log::trace!("Handling MOVE_BACKWARD_SELECTING");
                self.handle_move(view, true, move_backward);
            }
            actions::MOVE_FORWARD => {
                log::trace!("Handling MOVE_FORWARD");
                self.handle_move(view, false, move_forward);
            }
            actions::MOVE_FORWARD_SELECTING => {
                log::trace!("Handling MOVE_FORWARD_WORD_SELECTING");
                self.handle_move(view, true, move_forward);
            }
            actions::MOVE_LEFT => {
                log::trace!("Handling MOVE_LEFT");
                self.handle_move(view, false, move_left);
            }
            actions::MOVE_LEFT_SELECTING => {
                log::trace!("Handling MOVE_LEFT_WORD_SELECTING");
                self.handle_move(view, true, move_left);
            }
            actions::MOVE_RIGHT => {
                log::trace!("Handling MOVE_RIGHT");
                self.handle_move(view, false, move_right);
            }
            actions::MOVE_RIGHT_SELECTING => {
                log::trace!("Handling MOVE_RIGHT_WORD_SELECTING");
                self.handle_move(view, true, move_right);
            }

            actions::MOVE_BACKWARD_WORD => {
                log::trace!("Handling MOVE_BACKWARD_WORD");
                self.handle_move(view, false, move_backward_word);
            }
            actions::MOVE_BACKWARD_WORD_SELECTING => {
                log::trace!("Handling MOVE_BACKWARD_WORD_SELECTING");
                self.handle_move(view, true, move_backward_word);
            }
            actions::MOVE_FORWARD_WORD => {
                log::trace!("Handling MOVE_FORWARD_WORD");
                self.handle_move(view, false, move_forward_word);
            }
            actions::MOVE_FORWARD_WORD_SELECTING => {
                log::trace!("Handling MOVE_FORWARD_WORD_WORD_SELECTING");
                self.handle_move(view, true, move_forward_word);
            }
            actions::MOVE_LEFT_WORD => {
                log::trace!("Handling MOVE_LEFT_WORD");
                self.handle_move(view, false, move_left_word);
            }
            actions::MOVE_LEFT_WORD_SELECTING => {
                log::trace!("Handling MOVE_LEFT_WORD_WORD_SELECTING");
                self.handle_move(view, true, move_left_word);
            }
            actions::MOVE_RIGHT_WORD => {
                log::trace!("Handling MOVE_RIGHT_WORD");
                self.handle_move(view, false, move_right_word);
            }
            actions::MOVE_RIGHT_WORD_SELECTING => {
                log::trace!("Handling MOVE_RIGHT_WORD_WORD_SELECTING");
                self.handle_move(view, true, move_right_word);
            }

            actions::MOVE_UP
            | actions::MOVE_UP_PAGE
            | actions::MOVE_START_OF_LINE
            | actions::MOVE_START_OF_PARAGRAPH
            | actions::MOVE_START_OF_DOCUMENT => {
                log::trace!(
                    "Handling a 'move to start' command \
                    (MOVE_START_OF_LINE, etc.)"
                );
                self.handle_move(view, false, move_start);
            }
            actions::MOVE_UP_SELECTING
            | actions::MOVE_UP_PAGE_SELECTING
            | actions::MOVE_START_OF_LINE_SELECTING
            | actions::MOVE_START_OF_PARAGRAPH_SELECTING
            | actions::MOVE_START_OF_DOCUMENT_SELECTING => {
                log::trace!(
                    "Handling a 'move to start and modify selection' \
                    command (MOVE_START_OF_LINE_SELECTING, etc.)"
                );
                self.handle_move(view, true, move_start);
            }

            actions::MOVE_DOWN
            | actions::MOVE_DOWN_PAGE
            | actions::MOVE_END_OF_LINE
            | actions::MOVE_END_OF_PARAGRAPH
            | actions::MOVE_END_OF_DOCUMENT => {
                log::trace!(
                    "Handling a 'move to end' command \
                    (MOVE_END_OF_LINE, etc.)"
                );
                self.handle_move(view, false, move_end);
            }
            actions::MOVE_DOWN_SELECTING
            | actions::MOVE_DOWN_PAGE_SELECTING
            | actions::MOVE_END_OF_LINE_SELECTING
            | actions::MOVE_END_OF_PARAGRAPH_SELECTING
            | actions::MOVE_END_OF_DOCUMENT_SELECTING => {
                log::trace!(
                    "Handling a 'move to end and modify selection' \
                    command (MOVE_END_OF_LINE_SELECTING, etc.)"
                );
                self.handle_move(view, true, move_end);
            }

            actions::MOVE_LEFT_END_OF_LINE => {
                log::trace!("Handling MOVE_LEFT_END_OF_LINE");
                self.handle_move(view, false, move_left_end);
            }
            actions::MOVE_LEFT_END_OF_LINE_SELECTING => {
                log::trace!("Handling MOVE_LEFT_END_OF_LINE_SELECTING");
                self.handle_move(view, true, move_left_end);
            }

            actions::MOVE_RIGHT_END_OF_LINE => {
                log::trace!("Handling MOVE_RIGHT_END_OF_LINE");
                self.handle_move(view, false, move_right_end);
            }
            actions::MOVE_RIGHT_END_OF_LINE_SELECTING => {
                log::trace!("Handling MOVE_RIGHT_END_OF_LINE_SELECTING");
                self.handle_move(view, true, move_right_end);
            }

            actions::UNDO => {
                log::trace!("Handling UNDO");
                self.handle_undo(view);
            }
            actions::REDO => {
                log::trace!("Handling REDO");
                self.handle_redo(view);
            }

            unknown_action => {
                log::warn!("Unknown action: {}", unknown_action);
            }
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
        let mut state = self.inner.state.borrow_mut();
        state.canvas.position(wm, view);

        if state.scroll_cursor_into_view(view, &self.inner.style_elem) {
            state.canvas.pend_draw(view);
        }

        // Unborrow `state` before calling `text_input_ctx_on_layout_change`
        drop(state);

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

        let color = self.inner.style_elem.computed_values().fg_color();
        let sel_color = self.inner.style_sel_elem.computed_values().bg_color();

        let text_layout_info: &TextLayoutInfo = state.text_layout_info.as_ref().unwrap();
        let sel_range = &state.sel_range;
        let comp_range = &state.comp_range;
        let scroll = state.scroll;
        let text_origin = text_layout_info.text_origin(view, scroll, &self.inner.style_elem);
        let is_focused = view.improper_subview_is_focused();

        let visual_bounds = Box2::with_size(Point2::new(0.0, 0.0), view.frame().size());

        state
            .canvas
            .update_layer(wm, view, ctx.hwnd(), visual_bounds, |draw_ctx| {
                let c = &mut draw_ctx.canvas;

                let mut sel_range = sel_range.clone();
                let text_layout = &text_layout_info.text_layout;

                c.save();
                c.mult_transform(Matrix3::from_translation(text_origin));

                if is_focused && sel_range[0] != sel_range[1] {
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
                    c.set_fill_rgb(sel_color);
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

            // Hide the caret if it's out of view or `caret_blink == false`
            for i in 0..2 {
                if !state.caret_blink
                    || !(0.0..global_frame.size().x).contains(&(pos[i].x + text_origin.x))
                {
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

        let expected_num_layers = 1 + is_focused as usize * 2;

        if ctx.layers().len() != expected_num_layers {
            let mut layers = Vec::with_capacity(3);
            layers.push(state.canvas.layer().unwrap().clone());
            if is_focused {
                layers.push(caret_layers[0].clone());
                layers.push(caret_layers[1].clone());
            }
            ctx.set_layers(layers);
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
            inner: RcBorrow::from(&self.inner),
            history_tx: None,
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
    inner: RcBorrow<'a, Inner>,
    view: HView,
    history_tx: Option<history::HistoryTx>,
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

    /// Start a transaction of updating the undo history if it hasn't started
    /// yet.
    fn ensure_history_tx(&mut self) {
        if self.history_tx.is_none() {
            self.history_tx = Some(self.state.history.start_transaction());
        }
    }
}

impl Drop for Edit<'_> {
    fn drop(&mut self) {
        let state = &mut *self.state; // enable split borrow

        if let Some(history_tx) = self.history_tx.take() {
            history_tx.finish(&mut state.history, &state.text);

            // `text` might have changed, so raise `changed`
            // (False positives are positive because of `set_composition_range`)
            pend_raise_change(self.inner);
        }

        if self
            .state
            .scroll_cursor_into_view(self.view.as_ref(), &self.inner.style_elem)
        {
            self.state.canvas.pend_draw(self.view.as_ref());
        }
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

        // Update the timer's state
        self.state.reset_timer(self.view.as_ref(), self.inner, None);
        self.state.caret_blink = true;
    }

    fn set_composition_range(&mut self, range: Option<Range<usize>>) {
        range.as_ref().map(|r| self.check_range(r));

        let range = range.map(|r| [r.start, r.end]);
        if range == self.state.comp_range {
            return;
        }
        self.state.comp_range = range;
        self.state.canvas.pend_draw(self.view.as_ref());

        self.ensure_history_tx();
        self.history_tx
            .as_mut()
            .unwrap()
            .set_composition_active(range.is_some());
    }

    fn replace(&mut self, range: Range<usize>, text: &str) {
        self.check_range(&range);

        self.ensure_history_tx();

        let state = &mut *self.state; // enable split borrow

        // Record the change to the undo history
        self.history_tx.as_mut().unwrap().replace_range(
            &mut state.history,
            &state.text,
            range.clone(),
            text.to_owned(),
        );

        // Update the selection
        for i in state.sel_range.iter_mut() {
            if *i >= range.end {
                *i = *i - range.len() + text.len();
            } else if *i >= range.start {
                *i = range.start;
            }
        }

        // Update `text`
        state.text.replace_range(range, text);

        state.invalidate_text_layout();
        state.canvas.pend_draw(self.view.as_ref());

        // Reset the timer's phase
        state.reset_timer(self.view.as_ref(), self.inner, None);
        state.caret_blink = true;
    }

    fn slice(&mut self, range: Range<usize>) -> String {
        self.check_range(&range);

        self.state.text[range].to_owned()
    }

    fn floor_index(&mut self, i: usize) -> usize {
        str_floor(&self.state.text, i)
    }

    fn ceil_index(&mut self, i: usize) -> usize {
        str_ceil(&self.state.text, i)
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

        let scroll = self.state.scroll;
        let text_layout_info = self.state.ensure_text_layout(&self.inner.style_elem);
        let text_layout = &text_layout_info.text_layout;
        let text_origin =
            text_layout_info.text_origin(self.view.as_ref(), scroll, &self.inner.style_elem);

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

bitflags::bitflags! {
    struct UpdateStateFlags: u8 {
        /// The selection might have changed.
        const SEL = 1;
        const LAYOUT = 1 << 1;
        const ANY = 1 << 2;
    }
}

/// Update the text and/or selection using a given closure. This method mustn't
/// be used in an implementation of `TextInputCtxEdit` because it calls
/// `text_input_ctx_on_selection_change` and/or `text_input_ctx_reset`.
///
/// If the provided closure modifies the text, it is responsible for updating
/// the undo history accordingly.
fn update_state(
    hview: HViewRef<'_>,
    inner: RcBorrow<'_, Inner>,
    f: &mut dyn FnMut(&mut State) -> UpdateStateFlags,
) {
    let wm = inner.wm;
    let mut state = inner.state.borrow_mut();
    let old_sel_range = state.sel_range;

    // Call the given function
    let mut flags = f(&mut *state);

    // Clear `UpdateStateFlags::SEL` if the selection did not change.
    if old_sel_range == state.sel_range {
        flags.set(UpdateStateFlags::SEL, false);
    }

    // Return early if nothing has changed
    if flags.is_empty() {
        return;
    }

    let tictx = state.tictx.clone();

    if flags.contains(UpdateStateFlags::ANY) {
        state.invalidate_text_layout();
    }

    if flags.intersects(flags![UpdateStateFlags::{ANY | SEL}]) {
        let scroll_changed = state.scroll_cursor_into_view(hview, &inner.style_elem);
        if scroll_changed {
            flags |= UpdateStateFlags::LAYOUT;
        }
    }

    if flags.intersects(flags![UpdateStateFlags::{ANY | LAYOUT}])
        || (old_sel_range[0] != old_sel_range[1])
        || (state.sel_range[0] != state.sel_range[1])
    {
        // A ranged selection is rendered using the `CanvasMixin`, so we
        // have to set the redraw flag of `CanvasMixin` in addition to just
        // calling `pend_update` (which is implicitly called by `pend_draw`)
        state.canvas.pend_draw(hview);
    } else {
        hview.pend_update();
    }

    // Update the caret-blinking timer
    state.caret_blink = true;
    state.reset_timer(hview, inner, None);

    // Raise `changed`
    if flags.contains(UpdateStateFlags::ANY) {
        pend_raise_change(inner);
    }

    // Invalidate the remembered caret position
    state.caret = None;

    // Unborrow `state` before calling `text_input_ctx_on_selection_change`,
    // which might request a document lock
    drop(state);
    if let Some(tictx) = tictx {
        if flags.contains(UpdateStateFlags::ANY) {
            wm.text_input_ctx_reset(&tictx);
        } else {
            if flags.contains(UpdateStateFlags::LAYOUT) {
                wm.text_input_ctx_on_layout_change(&tictx);
            }
            if flags.contains(UpdateStateFlags::SEL) {
                wm.text_input_ctx_on_selection_change(&tictx);
            }
        }
    }
}

/// Pend calls to the `change` event handlers.
fn pend_raise_change(inner: RcBorrow<'_, Inner>) {
    if inner.pending_change_handler.get() {
        return;
    }

    // FIXME: Remove this extra `upgrade`-ing
    let inner_weak = Rc::downgrade(&RcBorrow::upgrade(inner));

    inner.wm.invoke_on_update(move |wm| {
        if let Some(inner) = inner_weak.upgrade() {
            inner.pending_change_handler.set(false);

            let handlers = inner.change_handlers.borrow();
            for handler in handlers.iter() {
                handler(wm);
            }
        }
    });
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

    fn update_selection(&self, mut f: impl FnMut(&mut State)) {
        update_state(
            self.view.as_ref(),
            RcBorrow::from(&self.inner),
            &mut move |state| {
                state.history.mark_logical_op_break();
                f(state);
                UpdateStateFlags::SEL
            },
        );
    }
}

impl MouseDragListener for EntryCoreDragListener {
    fn mouse_down(&self, _: pal::Wm, hview: HViewRef<'_>, loc: Point2<f32>, _button: u8) {
        self.update_selection(|state| {
            if let Some(text_layout_info) = &state.text_layout_info {
                let i = text_layout_info.cursor_index_from_global_point(
                    hview,
                    state.scroll,
                    &self.inner.style_elem,
                    loc.x,
                );
                state.sel_range = [i, i];
            }
        });
    }

    fn mouse_motion(&self, _: pal::Wm, hview: HViewRef<'_>, loc: Point2<f32>) {
        self.update_selection(|state| {
            if let Some(text_layout_info) = &state.text_layout_info {
                let i = text_layout_info.cursor_index_from_global_point(
                    hview,
                    state.scroll,
                    &self.inner.style_elem,
                    loc.x,
                );
                state.sel_range[1] = i;
            }
        });
    }

    fn cancel(&self, _: pal::Wm, _: HViewRef<'_>) {
        let orig_sel_range = &self.orig_sel_range;
        self.update_selection(|state| {
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
