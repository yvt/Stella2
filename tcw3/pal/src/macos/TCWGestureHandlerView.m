#include <math.h>
#include <objc/runtime.h>

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

/** Takes `NSAttributedString` or `NSString` and coalesces it into `NSString`.
 */
static NSString *coalesceString(id string) {
    if ([string isKindOfClass:[NSAttributedString class]]) {
        return ((NSAttributedString *)string).string;
    } else {
        NSCAssert([string isKindOfClass:[NSString class]], @"bad string type");
        return (NSString *)string;
    }
}

/**
 * Returns the character code if the given string consists of a single character
 * in Unicode BMP. Otherwise, returns `0`.
 */
static unichar singleCharcterCodeOfString(NSString *string) {
    if (string.length == 1) {
        return [string characterAtIndex:0];
    } else {
        return 0;
    }
}

static void dynamicActionHandler(TCWGestureHandlerView *self, SEL sel,
                                 id sender);

static NSMutableSet<TCWGestureHandlerView *> *viewInstances = nil;

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

        if (!viewInstances) {
            viewInstances = [[NSMutableSet alloc] init];
        }
        [viewInstances addObject:self];
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
        TCW3NcHit hit =
            tcw_wndlistener_nc_hit_test(self->controller.listenerUserData, loc);

        if (hit != kTCW3NcHitClient) {
            [self.window performWindowDragWithEvent:event];
            return;
        }

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

    [viewInstances removeObject:self];

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

/// Overrides `NSResponder`'s method.
- (void)keyDown:(NSEvent *)event {
    if (!self->controller) {
        return;
    }

    unichar charcodeUnmodified =
        singleCharcterCodeOfString(event.charactersIgnoringModifiers);
    int handled = tcw_wndlistener_key_down(
        self->controller.listenerUserData,
        (uint16_t)(event.modifierFlags >> 16), charcodeUnmodified);
    if (handled) {
        return;
    }

    if (!self->controller) {
        return;
    }

    if (tcw_wnd_has_text_input_ctx(self->controller.listenerUserData)) {
        // Forward the event to the system input manager for interpretation as
        // a text input command.
        [self interpretKeyEvents:@[ event ]];
    }
}

/// Implements `NSTextInputClient`'s method.
- (void)insertText:(id)string replacementRange:(NSRange)replacementRange {
    if (!self->controller) {
        return;
    }

    NSString *theString = coalesceString(string);

    tcw_wnd_insert_text(self->controller.listenerUserData, theString.UTF8String,
                        (size_t)replacementRange.location,
                        (size_t)replacementRange.length);
}

/// Implements `NSTextInputClient`'s method.
- (void)doCommandBySelector:(SEL)selector {
    (void)selector;
    NSLog(@"doCommandBySelector:%s TODO!", sel_getName(selector));
}

/// Implements `NSTextInputClient`'s method.
- (void)setMarkedText:(id)string
        selectedRange:(NSRange)selectedRange
     replacementRange:(NSRange)replacementRange {
    if (!self->controller) {
        return;
    }

    NSString *theString = coalesceString(string);

    tcw_wnd_set_marked_text(
        self->controller.listenerUserData, theString.UTF8String,
        (size_t)selectedRange.location, (size_t)selectedRange.length,
        (size_t)replacementRange.location, (size_t)replacementRange.length);
}

/// Implements `NSTextInputClient`'s method.
- (void)unmarkText {
    if (!self->controller) {
        return;
    }

    tcw_wnd_unmark_text(self->controller.listenerUserData);
}

/// Implements `NSTextInputClient`'s method.
- (NSRange)selectedRange {
    if (!self->controller) {
        return NSMakeRange(NSNotFound, 0);
    }

    return tcw_wnd_get_selected_range(self->controller.listenerUserData);
}

/// Implements `NSTextInputClient`'s method.
- (NSRange)markedRange {
    if (!self->controller) {
        return NSMakeRange(NSNotFound, 0);
    }

    return tcw_wnd_get_marked_range(self->controller.listenerUserData);
}

/// Implements `NSTextInputClient`'s method.
- (BOOL)hasMarkedText {
    return self.markedRange.location != NSNotFound;
}

/// Implements `NSTextInputClient`'s method.
- (nullable NSAttributedString *)
    attributedSubstringForProposedRange:(NSRange)range
                            actualRange:(nullable NSRangePointer)actualRange {
    if (!self->controller) {
        if (actualRange) {
            *actualRange = NSMakeRange(0, 0);
        }
        return [[NSAttributedString alloc] initWithString:@""];
    }

    NSString *string = tcw_wnd_get_text(self->controller.listenerUserData,
                                        (size_t)range.location,
                                        (size_t)range.length, actualRange);

    return [[NSAttributedString alloc] initWithString:string];
}

/// Implements `NSTextInputClient`'s method.
- (NSArray<NSAttributedStringKey> *)validAttributesForMarkedText {
    return @[];
}

/// Implements `NSTextInputClient`'s method.
- (NSRect)firstRectForCharacterRange:(NSRange)range
                         actualRange:(nullable NSRangePointer)actualRange {
    if (!self->controller) {
        if (actualRange) {
            *actualRange = NSMakeRange(0, 0);
        }
        return NSMakeRect(0, 0, 0, 0);
    }

    NSRect bounds = tcw_wnd_get_text_rect(self->controller.listenerUserData,
                                          (size_t)range.location,
                                          (size_t)range.length, actualRange);

    bounds = [self.superview convertRect:bounds toView:nil];

    return [self.window convertRectToScreen:bounds];
}

/// Implements `NSTextInputClient`'s method.
- (NSUInteger)characterIndexForPoint:(NSPoint)point {
    if (!self->controller) {
        return NSNotFound;
    }

    point =
        [self.window convertRectFromScreen:NSMakeRect(point.x, point.y, 0, 0)]
            .origin;

    point = [self convertRect:NSMakeRect(point.x, point.y, 0, 0) fromView:nil]
                .origin;

    return tcw_wnd_get_char_index_from_point(self->controller.listenerUserData,
                                             point);
}

/// Overrides `NSObject`'s method.
- (BOOL)respondsToSelector:(SEL)sel {
    if ([super respondsToSelector:sel]) {
        return YES;
    }

    // If `sel` can be translated by an accelerator table, dynamically define
    // a method with this selector.
    TCW3ActionStatus status = [self validateSelector:sel];

    if ((status & kTCW3ActionStatusValid) == 0) {
        return NO;
    }

    // Create an implementation
    class_addMethod([self class], sel, (IMP)dynamicActionHandler, "v@:@");

    return YES;
}

/// Implements `NSMenuItemValidation`.
- (BOOL)validateMenuItem:(NSMenuItem *)menuItem {
    TCW3ActionStatus status = [self validateSelector:menuItem.action];

    menuItem.state = (status & kTCW3ActionStatusChecked)
                         ? NSControlStateValueOn
                         : NSControlStateValueOff;

    return (status & (kTCW3ActionStatusValid | kTCW3ActionStatusEnabled)) ==
           (kTCW3ActionStatusValid | kTCW3ActionStatusEnabled);
}

/** @private */
- (TCW3ActionStatus)validateSelector:(SEL)sel {
    if (!self->controller) {
        return 0;
    }

    const char *selName = sel_getName(sel);
    size_t selLength = strlen(selName);

    return tcw_wndlistener_validate_selector(self->controller.listenerUserData,
                                             selName, selLength);
}

/** @private */
- (void)performSelectorDynamic:(SEL)sel {
    if (!self->controller) {
        return;
    }

    const char *selName = sel_getName(sel);
    size_t selLength = strlen(selName);

    tcw_wndlistener_perform_selector(self->controller.listenerUserData, selName,
                                     selLength);
}

@end

static void dynamicActionHandler(TCWGestureHandlerView *self, SEL sel,
                                 id sender) {
    (void)sender;
    [self performSelectorDynamic:sel];
}
