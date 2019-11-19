#pragma once
#import <Cocoa/Cocoa.h>

#import "TCWBridge.h"

@class TCWGestureHandlerView;

@interface TCWWindowController : NSObject
@property TCWListenerUserData listenerUserData;

// TODO: The following two methods aren't actually used - `TCWGestureHandlerView`
//       is WIP, perhaps?
/** Called by `TCWGestureHandlerView`. */
- (void)gestureStartedInView:(TCWGestureHandlerView *)view;
/** Called by `TCWGestureHandlerView`. */
- (void)gestureEndedInView:(TCWGestureHandlerView *)view;

/**
 * This method is used by `TCWGestureHandlerView` to convert the event's
 * position to content view coordinates.
 */
- (NSPoint)locationOfEvent:(NSEvent *)event;
@end
