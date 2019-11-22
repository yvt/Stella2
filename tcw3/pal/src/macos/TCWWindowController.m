#include <stdatomic.h>
#include <stdbool.h>

#import "TCWBridge.h"
#import "TCWGestureHandlerView.h"
#import "TCWWindowController.h"
#import "TCWWindowView.h"

@implementation TCWWindowController {
    NSWindow *window;

    CVDisplayLinkRef _Nullable displayLink;
    atomic_bool handlingDisplayLinkEvent;
    bool windowIsOnscreen;
    bool displayLinkIsRunning;
    bool wantsUpdateReadyCallback;

    TCWGestureHandlerView *inactiveGestureHandler;
    NSMutableArray<TCWGestureHandlerView *> *gestureHandlers;
}

- (id)init {
    if (self) {
        self = [super init];

        self->displayLink = nil;
        self->handlingDisplayLinkEvent = false;
        self->windowIsOnscreen = false;
        self->displayLinkIsRunning = false;
        self->wantsUpdateReadyCallback = false;

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
        self->window.delegate = (id<NSWindowDelegate>)self;

        self->window.contentView =
            [[TCWWindowView alloc] initWithController:self];
        self->window.contentView.wantsLayer = YES;

        // Create the first gesture handler view
        self->inactiveGestureHandler = [self newGestureHandlerView];
        self->gestureHandlers = [NSMutableArray new];
        [self->window.contentView addSubview:self->inactiveGestureHandler];
    }
    return self;
}

/** Called by `window.rs` */
- (void)close {
    [self->window close];
}

- (void)dealloc {
    if (self->displayLink) {
        CVDisplayLinkRelease(self->displayLink);
    }
}

/** Called by `window.rs` */
- (void)setTitle:(NSString *)windowTitle {
    [self->window setTitle:windowTitle];
}

/** Called by `window.rs` */
- (void)setContentSize:(NSSize)size {
    [self->window setContentSize:size];
}

/** Called by `window.rs` */
- (void)setContentMaxSize:(NSSize)size {
    [self->window setContentMaxSize:size];
}

/** Called by `window.rs` */
- (void)setContentMinSize:(NSSize)size {
    [self->window setContentMinSize:size];
}

/** Called by `window.rs` */
- (NSSize)contentSize {
    return self->window.contentView.frame.size;
}

/** Called by `window.rs` */
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

- (void)setCursorShape:(uint32_t)shape {
    TCWWindowView *view = self->window.contentView;

    [view setCursorShape:(TCW3CursorShape)shape];
}

/** Called by `window.rs` */
- (void)makeKeyAndOrderFront {
    [self->window makeKeyAndOrderFront:nil];
}

/** Called by `window.rs` */
- (void)orderOut {
    [self->window orderOut:nil];
}

/** Called by `window.rs` */
- (void)center {
    [self->window center];
}

/** Called by `window.rs` */
- (void)setLayer:(CALayer *)layer {
    self->window.contentView.layer.sublayers = @[ layer ];
}

/** Called by `window.rs` */
- (float)dpiScale {
    return (float)self->window.backingScaleFactor;
}

/** Called by `window.rs` */
- (void)requestUpdateReady {
    if (!self->displayLink) {
        [self renewDisplayLink];
    }

    self->wantsUpdateReadyCallback = true;

    if (!self->windowIsOnscreen) {
        // The window is currently offscreen. We'll try again later when
        // the window becomes visible.
        return;
    }

    if (!self->displayLinkIsRunning) {
        self->displayLinkIsRunning = true;
        CVReturn error = CVDisplayLinkStart(self->displayLink);
        if (error) {
            NSLog(@"CVDisplayLinkStart failed: %d", (int)error);
        }
    }
}

/** @private */
- (void)renewDisplayLink {
    CVReturn error;

    NSScreen *screen = self->window.screen;

    if (!screen) {
        if (self->displayLinkIsRunning) {
            self->displayLinkIsRunning = false;

            error = CVDisplayLinkStop(self->displayLink);
            if (error) {
                NSLog(@"CVDisplayLinkStop failed: %d", (int)error);
            }
        }
        self->windowIsOnscreen = false;
        return;
    }

    self->windowIsOnscreen = true;

    NSNumber *displayIDNum =
        [screen.deviceDescription objectForKey:@"NSScreenNumber"];
    CGDirectDisplayID displayID =
        (CGDirectDisplayID)displayIDNum.unsignedIntegerValue;

    if (self->displayLink) {
        error = CVDisplayLinkSetCurrentCGDisplay(self->displayLink, displayID);
        if (error) {
            NSLog(@"CVDisplayLinkSetCurrentCGDisplay failed: %d", (int)error);
        }
    } else {
        error = CVDisplayLinkCreateWithCGDisplay(displayID, &self->displayLink);
        if (error) {
            NSLog(@"CVDisplayLinkCreateWithCGDisplay failed: %d", (int)error);
            return;
        }

        TCWWindowController __weak *selfWeak = self;
        CVDisplayLinkOutputHandler handler = ^CVReturn(
            CVDisplayLinkRef _Nonnull _displayLink,
            const CVTimeStamp *_Nonnull inNow,
            const CVTimeStamp *_Nonnull inOutputTime, CVOptionFlags flagsIn,
            CVOptionFlags *_Nonnull flagsOut) {
          (void)inNow;
          (void)inOutputTime;
          (void)flagsIn;
          (void)flagsOut;

          TCWWindowController *self = selfWeak;
          if (!self) {
              CVDisplayLinkStop(_displayLink);
              return kCVReturnSuccess;
          }

          if (atomic_load_explicit(&self->handlingDisplayLinkEvent,
                                   memory_order_relaxed)) {
              // The main thread cannot keep up with `CVDisplayLink`,
              // dropping the frame
              return kCVReturnSuccess;
          }

          atomic_store_explicit(&self->handlingDisplayLinkEvent, true,
                                memory_order_relaxed);

          [self performSelectorOnMainThread:@selector(handleDisplayLinkEvent)
                                 withObject:nil
                              waitUntilDone:NO];

          return kCVReturnSuccess;
        };
        CVReturn error =
            CVDisplayLinkSetOutputHandler(self->displayLink, handler);
        if (error) {
            NSLog(@"CVDisplayLinkSetOutputHandler failed: %d", (int)error);
        }
    }

    if (self->wantsUpdateReadyCallback && !self->displayLinkIsRunning) {
        self->displayLinkIsRunning = true;
        CVReturn error = CVDisplayLinkStart(self->displayLink);
        if (error) {
            NSLog(@"CVDisplayLinkStart failed: %d", (int)error);
        }
    }
}

/** @private */
- (void)handleDisplayLinkEvent {
    atomic_store_explicit(&self->handlingDisplayLinkEvent, false,
                          memory_order_relaxed);

    if (!self->displayLinkIsRunning) {
        return;
    }

    if (!self->wantsUpdateReadyCallback) {
        // The client does not want the callback to be called anymore...
        // Stop the `CVDisplayLink`.
        self->displayLinkIsRunning = false;
        CVReturn error = CVDisplayLinkStop(self->displayLink);
        if (error) {
            NSLog(@"CVDisplayLinkStop failed: %d", (int)error);
        }
        return;
    }

    self->wantsUpdateReadyCallback = false;
    tcw_wndlistener_update_ready(self.listenerUserData);
}

/** Implements `NSWindowDelegate`. */
- (BOOL)windowShouldClose:(NSWindow *)sender {
    (void)sender;
    return tcw_wndlistener_should_close(self.listenerUserData);
}

/** Implements `NSWindowDelegate`. */
- (void)windowWillClose:(NSNotification *)notification {
    (void)notification;
    self->window.delegate = nil;

    // Cancel all input gestures
    for (TCWGestureHandlerView *view in self->gestureHandlers) {
        [view cancelGesture];
    }

    tcw_wndlistener_close(self.listenerUserData);
}

/** Implements `NSWindowDelegate`. */
- (void)windowDidResize:(NSNotification *)notification {
    (void)notification;
    tcw_wndlistener_resize(self.listenerUserData);
}

/** Implements `NSWindowDelegate`. */
- (void)windowDidChangeBackingProperties:(NSNotification *)notification {
    (void)notification;
    tcw_wndlistener_dpi_scale_changed(self.listenerUserData);
}

/** Implements `NSWindowDelegate`. */
- (void)windowDidChangeScreen:(NSNotification *)notification {
    (void)notification;
    [self renewDisplayLink];
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

    // Create views on-demand to distinguish input gestures.
    // TODO: Do we really need this, though? It seems impossible to have
    //       multiple ongoing gestures of the same type at the same moment...

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

    [view removeFromSuperview];
}

- (NSPoint)locationOfEvent:(NSEvent *)event {
    NSPoint windowLoc = event.locationInWindow;
    return [self->window.contentView convertPoint:windowLoc fromView:nil];
}

@end

Class tcw_wnd_ctrler_cls() { return [TCWWindowController class]; }
