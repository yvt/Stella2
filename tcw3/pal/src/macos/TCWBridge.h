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
extern void tcw_wndlistener_update_ready(TCWListenerUserData ud);
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

// These callbacks are defined in `timer.rs`
typedef struct _TraitObject {
    void *__data;
    void *__vtable;
} TCWInvokeUserData;
extern void tcw_invoke_fire(TCWInvokeUserData ud);
extern void tcw_invoke_cancel(TCWInvokeUserData ud);

// These variants must be synchronized with `CursorShape`
typedef enum TCW3CursorShape {
    kTCW3CursorShapeDefault,
    kTCW3CursorShapeCrosshair,
    kTCW3CursorShapeHand,
    kTCW3CursorShapeArrow,
    kTCW3CursorShapeMove,
    kTCW3CursorShapeText,
    kTCW3CursorShapeWait,
    kTCW3CursorShapeHelp,
    kTCW3CursorShapeProgress,
    kTCW3CursorShapeNotAllowed,
    kTCW3CursorShapeContextMenu,
    kTCW3CursorShapeCell,
    kTCW3CursorShapeVerticalText,
    kTCW3CursorShapeAlias,
    kTCW3CursorShapeCopy,
    kTCW3CursorShapeNoDrop,
    kTCW3CursorShapeGrab,
    kTCW3CursorShapeGrabbing,
    kTCW3CursorShapeAllScroll,
    kTCW3CursorShapeZoomIn,
    kTCW3CursorShapeZoomOut,
    kTCW3CursorShapeEResize,
    kTCW3CursorShapeNResize,
    kTCW3CursorShapeNeResize,
    kTCW3CursorShapeNwResize,
    kTCW3CursorShapeSResize,
    kTCW3CursorShapeSeResize,
    kTCW3CursorShapeSwResize,
    kTCW3CursorShapeWResize,
    kTCW3CursorShapeEwResize,
    kTCW3CursorShapeNsResize,
    kTCW3CursorShapeNeswResize,
    kTCW3CursorShapeNwseResize,
    kTCW3CursorShapeColResize,
    kTCW3CursorShapeRowResize,
} TCW3CursorShape;
