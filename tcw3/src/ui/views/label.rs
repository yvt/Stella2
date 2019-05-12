use cggeom::{prelude::*, Box2};
use cgmath::{Point2, Vector2};
use std::{cell::RefCell, rc::Rc};

use crate::{
    pal,
    pal::prelude::*,
    ui::mixins::CanvasMixin,
    uicore::{HView, HWnd, Layout, LayoutCtx, SizeTraits, UpdateCtx, ViewFlags, ViewListener},
};

/// A widget for displaying a static text.
#[derive(Debug)]
pub struct Label {
    view: HView,
    state: Rc<RefCell<State>>,
}

#[derive(Debug)]
struct State {
    text: String,
    text_layout_info: Option<TextLayoutInfo>,
    canvas: CanvasMixin,
}

#[derive(Debug)]
struct TextLayoutInfo {
    text_layout: pal::TextLayout,
    layout_bounds: Box2<f32>,
    visual_bounds: Box2<f32>,
}

impl Label {
    pub fn new() -> Self {
        let this = Self {
            view: HView::new(ViewFlags::empty()),
            state: Rc::new(RefCell::new(State {
                text: String::new(),
                text_layout_info: None,
                canvas: CanvasMixin::new(),
            })),
        };

        this.view
            .set_layout(Box::new(LabelListener::new(Rc::clone(&this.state))));
        this.view
            .set_listener(Box::new(LabelListener::new(Rc::clone(&this.state))));

        this
    }

    /// Get the view representing a label widget.
    pub fn view(&self) -> &HView {
        &self.view
    }

    /// Set the text displayed in a label widget.
    pub fn set_text(&mut self, value: impl Into<String>) {
        self.set_text_core(value.into());
    }

    fn set_text_core(&mut self, value: String) {
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
            .set_layout(Box::new(LabelListener::new(Rc::clone(&self.state))));
    }
}

impl State {
    fn ensure_text_layout(&mut self) {
        if self.text_layout_info.is_none() {
            let char_style = pal::CharStyle::new(Default::default());
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

        let text_layout_info: &TextLayoutInfo = state.text_layout_info.as_ref().unwrap();

        state.canvas.update_layer(
            wm,
            view,
            ctx.hwnd(),
            text_layout_info.visual_bounds,
            |draw_ctx| {
                let c = &mut draw_ctx.canvas;

                // TODO: Get text color from a system theme or somewhere else
                c.draw_text(
                    &text_layout_info.text_layout,
                    Point2::new(0.0, 0.0),
                    pal::RGBAF32::new(0.0, 0.0, 0.0, 1.0),
                );
            },
        );

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![state.canvas.layer().unwrap().clone()]);
        }
    }
}
