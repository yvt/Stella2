use cggeom::{prelude::*, Box2};
use cgmath::{Point2, Vector2};
use momo::momo;
use std::{cell::RefCell, rc::Rc};

use crate::{
    pal,
    pal::prelude::*,
    ui::mixins::CanvasMixin,
    ui::theming::{ClassSet, Elem, HElem, Manager, Prop, PropKindFlags, PropValue, Widget},
    uicore::{HView, HWnd, Layout, LayoutCtx, SizeTraits, UpdateCtx, ViewFlags, ViewListener},
};

/// A widget for displaying a static text.
#[derive(Debug)]
pub struct Label {
    view: HView,
    inner: Rc<Inner>,
}

#[derive(Debug)]
struct Inner {
    state: RefCell<State>,
    style_elem: Elem,
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
    pub fn new(style_manager: &'static Manager) -> Self {
        let style_elem = Elem::new(style_manager);
        style_elem.set_class_set(ClassSet::LABEL);

        let this = Self {
            view: HView::new(ViewFlags::default()),
            inner: Rc::new(Inner {
                state: RefCell::new(State {
                    text: String::new(),
                    text_layout_info: None,
                    canvas: CanvasMixin::new(),
                }),
                style_elem,
            }),
        };

        // Get notified when a styling property changes
        let view = this.view().downgrade();
        let inner = Rc::downgrade(&this.inner);
        this.inner
            .style_elem
            .set_on_change(Box::new(move |_, kind_flags| {
                if let (Some(inner), Some(view)) = (inner.upgrade(), view.upgrade()) {
                    reapply_style(&inner, &view, kind_flags);
                }
            }));

        this.view
            .set_layout(LabelListener::new(Rc::clone(&this.inner)));
        this.view
            .set_listener(LabelListener::new(Rc::clone(&this.inner)));

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

    /// Get the styling element representing a label widget.
    pub fn style_elem(&self) -> HElem {
        self.inner.style_elem.helem()
    }

    /// Set the text displayed in a label widget.
    #[momo]
    pub fn set_text(&self, value: impl Into<String>) {
        let value = value.into();
        {
            let mut state = self.inner.state.borrow_mut();
            if state.text == value {
                return;
            }
            state.text = value;
            state.invalidate_text_layout();
            state.canvas.pend_draw(&self.view);
        }

        // Invalidate the layout, since the label size might be changed
        self.view
            .set_layout(LabelListener::new(Rc::clone(&self.inner)));
    }

    /// Call `set_text`, retuning `self`.
    ///
    /// This method is useful for constructing `Label` using the builder
    /// pattern.
    pub fn with_text(self, value: impl Into<String>) -> Self {
        self.set_text(value);
        self
    }
}

impl Widget for Label {
    fn view(&self) -> &HView {
        self.view()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

fn reapply_style(inner: &Rc<Inner>, view: &HView, kind_flags: PropKindFlags) {
    let mut state = inner.state.borrow_mut();

    if kind_flags.intersects(PropKindFlags::FG_COLOR) {
        state.canvas.pend_draw(view);
    }

    if kind_flags.intersects(PropKindFlags::FONT) {
        state.invalidate_text_layout();
        state.canvas.pend_draw(view);
        view.set_layout(LabelListener::new(Rc::clone(inner)));
    }
}

impl State {
    fn ensure_text_layout(&mut self, elem: &Elem) {
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
    inner: Rc<Inner>,
}

impl LabelListener {
    fn new(inner: Rc<Inner>) -> Self {
        Self { inner }
    }
}

impl Layout for LabelListener {
    fn subviews(&self) -> &[HView] {
        &[]
    }

    fn size_traits(&self, _: &LayoutCtx<'_>) -> SizeTraits {
        let mut state = self.inner.state.borrow_mut();
        state.ensure_text_layout(&self.inner.style_elem);

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
    fn mount(&self, wm: pal::Wm, view: &HView, wnd: &HWnd) {
        self.inner.state.borrow_mut().canvas.mount(wm, view, wnd);
    }

    fn unmount(&self, wm: pal::Wm, view: &HView) {
        self.inner.state.borrow_mut().canvas.unmount(wm, view);
    }

    fn position(&self, wm: pal::Wm, view: &HView) {
        self.inner.state.borrow_mut().canvas.position(wm, view);
    }

    fn update(&self, wm: pal::Wm, view: &HView, ctx: &mut UpdateCtx<'_>) {
        let mut state = self.inner.state.borrow_mut();
        let state = &mut *state; // enable split borrow

        state.ensure_text_layout(&self.inner.style_elem);

        let color = match self.inner.style_elem.compute_prop(Prop::FgColor) {
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
