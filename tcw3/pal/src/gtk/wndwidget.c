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
static void tcw_wnd_widget_notify_scale_factor(TcwWndWidget *wnd_widget,
                                               GParamSpec *pspec,
                                               gpointer user_data);
static gboolean tcw_wnd_widget_button_press_event(GtkWidget *widget,
                                                  GdkEventButton *event);
static gboolean tcw_wnd_widget_button_release_event(GtkWidget *widget,
                                                    GdkEventButton *event);
static gboolean tcw_wnd_widget_motion_notify_event(GtkWidget *widget,
                                                   GdkEventMotion *event);
static gboolean tcw_wnd_widget_leave_notify_event(GtkWidget *widget,
                                                  GdkEventCrossing *event);
static gboolean tcw_wnd_widget_scroll_event(GtkWidget *widget,
                                            GdkEventScroll *event);

static void tcw_wnd_widget_class_init(TcwWndWidgetClass *klass) {
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);
    widget_class->draw = tcw_wnd_widget_draw;
    widget_class->button_press_event = tcw_wnd_widget_button_press_event;
    widget_class->button_release_event = tcw_wnd_widget_button_release_event;
    widget_class->motion_notify_event = tcw_wnd_widget_motion_notify_event;
    widget_class->leave_notify_event = tcw_wnd_widget_leave_notify_event;
    widget_class->scroll_event = tcw_wnd_widget_scroll_event;
}

static void tcw_wnd_widget_init(TcwWndWidget *self) {
    GtkWidget *widget = GTK_WIDGET(self);

    g_signal_connect_object(self, "notify::scale-factor",
                            G_CALLBACK(tcw_wnd_widget_notify_scale_factor),
                            self, 0);

    // Enable events
    gtk_widget_set_events(
        widget, gtk_widget_get_events(widget) | GDK_LEAVE_NOTIFY_MASK |
                    GDK_BUTTON_PRESS_MASK | GDK_BUTTON_RELEASE_MASK |
                    GDK_POINTER_MOTION_MASK | GDK_SCROLL_MASK |
                    GDK_SMOOTH_SCROLL_MASK);
}

static gboolean tcw_wnd_widget_draw(GtkWidget *widget, cairo_t *cr) {
    TcwWndWidget *wnd_widget = TCW_WND_WIDGET(widget);
    tcw_wnd_widget_draw_handler(wnd_widget->wnd_ptr, cr);
    return TRUE;
}

static void tcw_wnd_widget_notify_scale_factor(TcwWndWidget *wnd_widget,
                                               GParamSpec *pspec,
                                               gpointer user_data) {
    (void)pspec;
    (void)user_data;
    tcw_wnd_widget_dpi_scale_changed_handler(wnd_widget->wnd_ptr);
}

static gboolean tcw_wnd_widget_button_press_event(GtkWidget *widget,
                                                  GdkEventButton *event) {
    TcwWndWidget *wnd_widget = TCW_WND_WIDGET(widget);
    if (event->type != GDK_BUTTON_PRESS) {
        // When the user performs a double click, the system generates
        // `GDK_2BUTTON_PRESS` along with the normal mouse down event
        // (`GDK_BUTTON_PRESS`) for the second click. Just ignore
        // `GDK_2BUTTON_PRESS`.
        return TRUE;
    }
    tcw_wnd_widget_button_handler(wnd_widget->wnd_ptr, (float)event->x,
                                  (float)event->y, 1, (int)event->button - 1);
    return TRUE;
}

static gboolean tcw_wnd_widget_button_release_event(GtkWidget *widget,
                                                    GdkEventButton *event) {
    TcwWndWidget *wnd_widget = TCW_WND_WIDGET(widget);
    tcw_wnd_widget_button_handler(wnd_widget->wnd_ptr, (float)event->x,
                                  (float)event->y, 0, (int)event->button - 1);
    return TRUE;
}

static gboolean tcw_wnd_widget_motion_notify_event(GtkWidget *widget,
                                                   GdkEventMotion *event) {
    TcwWndWidget *wnd_widget = TCW_WND_WIDGET(widget);
    tcw_wnd_widget_motion_handler(wnd_widget->wnd_ptr, (float)event->x,
                                  (float)event->y);
    return TRUE;
}

static gboolean tcw_wnd_widget_leave_notify_event(GtkWidget *widget,
                                                  GdkEventCrossing *event) {
    TcwWndWidget *wnd_widget = TCW_WND_WIDGET(widget);
    (void)event;
    tcw_wnd_widget_leave_handler(wnd_widget->wnd_ptr);
    return TRUE;
}

static gboolean tcw_wnd_widget_scroll_event(GtkWidget *widget,
                                            GdkEventScroll *event) {
    TcwWndWidget *wnd_widget = TCW_WND_WIDGET(widget);
    (void)event;

    gdouble delta_x, delta_y;
    GdkScrollDirection direction;

    if (gdk_event_get_scroll_deltas((GdkEvent *)event, &delta_x, &delta_y)) {
        if (gdk_event_is_scroll_stop_event((GdkEvent *)event)) {
            tcw_wnd_widget_smooth_scroll_stop_handler(wnd_widget->wnd_ptr,
                                                      event->time);
        } else {
            tcw_wnd_widget_smooth_scroll_handler(
                wnd_widget->wnd_ptr, (float)event->x, (float)event->y,
                (float)delta_x, (float)delta_y, event->time);
        }
    } else if (gdk_event_get_scroll_direction((GdkEvent *)event, &direction)) {
        static float steps[4][2] = {
            [GDK_SCROLL_LEFT][0] = 1.0,   [GDK_SCROLL_LEFT][1] = 0.0,
            [GDK_SCROLL_RIGHT][0] = -1.0, [GDK_SCROLL_RIGHT][1] = 0.0,
            [GDK_SCROLL_UP][0] = 0.0,     [GDK_SCROLL_UP][1] = 1.0,
            [GDK_SCROLL_DOWN][0] = 0.0,   [GDK_SCROLL_DOWN][1] = -1.0,
        };
        tcw_wnd_widget_discrete_scroll_handler(
            wnd_widget->wnd_ptr, (float)event->x, (float)event->y,
            steps[direction][0], steps[direction][1]);
    }

    return TRUE;
}

/// Called by `window.rs`.
extern TcwWndWidget *tcw_wnd_widget_new(void) {
    return g_object_new(TCW_TYPE_WND_WIDGET, NULL);
}
