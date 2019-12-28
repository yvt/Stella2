#include <gtk/gtk.h>
#include <stdint.h>

#include "window.h"

#define TCW_TYPE_WND_WIDGET (tcw_wnd_widget_get_type())
#define TCW_WND_WIDGET(obj)                                                    \
    (G_TYPE_CHECK_INSTANCE_CAST((obj), TCW_TYPE_WND_WIDGET, TcwWndWidget))
#define TCW_WND_WIDGET_CLASS(klass)                                            \
    (G_TYPE_CHECK_CLASS_CAST((klass), TCW_TYPE_WND_WIDGET, TcwWndWidgetClass))
#define STROKER_IS_NODAL_CONTAINER(obj)                                        \
    (G_TYPE_CHECK_INSTANCE_TYPE((obj), TCW_TYPE_WND_WIDGET))
#define STROKER_IS_NODAL_CONTAINER_CLASS(klass)                                \
    (G_TYPE_CHECK_CLASS_TYPE((klass), TCW_TYPE_WND_WIDGET))
#define TCW_WND_WIDGET_GET_CLASS(obj)                                          \
    (G_TYPE_INSTANCE_GET_CLASS((obj), TCW_TYPE_WND_WIDGET, TcwWndWidgetClass))

typedef struct _TcwWndWidget TcwWndWidget;
typedef struct _TcwWndWidgetClass TcwWndWidgetClass;

// These definitions must be synchronized with `window.rs`
struct _TcwWndWidget {
    GtkDrawingArea parent_instance;
    size_t wnd_ptr;
};

struct _TcwWndWidgetClass {
    GtkDrawingAreaClass parent_class;
};

GType tcw_wnd_widget_get_type(void);

G_DEFINE_TYPE(TcwWndWidget, tcw_wnd_widget, GTK_TYPE_DRAWING_AREA)

static gboolean tcw_wnd_widget_draw(GtkWidget *widget, cairo_t *cr);

static void tcw_wnd_widget_class_init(TcwWndWidgetClass *klass) {
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);
    widget_class->draw = tcw_wnd_widget_draw;
}

static void tcw_wnd_widget_init(TcwWndWidget *self) {
    GtkWidget *widget = GTK_WIDGET(self);
    (void)widget;
}

static gboolean tcw_wnd_widget_draw(GtkWidget *widget, cairo_t *cr) {
    TcwWndWidget *wnd_widget = TCW_WND_WIDGET(widget);
    tcw_wnd_widget_draw_handler(wnd_widget->wnd_ptr, cr);
    return TRUE;
}

/// Called by `window.rs`.
extern TcwWndWidget *tcw_wnd_widget_new(void) {
    return g_object_new(TCW_TYPE_WND_WIDGET, NULL);
}
