#pragma once
#import <Cocoa/Cocoa.h>

@class TCWWindowController;

/**
 * This view receives pointer events and redirects them to appropriate handler
 * functions.
 *
 * It initially serves as the responder for all pointer events within a
 * window. When the start of a gesture (e.g., scroll wheel with inertia
 * scrolling) is detected, it transitions into the state where it only handles
 * the events associated with the gesture so that they can be discerned from
 * other events. Meanwhile a new instance of `TCWGestureHandlerView` is created
 * to capture the non-gesture events.
 */
@interface TCWGestureHandlerView : NSView

- (id)initWithController:(TCWWindowController *)controller;

/**
 * Cancel the current gesture associated with this view. This method
 * calls event callbacks, but does not call `gestureEndedInView:`.
 */
- (void)cancelGesture;

@end
