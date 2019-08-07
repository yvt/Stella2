#import <Cocoa/Cocoa.h>

@interface TCWWinitView : NSView
- (id)init;
- (void)setContentLayer:(CALayer *)layer;
@end

@implementation TCWWinitView {
}

- (id)init {
    if (self = [super init]) {
        self.wantsLayer = YES;
    }
    return self;
}

// Override `NSView`
- (BOOL)isFlipped {
    // Flip the window contents to match TCW3's coordinate space
    return YES;
}

- (void)setContentLayer:(CALayer *)layer {
    self.layer.sublayers = layer ? @[ layer ] : @[];
}

- (void)setupLayout {
    NSView *superview = self.superview;

    NSAssert(superview, @"superview is not set");

    self.frame = superview.frame;
    self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
}

@end

Class tcw_winit_view_cls() { return [TCWWinitView class]; }
