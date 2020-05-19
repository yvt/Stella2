use bitflags::bitflags;
use cggeom::prelude::*;
use cgmath::Point2;
use flags_macro::flags;
use std::{cell::Cell, rc::Rc};

use crate::{
    pal,
    pal::Wm,
    uicore::{HViewRef, KeyEvent, MouseDragListener},
};

/// A view listener mix-in that allows the client to implement the behaviour of
/// a push button.
#[derive(Debug)]
pub struct ButtonMixin {
    inner: Rc<Inner>,
}

pub trait ButtonListener {
    /// The state of the push button was updated.
    fn update(&self, _: Wm, _: HViewRef<'_>) {}

    /// The push button was activated.
    ///
    /// This method is called from a view's event handler, the same restrictions
    /// of `ViewListener` apply. Consider using `Wm::invoke` to circumvent
    /// them.
    fn activate(&self, _: Wm, _: HViewRef<'_>) {}
}

#[derive(Debug)]
struct Inner {
    state: Cell<StateFlags>,
}

bitflags! {
    struct StateFlags: u8 {
        const DRAG = 1;
        const PRESS = 1 << 1;
        const KEY_PRESS = 1 << 2;
    }
}

impl StateFlags {
    fn is_pressed(self) -> bool {
        self.intersects(StateFlags::PRESS | StateFlags::KEY_PRESS)
    }
}

impl Default for ButtonMixin {
    fn default() -> Self {
        Self::new()
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

    /// Handles [`ViewListener::focus_leave`].
    ///
    /// [`ViewListener::focus_leave`]: crate::uicore::ViewListener::focus_leave
    pub fn focus_leave(
        &self,
        wm: Wm,
        view: HViewRef<'_>,
        listener: Box<dyn ButtonListener + 'static>,
    ) {
        // Cancel key press
        self.inner.set_state(
            &*listener,
            wm,
            view,
            self.inner.state.get() - StateFlags::KEY_PRESS,
        );
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

    pub fn key_down(
        &self,
        wm: Wm,
        view: HViewRef<'_>,
        e: &KeyEvent<'_>,
        listener: Box<dyn ButtonListener + 'static>,
    ) -> bool {
        if e.translate_accel(&ACCEL_TABLE) == Some(ACTION_PRESS) {
            self.inner.set_state(
                &*listener,
                wm,
                view,
                self.inner.state.get() | StateFlags::KEY_PRESS,
            );
            true
        } else {
            false
        }
    }

    pub fn key_up(
        &self,
        wm: Wm,
        view: HViewRef<'_>,
        e: &KeyEvent<'_>,
        listener: Box<dyn ButtonListener + 'static>,
    ) -> bool {
        if e.translate_accel(&ACCEL_TABLE) == Some(ACTION_PRESS) {
            if self.inner.state.get().contains(StateFlags::KEY_PRESS) {
                self.inner.set_state(
                    &*listener,
                    wm,
                    view,
                    self.inner.state.get() - StateFlags::KEY_PRESS,
                );
                listener.activate(wm, view);
                true
            } else {
                false
            }
        } else {
            false
        }
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
        self.inner.state.get().is_pressed()
    }
}

const ACTION_PRESS: pal::ActionId = 0;
static ACCEL_TABLE: pal::AccelTable =
    pal::accel_table![(ACTION_PRESS, windows(" "), macos(" "), gtk(" "))];

struct MouseDragListenerImpl {
    inner: Rc<Inner>,
    client_listener: Box<dyn ButtonListener + 'static>,
}

impl MouseDragListener for MouseDragListenerImpl {
    fn mouse_motion(&self, wm: Wm, view: HViewRef<'_>, loc: Point2<f32>) {
        let inner = &self.inner;
        let mut state = inner.state.get();
        if state.contains(StateFlags::DRAG) {
            // Display the button in the pressed state only if the mouse cursor
            // is inside
            state.set(StateFlags::PRESS, hit_test(view, loc));
            inner.set_state(&*self.client_listener, wm, view, state);
        }
    }

    fn mouse_down(&self, wm: Wm, view: HViewRef<'_>, _loc: Point2<f32>, button: u8) {
        if button != 0 {
            return;
        }

        let inner = &self.inner;
        inner.set_state(
            &*self.client_listener,
            wm,
            view,
            flags![StateFlags::{PRESS | DRAG}],
        );
    }

    fn mouse_up(&self, wm: Wm, view: HViewRef<'_>, loc: Point2<f32>, button: u8) {
        if button != 0 {
            return;
        }

        let inner = &self.inner;
        inner.set_state(&*self.client_listener, wm, view, StateFlags::empty());

        if hit_test(view, loc) {
            self.client_listener.activate(wm, view);
        }
    }

    fn cancel(&self, wm: Wm, view: HViewRef<'_>) {
        self.inner
            .set_state(&*self.client_listener, wm, view, StateFlags::empty());
    }
}

fn hit_test(view: HViewRef<'_>, loc: Point2<f32>) -> bool {
    view.global_frame().contains_point(&loc)
}

impl Inner {
    /// Update `state`. Call `ButtonListener::update` as necessary.
    fn set_state(
        &self,
        listener: &dyn ButtonListener,
        wm: Wm,
        view: HViewRef<'_>,
        new_flags: StateFlags,
    ) {
        let should_call_update = new_flags.is_pressed() != self.state.get().is_pressed();
        self.state.set(new_flags);
        if should_call_update {
            listener.update(wm, view);
        }
    }
}
