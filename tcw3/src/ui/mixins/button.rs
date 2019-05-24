use bitflags::bitflags;
use cggeom::prelude::*;
use cgmath::Point2;
use flags_macro::flags;
use std::{cell::Cell, rc::Rc};

use crate::{
    pal::WM,
    uicore::{HView, MouseDragListener},
};

/// A view listener mix-in that allows the client to implement the behaviour of
/// a push button.
#[derive(Debug)]
pub struct ButtonMixin {
    inner: Rc<Inner>,
}

pub trait ButtonListener {
    /// The state of the push button was updated.
    fn update(&self, _: WM, _: &HView) {}

    /// The push button was activated.
    ///
    /// This method is called from a view's event handler, the same restrictions
    /// of `ViewListener` apply. Consider using `WM::invoke` to circumvent
    /// them.
    fn activate(&self, _: WM, _: &HView) {}
}

#[derive(Debug)]
struct Inner {
    state: Cell<StateFlags>,
}

bitflags! {
    struct StateFlags: u8 {
        const DRAG = 1 << 0;
        const PRESS = 1 << 1;
    }
}

impl ButtonMixin {
    /// Construct a `ButtonMixin`.
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                state: Cell::new(StateFlags::empty()),
            }),
        }
    }

    /// Handles [`ViewListener::mouse_drag`].
    ///
    /// [`ViewListener::mouse_drag`]: crate::uicore::ViewListener::mouse_drag
    pub fn mouse_drag(
        &self,
        listener: Box<dyn ButtonListener + 'static>,
    ) -> Box<dyn MouseDragListener> {
        Box::new(MouseDragListenerImpl {
            inner: Rc::clone(&self.inner),
            client_listener: listener,
        })
    }

    /// Get a flag indicating if the push button is currently pressed.
    ///
    /// This method returns `true` if the push button is currently pressed down
    /// and the mouse cursor is within the view.
    ///
    /// The client should use this method to determine the apperance of the push
    /// button to be drawn. The client should listen to changes in this value by
    /// implementing [`ButtonListener::update`].
    pub fn is_pressed(&self) -> bool {
        self.inner.state.get().contains(StateFlags::PRESS)
    }
}

struct MouseDragListenerImpl {
    inner: Rc<Inner>,
    client_listener: Box<dyn ButtonListener + 'static>,
}

impl MouseDragListener for MouseDragListenerImpl {
    fn mouse_motion(&self, wm: WM, view: &HView, loc: Point2<f32>) {
        let inner = &self.inner;
        let mut state = inner.state.get();
        if state.contains(StateFlags::DRAG) {
            // Display the button in the pressed state only if the mouse cursor
            // is inside
            state.set(StateFlags::PRESS, hit_test(view, loc));
            inner.set_state(&self.client_listener, wm, view, state);
        }
    }

    fn mouse_down(&self, wm: WM, view: &HView, _loc: Point2<f32>, button: u8) {
        if button != 0 {
            return;
        }

        let inner = &self.inner;
        inner.set_state(
            &self.client_listener,
            wm,
            view,
            flags![StateFlags::{PRESS | DRAG}],
        );
    }

    fn mouse_up(&self, wm: WM, view: &HView, loc: Point2<f32>, button: u8) {
        if button != 0 {
            return;
        }

        let inner = &self.inner;
        inner.set_state(&self.client_listener, wm, view, StateFlags::empty());

        if hit_test(view, loc) {
            self.client_listener.activate(wm, view);
        }
    }

    fn cancel(&self, wm: WM, view: &HView) {
        self.inner
            .set_state(&self.client_listener, wm, view, StateFlags::empty());
    }
}

fn hit_test(view: &HView, loc: Point2<f32>) -> bool {
    view.global_frame().contains_point(&loc)
}

impl Inner {
    /// Update `state`. Call `ButtonListener::update` as necessary.
    fn set_state(
        &self,
        listener: &Box<dyn ButtonListener + 'static>,
        wm: WM,
        view: &HView,
        new_flags: StateFlags,
    ) {
        let should_call_update = (new_flags ^ self.state.get()).contains(StateFlags::PRESS);
        self.state.set(new_flags);
        if should_call_update {
            listener.update(wm, view);
        }
    }
}
