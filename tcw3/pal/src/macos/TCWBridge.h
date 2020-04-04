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
typedef OPQAUE_HANDLE TCWScrollListenerUserData;
extern BOOL tcw_wndlistener_should_close(TCWListenerUserData ud);
extern void tcw_wndlistener_close(TCWListenerUserData ud);
extern void tcw_wndlistener_resize(TCWListenerUserData ud);
extern void tcw_wndlistener_dpi_scale_changed(TCWListenerUserData ud);
extern void tcw_wndlistener_update_ready(TCWListenerUserData ud);
extern void tcw_wndlistener_focus(TCWListenerUserData ud);
extern void tcw_wndlistener_mouse_motion(TCWListenerUserData ud, NSPoint loc);
extern void tcw_wndlistener_mouse_leave(TCWListenerUserData ud);
extern TCWMouseDragListenerUserData
tcw_wndlistener_mouse_drag(TCWListenerUserData ud, NSPoint loc, uint8_t button);

extern int tcw_wnd_has_text_input_ctx(TCWListenerUserData ud);
extern void tcw_wnd_insert_text(TCWListenerUserData ud, const char *str,
                                size_t replace_start, size_t replace_len);
extern void tcw_wnd_set_marked_text(TCWListenerUserData ud, const char *str,
                                    size_t sel_start, size_t sel_len,
                                    size_t replace_start, size_t replace_len);
extern void tcw_wnd_unmark_text(TCWListenerUserData ud);
extern NSRange tcw_wnd_get_selected_range(TCWListenerUserData ud);
extern NSRange tcw_wnd_get_marked_range(TCWListenerUserData ud);
extern NSString *tcw_wnd_get_text(TCWListenerUserData ud, size_t start,
                                  size_t len, NSRange *actual_range);
extern NSRect tcw_wnd_get_text_rect(TCWListenerUserData ud, size_t start,
                                    size_t len, NSRange *actual_range);
extern NSUInteger tcw_wnd_get_char_index_from_point(TCWListenerUserData ud,
                                                    NSPoint loc);

extern void tcw_mousedraglistener_release(TCWMouseDragListenerUserData ud);
extern void tcw_mousedraglistener_cancel(TCWMouseDragListenerUserData ud);
extern void tcw_mousedraglistener_mouse_motion(TCWMouseDragListenerUserData ud,
                                               NSPoint loc);
extern void tcw_mousedraglistener_mouse_down(TCWMouseDragListenerUserData ud,
                                             NSPoint loc, uint8_t button);
extern void tcw_mousedraglistener_mouse_up(TCWMouseDragListenerUserData ud,
                                           NSPoint loc, uint8_t button);

extern void tcw_wndlistener_scroll_motion(TCWListenerUserData ud, NSPoint loc,
                                          uint8_t precise, double delta_x,
                                          double delta_y);
extern TCWScrollListenerUserData
tcw_wndlistener_scroll_gesture(TCWListenerUserData ud, NSPoint loc);
extern void tcw_scrolllistener_release(TCWScrollListenerUserData ud);
extern void tcw_scrolllistener_cancel(TCWScrollListenerUserData ud);
extern void tcw_scrolllistener_end(TCWScrollListenerUserData ud);
extern void
tcw_scrolllistener_start_momentum_phase(TCWScrollListenerUserData ud);
extern void tcw_scrolllistener_motion(TCWScrollListenerUserData ud,
                                      uint8_t precise, double delta_x,
                                      double delta_y, double vel_x,
                                      double vel_y);

// These flags must be synchronized with `WndFlags`
#define kTCW3WndFlagsResizable ((uint32_t)(1 << 0))
#define kTCW3WndFlagsBorderless ((uint32_t)(1 << 1))
#define kTCW3WndFlagsTransparentBackdropBlur ((uint32_t)(1 << 2))
#define kTCW3WndFlagsFullSizeContent ((uint32_t)(1 << 3))

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
