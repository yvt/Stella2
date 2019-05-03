#import <Cocoa/Cocoa.h>

// These callbacks are defined in `window.rs`
typedef void *TCWListenerUserData;
extern BOOL tcw_wndlistener_should_close(TCWListenerUserData ud);
extern void tcw_wndlistener_close(TCWListenerUserData ud);
extern void tcw_wndlistener_resize(TCWListenerUserData ud);
extern void tcw_wndlistener_dpi_scale_changed(TCWListenerUserData ud);

// These flags must be synchronized with `WndFlags`
#define kTCW3WndFlagsResizable ((uint32_t)(1 << 0))
#define kTCW3WndFlagsBorderless ((uint32_t)(1 << 1))

@interface TCWWindowView : NSView
@end

@implementation TCWWindowView

// Override `NSView`
- (BOOL)isFlipped {
    // Flip the window contents to match TCW3's coordinate space
    return YES;
}

@end

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

        self->window.contentView = [TCWWindowView new];
        self->window.contentView.wantsLayer = YES;
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

@end

Class tcw_wnd_ctrler_cls() { return [TCWWindowController class]; }
