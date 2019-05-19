#import "TCWGestureHandlerView.h"
#import "TCWWindowController.h"

/**
 * The maximum button number handled. The button number is limited by
 * the number of bits in `pressedMouseButtons`.
 */
#define kMaxButtonNumber 63

@implementation TCWGestureHandlerView {
    TCWWindowController __weak *controller;

    TCWMouseDragListenerUserData mouseDragListener;
    BOOL hasMouseDragListener;
    uint64_t pressedMouseButtons;
}

- (id)initWithController:(TCWWindowController *)_controller {
    if (self = [self init]) {
        self->controller = _controller;
        self->hasMouseDragListener = NO;
        self->pressedMouseButtons = 0;

        self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    }
    return self;
}

- (BOOL)hasActiveGesture {
    return self->hasMouseDragListener;
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

    if (![self hasActiveGesture]) {
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

- (void)cancelGesture {
    self->controller = nil;

    if (self->hasMouseDragListener) {
        self->hasMouseDragListener = NO;
        tcw_mousedraglistener_cancel(self->mouseDragListener);
        tcw_mousedraglistener_release(self->mouseDragListener);
    }
}

@end
