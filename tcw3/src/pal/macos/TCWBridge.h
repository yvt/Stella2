#pragma once

#import <Cocoa/Cocoa.h>
#include <stdint.h>

// Represents a type-safe opaque handle.
#define OPQAUE_HANDLE                                                          \
    struct {                                                                   \
        void *__ptr;                                                           \
    }

// These callbacks are defined in `window.rs`
typedef OPQAUE_HANDLE TCWListenerUserData;
typedef OPQAUE_HANDLE TCWMouseDragListenerUserData;
extern BOOL tcw_wndlistener_should_close(TCWListenerUserData ud);
extern void tcw_wndlistener_close(TCWListenerUserData ud);
extern void tcw_wndlistener_resize(TCWListenerUserData ud);
extern void tcw_wndlistener_dpi_scale_changed(TCWListenerUserData ud);
extern void tcw_wndlistener_mouse_motion(TCWListenerUserData ud, NSPoint loc);
extern void tcw_wndlistener_mouse_leave(TCWListenerUserData ud);
extern TCWMouseDragListenerUserData
tcw_wndlistener_mouse_drag(TCWListenerUserData ud, NSPoint loc, uint8_t button);

extern void tcw_mousedraglistener_release(TCWMouseDragListenerUserData ud);
extern void tcw_mousedraglistener_cancel(TCWMouseDragListenerUserData ud);
extern void tcw_mousedraglistener_mouse_motion(TCWMouseDragListenerUserData ud,
                                               NSPoint loc);
extern void tcw_mousedraglistener_mouse_down(TCWMouseDragListenerUserData ud,
                                             NSPoint loc, uint8_t button);
extern void tcw_mousedraglistener_mouse_up(TCWMouseDragListenerUserData ud,
                                           NSPoint loc, uint8_t button);

// These flags must be synchronized with `WndFlags`
#define kTCW3WndFlagsResizable ((uint32_t)(1 << 0))
#define kTCW3WndFlagsBorderless ((uint32_t)(1 << 1))
