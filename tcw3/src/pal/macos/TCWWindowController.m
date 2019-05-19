#import "TCWWindowController.h"
#import "TCWBridge.h"
#import "TCWGestureHandlerView.h"

@interface TCWWindowView : NSView
@end

@implementation TCWWindowView

// Override `NSView`
- (BOOL)isFlipped {
    // Flip the window contents to match TCW3's coordinate space
    return YES;
}

@end

@implementation TCWWindowController {
    NSWindow *window;

    TCWGestureHandlerView *inactiveGestureHandler;

    NSMutableArray<TCWGestureHandlerView *> *gestureHandlers;
}

- (id)init {
    if (self) {
        self = [super init];

        NSRect frame = NSMakeRect(0.0, 0.0, 800.0, 600.0);

        NSWindowStyleMask masks =
            NSWindowStyleMaskClosable | NSWindowStyleMaskMiniaturizable |
            NSWindowStyleMaskResizable | NSWindowStyleMaskTitled;

        self->window =
            [[NSWindow alloc] initWithContentRect:frame
                                        styleMask:masks
                                          backing:NSBackingStoreBuffered
                                            defer:NO];

        self->window.releasedWhenClosed = NO;
        self->window.acceptsMouseMovedEvents = YES;
        self->window.delegate = (id<NSWindowDelegate>)self;

        self->window.contentView = [TCWWindowView new];
        self->window.contentView.wantsLayer = YES;

        // Create the first gesture handler view
        self->inactiveGestureHandler = [self newGestureHandlerView];
        self->gestureHandlers = [NSMutableArray new];
        [self->window.contentView addSubview:self->inactiveGestureHandler];
    }
    return self;
}

- (void)close {
    [self->window close];
}

- (void)setTitle:(NSString *)windowTitle {
    [self->window setTitle:windowTitle];
}

- (void)setContentSize:(NSSize)size {
    [self->window setContentSize:size];
}

- (void)setContentMaxSize:(NSSize)size {
    [self->window setContentMaxSize:size];
}

- (void)setContentMinSize:(NSSize)size {
    [self->window setContentMinSize:size];
}

- (NSSize)contentSize {
    return self->window.contentView.frame.size;
}

- (void)setFlags:(uint32_t)flags {
    // Compute the new masks
    NSWindowStyleMask masks = 0;

    if (flags & kTCW3WndFlagsResizable) {
        masks |= NSWindowStyleMaskResizable;
    }

    if (flags & kTCW3WndFlagsBorderless) {
        masks |= NSWindowStyleMaskBorderless;
    } else {
        masks |= NSWindowStyleMaskClosable | NSWindowStyleMaskMiniaturizable |
                 NSWindowStyleMaskTitled;
    }

    self->window.styleMask = masks;
}

- (void)makeKeyAndOrderFront {
    [self->window makeKeyAndOrderFront:nil];
}

- (void)orderOut {
    [self->window orderOut:nil];
}

- (void)center {
    [self->window center];
}

- (void)setLayer:(CALayer *)layer {
    self->window.contentView.layer.sublayers = @[ layer ];
}

- (float)dpiScale {
    return (float)self->window.backingScaleFactor;
}

// Implements `NSWindowDelegate`
- (BOOL)windowShouldClose:(NSWindow *)sender {
    (void)sender;
    return tcw_wndlistener_should_close(self.listenerUserData);
}

// Implements `NSWindowDelegate`
- (void)windowWillClose:(NSNotification *)notification {
    (void)notification;
    self->window.delegate = nil;

    // Cancel all input gestures
    for (TCWGestureHandlerView *view in self->gestureHandlers) {
        [view cancelGesture];
    }

    tcw_wndlistener_close(self.listenerUserData);
}

// Implements `NSWindowDelegate`
- (void)windowDidResize:(NSNotification *)notification {
    (void)notification;
    tcw_wndlistener_resize(self.listenerUserData);
}

// Implements `NSWindowDelegate`
- (void)windowDidChangeBackingProperties:(NSNotification *)notification {
    (void)notification;
    tcw_wndlistener_dpi_scale_changed(self.listenerUserData);
}

/**
 * Create a new `TCWGestureHandlerView` and add it to the window.
 */
- (TCWGestureHandlerView *)newGestureHandlerView {
    TCWGestureHandlerView *view =
        [[TCWGestureHandlerView alloc] initWithController:self];

    view.frame = self->window.contentView.frame;

    [self->window.contentView addSubview:view];

    return view;
}

- (void)gestureStartedInView:(TCWGestureHandlerView *)view {
    if (self->inactiveGestureHandler != view) {
        return;
    }

    [view removeFromSuperview];

    [self->gestureHandlers addObject:view];

    self->inactiveGestureHandler = [self newGestureHandlerView];

    if (self->gestureHandlers.count > 10) {
        NSLog(@"Evicting excessive gesture handlers "
               "(perhaps there's an unhandled 'end of gesture' event?)");

        TCWGestureHandlerView *deletedView =
            [self->gestureHandlers objectAtIndex:0];
        [deletedView cancelGesture];
        [self->gestureHandlers removeObjectAtIndex:0];
    }
}

- (void)gestureEndedInView:(TCWGestureHandlerView *)view {
    NSUInteger index = [self->gestureHandlers indexOfObject:view];
    NSAssert(index != NSNotFound, @"Unrecognized view");

    [self->gestureHandlers removeObjectAtIndex:index];
}

- (NSPoint)locationOfEvent:(NSEvent *)event {
    NSPoint windowLoc = event.locationInWindow;
    return [self->window.contentView convertPoint:windowLoc fromView:nil];
}

@end

Class tcw_wnd_ctrler_cls() { return [TCWWindowController class]; }
