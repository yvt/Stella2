#import <Cocoa/Cocoa.h>

// These callbacks are defined in `window.rs`
typedef void *TCWListenerUserData;
extern BOOL tcw_wndlistener_should_close(TCWListenerUserData ud);
extern void tcw_wndlistener_close(TCWListenerUserData ud);

@interface TCWWindowController : NSObject {
    NSWindow *window;
}
@property TCWListenerUserData listenerUserData;
@end

@implementation TCWWindowController

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
        self->window.delegate = (id<NSWindowDelegate>)self;
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
    self->window.contentView.layer = layer;
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
    tcw_wndlistener_close(self.listenerUserData);
}

@end

Class tcw_wnd_ctrler_cls() { return [TCWWindowController class]; }
