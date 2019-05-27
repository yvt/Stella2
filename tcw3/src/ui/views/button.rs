use cggeom::box2;
use cgmath::Point2;
use std::{cell::RefCell, fmt, rc::Rc};

use crate::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::FillLayout,
        mixins::{ButtonMixin, CanvasMixin},
        views::Label,
    },
    uicore::{HView, HWnd, UpdateCtx, ViewFlags, ViewListener},
};

/// A push button widget.
#[derive(Debug)]
pub struct Button {
    view: HView,
    inner: Rc<Inner>,
}

struct Inner {
    button_mixin: ButtonMixin,
    canvas_mixin: RefCell<CanvasMixin>,
    label: RefCell<Label>,
    activate_handler: RefCell<Box<dyn Fn(pal::WM)>>,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Inner")
            .field("button_mixin", &self.button_mixin)
            .field("canvas_mixin", &self.canvas_mixin)
            .field("label", &self.label)
            .field("activate_handler", &())
            .finish()
    }
}

impl Button {
    pub fn new() -> Self {
        let view = HView::new(ViewFlags::default() | ViewFlags::ACCEPT_MOUSE_DRAG);

        let label = Label::new();

        let margin = 4.0;
        view.set_layout(FillLayout::new(label.view().clone()).with_uniform_margin(margin));

        let inner = Rc::new(Inner {
            button_mixin: ButtonMixin::new(),
            canvas_mixin: RefCell::new(CanvasMixin::new()),
            label: RefCell::new(label),
            activate_handler: RefCell::new(Box::new(|_| {})),
        });

        view.set_listener(ButtonViewListener {
            inner: Rc::clone(&inner),
        });

        Self { view, inner }
    }

    /// Get the view representing a push button widget.
    pub fn view(&self) -> &HView {
        &self.view
    }

    /// Set the text displayed in a push button widget.
    pub fn set_caption(&mut self, value: impl Into<String>) {
        self.inner.label.borrow_mut().set_text(value);
    }

    /// Set the function called when a push button widget is activated.
    ///
    /// The function is called via `WM::invoke`, thus allowed to modify
    /// view hierarchy and view attributes. However, it's not allowed to call
    /// `set_on_activate` on the activated `Button`.
    pub fn set_on_activate(&mut self, cb: impl Fn(pal::WM) + 'static) {
        self.inner.activate_handler.replace(Box::new(cb));
    }
}

struct ButtonViewListener {
    inner: Rc<Inner>,
}

impl ViewListener for ButtonViewListener {
    fn mount(&self, wm: pal::WM, view: &HView, wnd: &HWnd) {
        self.inner.canvas_mixin.borrow_mut().mount(wm, view, wnd);
    }

    fn unmount(&self, wm: pal::WM, view: &HView) {
        self.inner.canvas_mixin.borrow_mut().unmount(wm, view);
    }

    fn position(&self, wm: pal::WM, view: &HView) {
        self.inner.canvas_mixin.borrow_mut().position(wm, view);
    }

    fn update(&self, wm: pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        const RADIUS: f32 = 4.0;
        let mut canvas_mixin = self.inner.canvas_mixin.borrow_mut();

        canvas_mixin.update_layer_border(wm, view, ctx.hwnd(), RADIUS, |draw_ctx| {
            let c = &mut draw_ctx.canvas;

            let is_pressed = self.inner.button_mixin.is_pressed();
            // TODO: Get bg color from system theme or somewhere else
            let bg_color = if is_pressed {
                pal::RGBAF32::new(0.2, 0.4, 0.9, 1.0)
            } else {
                pal::RGBAF32::new(0.7, 0.7, 0.7, 1.0)
            };
            c.set_fill_rgb(bg_color);

            c.rounded_rect(
                box2! { min: [-RADIUS, -RADIUS], max: [RADIUS, RADIUS] },
                [[RADIUS - 1.0; 2]; 4],
            );
            c.fill();
        });

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![canvas_mixin.layer().unwrap().clone()]);
        }
    }

    fn mouse_drag(
        &self,
        _: pal::WM,
        _: &HView,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn crate::uicore::MouseDragListener> {
        self.inner
            .button_mixin
            .mouse_drag(Box::new(ButtonMixinListener {
                inner: Rc::clone(&self.inner),
            }))
    }
}

struct ButtonMixinListener {
    inner: Rc<Inner>,
}

impl crate::ui::mixins::button::ButtonListener for ButtonMixinListener {
    fn update(&self, _: pal::WM, view: &HView) {
        self.inner.canvas_mixin.borrow_mut().pend_draw(view);
    }

    fn activate(&self, wm: pal::WM, _: &HView) {
        let inner = Rc::clone(&self.inner);
        wm.invoke(move |wm| {
            let handler = inner.activate_handler.borrow();
            handler(wm);
        });
    }
}
