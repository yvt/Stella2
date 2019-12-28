#include <gtk/gtk.h>

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

struct _TcwWndWidget {
    GtkDrawingArea parent_instance;
};

struct _TcwWndWidgetClass {
    GtkDrawingAreaClass parent_class;
};

GType tcw_wnd_widget_get_type(void);

G_DEFINE_TYPE(TcwWndWidget, tcw_wnd_widget, GTK_TYPE_DRAWING_AREA)

static void tcw_wnd_widget_class_init(TcwWndWidgetClass *klass) {
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);
    (void)widget_class;
}

static void tcw_wnd_widget_init(TcwWndWidget *self) {
    GtkWidget *widget = GTK_WIDGET(self);
    (void)widget;
}
