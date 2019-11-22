#include <math.h>

#import "TCWGestureHandlerView.h"
#import "TCWWindowController.h"

/**
 * The maximum button number handled. The button number is limited by
 * the number of bits in `pressedMouseButtons`.
 */
#define kMaxButtonNumber 63

/** Must be a power of two */
#define kScrollEventHistoryLen 4

typedef struct _TCWScrollEvent {
    double timestamp;
    double deltaX;
    double deltaY;
} TCWScrollEvent;

@implementation TCWGestureHandlerView {
    TCWWindowController __weak *controller;

    TCWMouseDragListenerUserData mouseDragListener;
    BOOL hasMouseDragListener;
    uint64_t pressedMouseButtons;

    TCWScrollListenerUserData scrollListener;
    BOOL hasScrollListener;
    BOOL momentumPhaseActive;
    TCWScrollEvent scrollEventHistory[kScrollEventHistoryLen];
    size_t scrollEventHistoryIndex;
}

- (id)initWithController:(TCWWindowController *)_controller {
    if (self = [self init]) {
        self->controller = _controller;
        self->hasMouseDragListener = NO;
        self->pressedMouseButtons = 0;

        self->hasScrollListener = NO;
        self->scrollEventHistoryIndex = 0;
        for (size_t i = 0; i < kScrollEventHistoryLen; ++i) {
            self->scrollEventHistory[i].timestamp = -INFINITY;
        }

        self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    }
    return self;
}

// Implements `NSView`
- (BOOL)acceptsFirstMouse:(NSEvent *)event {
    (void)event;

    // Respond to an initial mouse-down event. I.e., do not just activate
    // the window but also dispatch the event to the view.
    return YES;
}

// Implements `NSResponder`
- (BOOL)acceptsFirstResponder {
    return YES;
}

// Implements `NSResponder`
- (void)mouseDown:(NSEvent *)event {
    if (!self->controller || event.buttonNumber > kMaxButtonNumber) {
        return;
    }

    NSPoint loc = [self->controller locationOfEvent:event];

    if (!hasMouseDragListener) {
        // Start a new gesture
        self->mouseDragListener =
            tcw_wndlistener_mouse_drag(self->controller.listenerUserData, loc,
                                       (uint8_t)event.buttonNumber);
        self->hasMouseDragListener = YES;
    }

    if (self->hasMouseDragListener) {
        self->pressedMouseButtons |= (uint64_t)1 << event.buttonNumber;
        tcw_mousedraglistener_mouse_down(self->mouseDragListener, loc,
                                         (uint8_t)event.buttonNumber);
    }
}

// Implements `NSResponder`
- (void)mouseDragged:(NSEvent *)event {
    if (!self->controller) {
        return;
    }

    if (self->hasMouseDragListener) {
        NSPoint loc = [self->controller locationOfEvent:event];

        tcw_mousedraglistener_mouse_motion(self->mouseDragListener, loc);
    }
}

// Implements `NSResponder`
- (void)mouseUp:(NSEvent *)event {
    if (!self->controller || event.buttonNumber > kMaxButtonNumber) {
        return;
    }

    NSPoint loc = [self->controller locationOfEvent:event];

    if (self->hasMouseDragListener) {
        self->pressedMouseButtons &= ~((uint64_t)1 << event.buttonNumber);
        tcw_mousedraglistener_mouse_up(self->mouseDragListener, loc,
                                       (uint8_t)event.buttonNumber);
    }

    if (self->hasMouseDragListener && self->pressedMouseButtons == 0) {
        self->hasMouseDragListener = NO;
        tcw_mousedraglistener_release(self->mouseDragListener);
    }
}

// Implements `NSResponder`
- (void)rightMouseDown:(NSEvent *)event {
    [self mouseDown:event];
}

// Implements `NSResponder`
- (void)rightMouseDragged:(NSEvent *)event {
    [self mouseDragged:event];
}

// Implements `NSResponder`
- (void)rightMouseUp:(NSEvent *)event {
    [self mouseUp:event];
}

// Implements `NSResponder`
- (void)otherMouseDown:(NSEvent *)event {
    [self mouseDown:event];
}

// Implements `NSResponder`
- (void)otherMouseDragged:(NSEvent *)event {
    [self mouseDragged:event];
}

// Implements `NSResponder`
- (void)otherMouseUp:(NSEvent *)event {
    [self mouseUp:event];
}

- (void)scrollWheel:(NSEvent *)event {
    NSPoint loc = [self->controller locationOfEvent:event];
    NSEventPhase phase = event.phase;
    NSEventPhase momentumPhase = event.momentumPhase;

    // The constants of `NSEventPhase` look like bit flags, but they are
    // actually used like normal enums? I'm not sure about their intention.

    // Although the documentation is not very specific about this, it seems
    // that the event phases change in the following order:
    //
    //  | phase    | momentumPhase |
    //  | -------- | ------------- |
    //  | MayBegin | None          |
    //  | Began    | None          |
    //  | Changed  | None          |
    //  | Ended    | None          |
    //  | None     | MayBegin      |
    //  | None     | Began         |
    //  | None     | Changed       |
    //  | None     | Ended         |
    //
    // Note that the system does not tell if the momentum phase is about to
    // start when claiming `NSEventPhaseEnded`.

    if (phase == NSEventPhaseNone && momentumPhase == NSEventPhaseNone) {
        [self timeoutMomentumPhaseWait];

        // Legacy mouse wheel event
        tcw_wndlistener_scroll_motion(self->controller.listenerUserData, loc,
                                      (uint8_t)event.hasPreciseScrollingDeltas,
                                      event.scrollingDeltaX,
                                      event.scrollingDeltaY);
        return;
    }

    if (phase == NSEventPhaseCancelled ||
        momentumPhase == NSEventPhaseCancelled) {
        if (self->hasScrollListener) {
            self->hasScrollListener = NO;
            tcw_scrolllistener_cancel(self->scrollListener);
            tcw_scrolllistener_release(self->scrollListener);
        }
        return;
    }

    if (momentumPhase == NSEventPhaseEnded) {
        if (self->hasScrollListener) {
            self->hasScrollListener = NO;
            tcw_scrolllistener_end(self->scrollListener);
            tcw_scrolllistener_release(self->scrollListener);
        }
        return;
    } else if (phase == NSEventPhaseEnded) {
        // The momentum phase may start soon, but we can't tell if it will
        // happen. Wait for a moment to see if it will happen.
        [self performSelector:@selector(timeoutMomentumPhaseWait)
                   withObject:nil
                   afterDelay:0.05];
        return;
    }

    if (phase == NSEventPhaseBegan && !hasScrollListener) {
        [self timeoutMomentumPhaseWait];

        // Start a new gesture
        self->scrollListener = tcw_wndlistener_scroll_gesture(
            self->controller.listenerUserData, loc);
        self->hasScrollListener = YES;
        self->momentumPhaseActive = NO;
    }

    if (!self->hasScrollListener) {
        return;
    }

    if (momentumPhase == NSEventPhaseBegan) {
        tcw_scrolllistener_start_momentum_phase(self->scrollListener);
        self->momentumPhaseActive = YES;
    }

    if (phase == NSEventPhaseChanged || momentumPhase == NSEventPhaseChanged) {
        double deltaX = event.scrollingDeltaX;
        double deltaY = event.scrollingDeltaY;

        // Estimate the velocity based on recent events
        size_t numEvents = 1;
        double timestamp = event.timestamp;
        for (; numEvents <= kScrollEventHistoryLen; ++numEvents) {
            // Note the wrap-around arithmetic
            size_t i = self->scrollEventHistoryIndex - numEvents;
            i %= kScrollEventHistoryLen;

            double t = self->scrollEventHistory[i].timestamp;

            if (timestamp > t + 0.05) {
                // Too early, probably a separate series of events
                break;
            }

            timestamp = t;
        }

        {
            TCWScrollEvent *record =
                &self->scrollEventHistory[self->scrollEventHistoryIndex %
                                          kScrollEventHistoryLen];
            record->timestamp = event.timestamp;
            record->deltaX = deltaX;
            record->deltaY = deltaY;
            self->scrollEventHistoryIndex += 1;
        }

        double velX = 0.0, velY = 0.0;

        // Needs at least two events to estimate the velocity
        if (numEvents >= 2) {
            //
            //      ───────────────→ time
            //   delta:   3   2   1     (each number represents the event
            //                           wherein the delta value is recorded)
            //       k:     3   2   1   (numEvents = 3)
            //              ↑       ↑
            //              │       └─ event.timestamp
            //              └───────── timestamp
            //
            // In this example, the delta values from the two events 1 and 2
            // should be summed and divided by the timing difference between the
            // events 1 and 3.
            for (size_t k = 1; k < numEvents; ++k) {
                // Note the wrap-around arithmetic
                size_t i = self->scrollEventHistoryIndex - k;
                i %= kScrollEventHistoryLen;

                velX += self->scrollEventHistory[i].deltaX;
                velY += self->scrollEventHistory[i].deltaY;
            }

            // Divide by the duration
            double duration = event.timestamp - timestamp;
            velX /= duration;
            velY /= duration;
        }

        // Clean non-finite numbers just in case
        if (!isfinite(velX)) {
            velX = 0.0;
        }
        if (!isfinite(velY)) {
            velY = 0.0;
        }

        tcw_scrolllistener_motion(self->scrollListener,
                                  (uint8_t)event.hasPreciseScrollingDeltas,
                                  deltaX, deltaY, velX, velY);
    }
}

/** Stop waiting for a momentum scroll phase to start. */
- (void)timeoutMomentumPhaseWait {
    if (self->hasScrollListener && !self->momentumPhaseActive) {
        self->hasScrollListener = NO;
        tcw_scrolllistener_end(self->scrollListener);
        tcw_scrolllistener_release(self->scrollListener);
    }
}

- (void)cancelGesture {
    self->controller = nil;

    if (self->hasMouseDragListener) {
        self->hasMouseDragListener = NO;
        tcw_mousedraglistener_cancel(self->mouseDragListener);
        tcw_mousedraglistener_release(self->mouseDragListener);
    }

    if (self->hasScrollListener) {
        self->hasScrollListener = NO;
        tcw_scrolllistener_cancel(self->scrollListener);
        tcw_scrolllistener_release(self->scrollListener);
    }
}

@end
