#import "TCWWindowView.h"

@implementation TCWWindowView

// Override `NSView`
- (BOOL)isFlipped {
    // Flip the window contents to match TCW3's coordinate space
    return YES;
}

@end
