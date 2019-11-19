#import "TCWWindowView.h"
#import "TCWBridge.h"
#import "TCWWindowController.h"

@implementation TCWWindowView {
    NSTrackingArea *trackingArea;
    TCWWindowController __weak *controller;

    NSCursor *_Nonnull currentCursor;
}

- (id)initWithController:(TCWWindowController *)_controller {
    if (self = [self init]) {
        self->controller = _controller;
        self->currentCursor = [NSCursor arrowCursor];
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

- (void)setCursorShape:(TCW3CursorShape)shape {
    // Based on `winit/.../macos/util/cursor.rs`
    switch (shape) {
    case kTCW3CursorShapeDefault:
    case kTCW3CursorShapeArrow:
        self->currentCursor = [NSCursor arrowCursor];
        break;
    case kTCW3CursorShapeCrosshair:
        self->currentCursor = [NSCursor crosshairCursor];
        break;
    case kTCW3CursorShapeHand:
        self->currentCursor = [NSCursor pointingHandCursor];
        break;
    case kTCW3CursorShapeText:
        self->currentCursor = [NSCursor IBeamCursor];
        break;
    case kTCW3CursorShapeNotAllowed:
    case kTCW3CursorShapeNoDrop:
        self->currentCursor = [NSCursor operationNotAllowedCursor];
        break;
    case kTCW3CursorShapeContextMenu:
        self->currentCursor = [NSCursor contextualMenuCursor];
        break;
    case kTCW3CursorShapeVerticalText:
        self->currentCursor = [NSCursor IBeamCursorForVerticalLayout];
        break;
    case kTCW3CursorShapeAlias:
        self->currentCursor = [NSCursor dragLinkCursor];
        break;
    case kTCW3CursorShapeCopy:
        self->currentCursor = [NSCursor dragCopyCursor];
        break;
    case kTCW3CursorShapeGrab:
        self->currentCursor = [NSCursor openHandCursor];
        break;
    case kTCW3CursorShapeGrabbing:
        self->currentCursor = [NSCursor closedHandCursor];
        break;
    case kTCW3CursorShapeEResize:
        self->currentCursor = [NSCursor resizeRightCursor];
        break;
    case kTCW3CursorShapeNResize:
        self->currentCursor = [NSCursor resizeUpCursor];
        break;
    case kTCW3CursorShapeWResize:
        self->currentCursor = [NSCursor resizeLeftCursor];
        break;
    case kTCW3CursorShapeSResize:
        self->currentCursor = [NSCursor resizeDownCursor];
        break;
    case kTCW3CursorShapeEwResize:
    case kTCW3CursorShapeColResize:
        self->currentCursor = [NSCursor resizeLeftRightCursor];
        break;
    case kTCW3CursorShapeNsResize:
    case kTCW3CursorShapeRowResize:
        self->currentCursor = [NSCursor resizeUpDownCursor];
        break;

    // Undocumented cursors: https://stackoverflow.com/a/46635398/5435443
    case kTCW3CursorShapeHelp:
        self->currentCursor =
            [TCWWindowView undocumentedSystemCursor:@selector(_helpCursor)];
        break;
    case kTCW3CursorShapeZoomIn:
        self->currentCursor =
            [TCWWindowView undocumentedSystemCursor:@selector(_zoomInCursor)];
        break;
    case kTCW3CursorShapeZoomOut:
        self->currentCursor =
            [TCWWindowView undocumentedSystemCursor:@selector(_zoomOutCursor)];
        break;
    case kTCW3CursorShapeNeResize:
        self->currentCursor = [TCWWindowView
            undocumentedSystemCursor:@selector(_windowResizeNorthEastCursor)];
        break;
    case kTCW3CursorShapeNwResize:
        self->currentCursor = [TCWWindowView
            undocumentedSystemCursor:@selector(_windowResizeNorthWestCursor)];
        break;
    case kTCW3CursorShapeSeResize:
        self->currentCursor = [TCWWindowView
            undocumentedSystemCursor:@selector(_windowResizeSouthEastCursor)];
        break;
    case kTCW3CursorShapeSwResize:
        self->currentCursor = [TCWWindowView
            undocumentedSystemCursor:@selector(_windowResizeSouthWestCursor)];
        break;
    case kTCW3CursorShapeNeswResize:
        self->currentCursor =
            [TCWWindowView undocumentedSystemCursor:@selector
                           (_windowResizeNorthEastSouthWestCursor)];
        break;
    case kTCW3CursorShapeNwseResize:
        self->currentCursor =
            [TCWWindowView undocumentedSystemCursor:@selector
                           (_windowResizeNorthWestSouthEastCursor)];
        break;
    // https://bugs.eclipse.org/bugs/show_bug.cgi?id=522349
    case kTCW3CursorShapeWait:
    case kTCW3CursorShapeProgress:
        self->currentCursor = [TCWWindowView
            undocumentedSystemCursor:@selector(busyButClickableCursor)];
        break;

    case kTCW3CursorShapeMove:
    case kTCW3CursorShapeCell:
    case kTCW3CursorShapeAllScroll:
    default:
        NSLog(@"Unimplemented cursor shape: %d", (int)shape);
        break;
    }

    [self.window invalidateCursorRectsForView:self];
}

+ (NSCursor *)undocumentedSystemCursor:(SEL)sel {
    if ([NSCursor respondsToSelector:sel]) {
        return [NSCursor performSelector:sel];
    } else {
        return [NSCursor arrowCursor];
    }
}

/** Overrides `NSView`'s method. */
- (void)resetCursorRects {
    [self addCursorRect:self.bounds cursor:self->currentCursor];
}

@end
