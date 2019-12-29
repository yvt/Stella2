#pragma once

#include <stdint.h>
#include <gtk/gtk.h>

// Defined in `window.rs`
extern void tcw_wnd_widget_draw_handler(size_t wnd_ptr, cairo_t *cr);
extern void tcw_wnd_widget_dpi_scale_changed_handler(size_t wnd_ptr);
