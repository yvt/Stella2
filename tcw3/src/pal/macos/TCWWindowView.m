#import "TCWWindowView.h"
#import "TCWBridge.h"
#import "TCWWindowController.h"

@implementation TCWWindowView {
    NSTrackingArea *trackingArea;
    TCWWindowController __weak *controller;
}

- (id)initWithController:(TCWWindowController *)_controller {
    if (self = [self init]) {
        self->controller = _controller;
    }
    return self;
}

// Override `NSView`
- (BOOL)isFlipped {
    // Flip the window contents to match TCW3's coordinate space
    return YES;
}

// Overrides `NSView`
- (void)updateTrackingAreas {
    if (self->trackingArea) {
        [self removeTrackingArea:self->trackingArea];
    }

    self->trackingArea = [[NSTrackingArea alloc]
        initWithRect:self.frame
             options:(NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved |
                      NSTrackingActiveAlways)
               owner:self
            userInfo:nil];

    [self addTrackingArea:self->trackingArea];
}

// Implements `NSResponder`
- (void)mouseMoved:(NSEvent *)event {
    if (!self->controller) {
        return;
    }

    NSPoint loc = [self->controller locationOfEvent:event];

    tcw_wndlistener_mouse_motion(self->controller.listenerUserData, loc);
}

// Implements `NSResponder`
- (void)mouseExited:(NSEvent *)event {
    (void)event;

    if (!self->controller) {
        return;
    }

    tcw_wndlistener_mouse_leave(self->controller.listenerUserData);
}

@end
