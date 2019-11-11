#import "TCWBridge.h"
#import <Cocoa/Cocoa.h>

@interface TCWDeferredInvocation : NSObject
- (id)initWithUserData:(TCWInvokeUserData)ud;
- (void)fire:(NSTimer *)timer;
- (void)dealloc;
@end

@implementation TCWDeferredInvocation {
    TCWInvokeUserData ud;
    bool done;
}

- (id)initWithUserData:(TCWInvokeUserData)_ud {
    if (self = [super init]) {
        self->ud = _ud;
        self->done = false;
    }

    return self;
}

- (void)fire:(NSTimer *)timer {
    (void)timer;

    NSAssert(!self->done, @"The function has already been called.");
    self->done = true;
    tcw_invoke_fire(self->ud);
}

- (void)dealloc {
    if (!self->done) {
        tcw_invoke_cancel(self->ud);
    }
}

@end

extern id TCWInvokeAfter(double delay, double tolerance, TCWInvokeUserData ud) {
    TCWDeferredInvocation *invocation =
        [[TCWDeferredInvocation alloc] initWithUserData:ud];

    NSTimer *timer = [NSTimer timerWithTimeInterval:delay
                                             target:invocation
                                           selector:@selector(fire)
                                           userInfo:nil
                                            repeats:NO];

    timer.tolerance = tolerance;

    [[NSRunLoop mainRunLoop] addTimer:timer forMode:NSRunLoopCommonModes];

    return timer;
}
