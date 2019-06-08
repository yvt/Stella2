use cggeom::{prelude::*, Box2};
use cgmath::{Point2, Vector2};
use momo::momo;
use std::{cell::RefCell, rc::Rc};

use crate::{
    pal,
    pal::prelude::*,
    ui::mixins::CanvasMixin,
    ui::theming::{
        manager::{Elem, PropKindFlags},
        ClassSet, ElemClassPath, Manager, Prop, PropValue,
    },
    uicore::{HView, HWnd, Layout, LayoutCtx, SizeTraits, Sub, UpdateCtx, ViewFlags, ViewListener},
};

/// A widget for displaying a static text.
#[derive(Debug)]
pub struct Label {
    view: HView,
    state: Rc<RefCell<State>>,
}

#[derive(Debug)]
struct State {
    style_manager: &'static Manager,
    class_path: ElemClassPath,
    dirty_class_path: bool,
    sheet_set_change_sub: Option<Sub>,

    text: String,
    text_layout_info: Option<TextLayoutInfo>,
    canvas: CanvasMixin,
    style_elem: Elem,
}

#[derive(Debug)]
struct TextLayoutInfo {
    text_layout: pal::TextLayout,
    layout_bounds: Box2<f32>,
    visual_bounds: Box2<f32>,
}

impl Label {
    pub fn new(style_manager: &'static Manager) -> Self {
        let this = Self {
            view: HView::new(ViewFlags::default()),
            state: Rc::new(RefCell::new(State {
                style_manager,
                class_path: ElemClassPath {
                    tail: None,
                    class_set: ClassSet::LABEL,
                },
                dirty_class_path: false,
                sheet_set_change_sub: None,
                text: String::new(),
                text_layout_info: None,
                canvas: CanvasMixin::new(),
                style_elem: Elem::new(),
            })),
        };

        let view_weak = this.view().downgrade();
        let state_rc = Rc::clone(&this.state);
        {
            let mut state = this.state.borrow_mut();
            let state = &mut *state;

            let sheet_set = state.style_manager.sheet_set();

            state
                .style_elem
                .set_class_path(&sheet_set, &state.class_path);

            // Get notified when the sheet set changes
            let sub = state
                .style_manager
                .subscribe_sheet_set_changed(Box::new(move |_, _| {
                    if let Some(view) = view_weak.upgrade() {
                        reapply_style(&state_rc, &view, true);
                    }
                }));
            state.sheet_set_change_sub = Some(sub);
        }

        this.view
            .set_layout(LabelListener::new(Rc::clone(&this.state)));
        this.view
            .set_listener(LabelListener::new(Rc::clone(&this.state)));

        this
    }

    /// Get the view representing a label widget.
    pub fn view(&self) -> &HView {
        &self.view
    }

    /// Get the view representing a label widget, consuming `self`.
    pub fn into_view(self) -> HView {
        self.view
    }

    /// Set the text displayed in a label widget.
    #[momo]
    pub fn set_text(&mut self, value: impl Into<String>) {
        let value = value.into();
        {
            let mut state = self.state.borrow_mut();
            if state.text == value {
                return;
            }
            state.text = value;
            state.invalidate_text_layout();
            state.canvas.pend_draw(&self.view);
        }
        self.view
            .set_layout(LabelListener::new(Rc::clone(&self.state)));
    }

    /// Call `set_text`, retuning `self`.
    ///
    /// This method is useful for constructing `Label` using the builder
    /// pattern.
    pub fn with_text(mut self, value: impl Into<String>) -> Self {
        self.set_text(value);
        self
    }

    /// Set the parent class path.
    pub fn set_parent_class_path(&mut self, parent_class_path: Option<Rc<ElemClassPath>>) {
        let mut state = self.state.borrow_mut();
        state.class_path.tail = parent_class_path;
        state.dirty_class_path = true;
        drop(state);

        reapply_style(&self.state, &self.view, false);
    }
}

fn reapply_style(state_rc: &Rc<RefCell<State>>, view: &HView, sheet_set_changed: bool) {
    let mut state = state_rc.borrow_mut();
    let state = &mut *state; // enable split borrow
    let style_elem = &mut state.style_elem;

    let sheet_set = state.style_manager.sheet_set();

    // Recalculate the active rule set
    let kind_flags;
    if sheet_set_changed {
        // The stylesheet set has changed, so do a full update
        style_elem.set_class_path(&sheet_set, &state.class_path);
        kind_flags = PropKindFlags::all();
    } else if state.dirty_class_path {
        // The class path has changed but the stylesheet set didn't change.
        kind_flags = style_elem.set_and_diff_class_path(&sheet_set, &state.class_path);
    } else {
        kind_flags = PropKindFlags::empty();
    }

    state.dirty_class_path = false;

    if kind_flags.intersects(PropKindFlags::FG_COLOR) {
        state.canvas.pend_draw(view);
    }

    if kind_flags.intersects(PropKindFlags::FONT) {
        state.invalidate_text_layout();
        state.canvas.pend_draw(view);
        view.set_layout(LabelListener::new(Rc::clone(state_rc)));
    }
}

impl State {
    fn ensure_text_layout(&mut self) {
        if self.text_layout_info.is_none() {
            let sheet_set = self.style_manager.sheet_set();
            let font_type = match self.style_elem.compute_prop(&sheet_set, Prop::Font) {
                PropValue::SysFontType(value) => value,
                _ => unreachable!(),
            };

            let char_style = pal::CharStyle::new(pal::CharStyleAttrs {
                sys: Some(font_type),
                ..Default::default()
            });
            let text_layout = pal::TextLayout::from_text(&self.text, &char_style, None);

            let visual_bounds = text_layout.visual_bounds();
            let layout_bounds = text_layout.layout_bounds();

            self.text_layout_info = Some(TextLayoutInfo {
                text_layout,
                visual_bounds,
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

impl Drop for State {
    fn drop(&mut self) {
        if let Some(sub) = self.sheet_set_change_sub.take() {
            sub.unsubscribe().unwrap();
        }
    }
}

/// Implements both of `Layout` and `ViewListener`.
struct LabelListener {
    state: Rc<RefCell<State>>,
}

impl LabelListener {
    fn new(state: Rc<RefCell<State>>) -> Self {
        Self { state }
    }
}

impl Layout for LabelListener {
    fn subviews(&self) -> &[HView] {
        &[]
    }

    fn size_traits(&self, _: &LayoutCtx<'_>) -> SizeTraits {
        let mut state = self.state.borrow_mut();
        state.ensure_text_layout();

        let size = state
            .text_layout_info
            .as_ref()
            .unwrap()
            .layout_bounds
            .size();

        SizeTraits {
            min: size,
            max: size,
            preferred: size,
        }
    }

    fn arrange(&self, _: &mut LayoutCtx<'_>, _: Vector2<f32>) {
        // has no subviews to layout
    }

    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        // See if `other` has the same type
        as_any::Downcast::is::<Self>(other)
    }
}

impl ViewListener for LabelListener {
    fn mount(&self, wm: pal::WM, view: &HView, wnd: &HWnd) {
        self.state.borrow_mut().canvas.mount(wm, view, wnd);
    }

    fn unmount(&self, wm: pal::WM, view: &HView) {
        self.state.borrow_mut().canvas.unmount(wm, view);
    }

    fn position(&self, wm: pal::WM, view: &HView) {
        self.state.borrow_mut().canvas.position(wm, view);
    }

    fn update(&self, wm: pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        let mut state = self.state.borrow_mut();
        let state = &mut *state; // enable split borrow

        state.ensure_text_layout();

        let sheet_set = state.style_manager.sheet_set();
        let color = match state.style_elem.compute_prop(&sheet_set, Prop::FgColor) {
            PropValue::Rgbaf32(value) => value,
            _ => unreachable!(),
        };

        let text_layout_info: &TextLayoutInfo = state.text_layout_info.as_ref().unwrap();

        state.canvas.update_layer(
            wm,
            view,
            ctx.hwnd(),
            text_layout_info.visual_bounds,
            |draw_ctx| {
                let c = &mut draw_ctx.canvas;

                c.draw_text(&text_layout_info.text_layout, Point2::new(0.0, 0.0), color);
            },
        );

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![state.canvas.layer().unwrap().clone()]);
        }
    }
}
