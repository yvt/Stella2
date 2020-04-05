use cggeom::box2;
use cgmath::{Rad, Vector2};
use std::f32::{consts::PI, NAN};
use stella2_assets as assets;
use stvg_tcw3::StvgImg;
#[allow(unused_imports)]
use tcw3::{
    images::{himg_figures, HImg},
    pal::{LayerFlags, SysFontType, RGBAF32},
    stylesheet,
    ui::theming::{LayerXform, Manager, Metrics, Role, Stylesheet},
};

/// Define styling ID values.
pub mod elem_id {
    use tcw3::ui::theming::ClassSet;

    iota::iota! {
        pub const SHOW_MENU: ClassSet = ClassSet::id(iota);
                , SIDEBAR_SHOW
                , SIDEBAR_HIDE

                , SEARCH_FIELD_WRAP
                , SEARCH_FIELD

                , TOOLBAR_SEPARATOR
                , MEMBER_COUNT_ICON

                , TOOLBAR
                , SIDEBAR
                , LOG_VIEW
                , EDITOR
                , EDITOR_SPLIT
                , EDITOR_FIELD

                , SIDEBAR_GROUP_HEADER
                , SIDEBAR_GROUP_BULLET
                , SIDEBAR_ITEM

                , WND
    }
}

// Import IDs (e.g., `#SHOW_MENU`) into the scope
use self::elem_id::*;

/// Construct a `HImg` from an StVG image.
#[inline(never)]
fn himg_from_stvg(data: &(&'static [u8], [f32; 2])) -> HImg {
    StvgImg::new(*data).into_himg()
}

/// Construct a colorized `HImg` from an StVG image.
#[allow(dead_code)]
fn himg_from_stvg_col(data: (&'static [u8], [f32; 2]), c: tcw3::pal::RGBAF32) -> HImg {
    StvgImg::new(data)
        .with_color_xform(stvg_tcw3::replace_color(c))
        .into_himg()
}

fn new_custom_stylesheet() -> impl Stylesheet {
    const TOOLBAR_IMG_SIZE: Vector2<f32> = Vector2::new(24.0, 16.0);
    const TOOLBAR_IMG_METRICS: Metrics = Metrics {
        margin: [NAN; 4],
        size: TOOLBAR_IMG_SIZE,
    };
    const TOOLBAR_BTN_MIN_SIZE: Vector2<f32> = Vector2::new(34.0, 22.0);

    stylesheet! {
        ([.SPLITTER]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(0.7, 0.7, 0.7, 1.0),
            min_size: Vector2::new(1.0, 1.0),
        },
        ([#EDITOR_SPLIT.SPLITTER]) (priority = 10000) {
            min_size: Vector2::new(0.0, 0.0),
        },

        // Toolbar and titlebar background
        ([#TOOLBAR]) (priority = 10000) {
            num_layers: 2,
            layer_bg_color[0]: RGBAF32::new(0.95, 0.95, 0.95, 1.0),

            layer_bg_color[1]: RGBAF32::new(0.3, 0.3, 0.3, 0.35),
            layer_metrics[1]: Metrics {
                margin: [NAN, 0.0, 0.0, 0.0],
                size: Vector2::new(NAN, 0.65),
            },

            subview_metrics[Role::Generic]: Metrics {
                margin: [8.0, 9.0, 8.0, 9.0],
                ..Metrics::default()
            },
        },
        ([#TOOLBAR] .. [#WND.ACTIVE]) (priority = 10500) {
            layer_bg_color[0]: RGBAF32::new(0.9, 0.9, 0.9, 1.0),
        },

        // Pane background
        ([#SIDEBAR]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(0.93, 0.93, 0.93, 1.0),
        },
        // Backdrop blur isn't supported by the GTK backend. The translucent
        // sidebar looks awkward without backdrop blur, so we disable
        // transparency in this case.
        // See also: `self::ENABLE_BACKDROP_BLUR`
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        ([#SIDEBAR] .. [#WND.ACTIVE]) (priority = 10500) {
            layer_bg_color[0]: RGBAF32::new(0.93, 0.93, 0.93, 0.8),
            layer_flags[0]: LayerFlags::BACKDROP_BLUR,
        },
        ([#LOG_VIEW]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(1.0, 1.0, 1.0, 1.0),
        },
        ([#EDITOR]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(0.93, 0.93, 0.93, 1.0),

            subview_metrics[Role::Generic]: Metrics {
                margin: [5.0; 4],
                ..Metrics::default()
            },
        },

        // Toolbar buttons
        ([#SHOW_MENU.BUTTON]) (priority = 10000) {
            num_layers: 2,
            #[dyn] layer_img[1]: Some(himg_from_stvg(&assets::toolbar::MENU)),
            layer_metrics[1]: TOOLBAR_IMG_METRICS,
            min_size: TOOLBAR_BTN_MIN_SIZE,
        },
        ([#SIDEBAR_SHOW.BUTTON]) (priority = 10000) {
            num_layers: 2,
            #[dyn] layer_img[1]: Some(himg_from_stvg(&assets::toolbar::SIDEBAR_SHOW)),
            layer_metrics[1]: TOOLBAR_IMG_METRICS,
            min_size: TOOLBAR_BTN_MIN_SIZE,
        },
        ([#SIDEBAR_HIDE.BUTTON]) (priority = 10000) {
            num_layers: 2,
            #[dyn] layer_img[1]: Some(himg_from_stvg(&assets::toolbar::SIDEBAR_HIDE)),
            layer_metrics[1]: TOOLBAR_IMG_METRICS,
            min_size: TOOLBAR_BTN_MIN_SIZE,
        },

        // Toolbar presentation elements
        ([#MEMBER_COUNT_ICON]) (priority = 10000) {
            num_layers: 1,
            #[dyn] layer_img[0]: Some(himg_from_stvg(&assets::toolbar::USER_OUTLINE)),
            layer_metrics[0]: Metrics {
                margin: [NAN; 4],
                size: Vector2::new(16.0, 16.0),
            },
            min_size: Vector2::new(16.0, 16.0),
            allow_grow: [false, true],
        },
        ([#TOOLBAR_SEPARATOR]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(0.63, 0.63, 0.63, 1.0),
            layer_metrics[0]: Metrics {
                margin: [NAN; 4],
                size: Vector2::new(1.0, 15.0),
            },
            min_size: Vector2::new(15.0, 15.0),
            allow_grow: [false, true],
        },

        // Search field
        ([#SEARCH_FIELD_WRAP]) (priority = 10000) {
            subview_metrics[Role::Generic]: Metrics {
                margin: [30.0, 10.0, 10.0, 10.0],
                ..Metrics::default()
            },
            allow_grow: [true, false],
        },
        ([#SEARCH_FIELD]) (priority = 10000) {
            num_layers: 3,

            // Focus ring
            #[dyn] layer_img[0]: Some(himg_figures![rect([0.1, 0.4, 0.8, 1.0]).radius(5.0)]),
            layer_center[0]: box2! { point: [0.5, 0.5] },
            layer_opacity[0]: 0.0,
            layer_metrics[0]: Metrics {
                margin: [-2.0; 4],
                ..Metrics::default()
            },

            // Background
            #[dyn] layer_img[1]: Some(himg_figures![
                rect([0.0, 0.0, 0.0, 0.05]).radius(3.0),
                rect([0.0, 0.0, 0.0, 0.15]).radius(3.0 - 0.25).margin([0.25; 4]).line_width(0.5),
            ]),
            layer_center[1]: box2! { point: [0.5, 0.5] },

            // Icon
            #[dyn] layer_img[2]: Some(himg_from_stvg_col(assets::SEARCH, [0.4, 0.4, 0.4, 1.0].into())),
            layer_metrics[2]: Metrics {
                margin: [NAN, NAN, NAN, 4.0],
                size: Vector2::new(16.0, 16.0),
            },

            min_size: Vector2::new(150.0, TOOLBAR_BTN_MIN_SIZE.y),

            subview_metrics[Role::Generic]: Metrics {
                margin: [2.0, 2.0, 2.0, 22.0],
                ..Metrics::default()
            },
        },
        ([#SEARCH_FIELD.FOCUS]) (priority = 10000) {
            // Display the focus ring
            layer_opacity[0]: 0.5,

            // Make the background opaque so that the focus ring really looks
            // like a ring
            #[dyn] layer_img[1]: Some(himg_figures![rect([1.0, 1.0, 1.0, 1.0]).radius(3.0)]),
        },

        // Composing area
        ([#EDITOR_FIELD]) (priority = 10000) {
            num_layers: 2,

            // Focus ring
            #[dyn] layer_img[0]: Some(himg_figures![rect([0.1, 0.4, 0.8, 1.0]).radius(5.0)]),
            layer_center[0]: box2! { point: [0.5, 0.5] },
            layer_opacity[0]: 0.0,
            layer_metrics[0]: Metrics {
                margin: [-2.0; 4],
                ..Metrics::default()
            },

            // Background
            #[dyn] layer_img[1]: Some(himg_figures![rect([1.0, 1.0, 1.0, 1.0]).radius(3.0)]),
            layer_center[1]: box2! { point: [0.5, 0.5] },

            subview_metrics[Role::Generic]: Metrics {
                margin: [3.0; 4],
                ..Metrics::default()
            },
        },
        ([#EDITOR_FIELD.FOCUS]) (priority = 10500) {
            // Focus ring
            layer_opacity[0]: 0.5,
        },
        ([.LABEL] < [#EDITOR_FIELD]) (priority = 10000) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 0.4),
        },

        // Sidebar
        ([#SIDEBAR_GROUP_HEADER]) (priority = 10000) {
            // label
            subview_metrics[Role::Generic]: Metrics {
                margin: [NAN, NAN, NAN, 25.0],
                ..Metrics::default()
            },
            // bullet (open/close)
            subview_metrics[Role::Bullet]: Metrics {
                margin: [NAN, NAN, NAN, 5.0],
                size: Vector2::new(16.0, 16.0),
            },
        },
        ([.LABEL] < [#SIDEBAR_GROUP_HEADER]) (priority = 10000) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 0.4),
            font: SysFontType::Emph,
        },

        ([#SIDEBAR_GROUP_BULLET]) (priority = 10000) {
            num_layers: 1,
            #[dyn] layer_img[0]: Some(himg_from_stvg(&assets::LIST_GROUP_OPEN)),
            layer_metrics[0]: Metrics {
                margin: [NAN, NAN, NAN, 4.0],
                size: Vector2::new(12.0, 12.0),
            },
            layer_opacity[0]: 0.5,
        },
        ([#SIDEBAR_GROUP_BULLET.HOVER]) (priority = 11000) {
            layer_opacity[0]: 0.7,
        },
        ([#SIDEBAR_GROUP_BULLET.ACTIVE]) (priority = 12000) {
            layer_opacity[0]: 1.0,
        },
        ([#SIDEBAR_GROUP_BULLET] < [#SIDEBAR_GROUP_HEADER:not(.ACTIVE)]) (priority = 10000) {
            layer_xform[0]: LayerXform {
                rotate: Rad(PI * -0.5),
                ..LayerXform::default()
            },
        },

        ([#SIDEBAR_ITEM]) (priority = 10000) {
            subview_metrics[Role::Generic]: Metrics {
                margin: [NAN, NAN, NAN, 25.0],
                ..Metrics::default()
            },
        },
        ([#SIDEBAR_ITEM.ACTIVE]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(0.3, 0.3, 0.3, 0.25),
        },
        ([#SIDEBAR_ITEM.ACTIVE] .. [#WND.ACTIVE]) (priority = 10500) {
            layer_bg_color[0]: RGBAF32::new(0.3, 0.3, 0.3, 0.5),
        },
        ([#SIDEBAR_ITEM.ACTIVE] .. [.FOCUS]) (priority = 11000) {
            layer_bg_color[0]: RGBAF32::new(0.1, 0.3, 0.6, 0.9),
        },
        ([.LABEL] < [#SIDEBAR_ITEM.ACTIVE]) (priority = 10000) {
            fg_color: RGBAF32::new(1.0, 1.0, 1.0, 1.0),
        },
    }
}

#[cfg(target_os = "windows")]
fn new_custom_platform_stylesheet() -> impl Stylesheet {
    const BORDER_COLOR: RGBAF32 = RGBAF32::new(0.7, 0.7, 0.7, 1.0);
    const BORDER_COLOR_ACT: RGBAF32 = RGBAF32::new(0.5, 0.5, 0.5, 1.0);
    stylesheet! {
        // Add a border around the window
        ([#WND]) (priority = 20000) {
            num_layers: 4,
            layer_bg_color[0]: BORDER_COLOR,
            layer_bg_color[1]: BORDER_COLOR,
            layer_bg_color[2]: BORDER_COLOR,
            layer_bg_color[3]: BORDER_COLOR,
            layer_metrics[0]: Metrics {
                margin: [0.0, 0.0, NAN, 0.0],
                size: Vector2::new(NAN, 1.0),
            },
            layer_metrics[1]: Metrics {
                margin: [1.0, 0.0, 1.0, NAN],
                size: Vector2::new(1.0, NAN),
            },
            layer_metrics[2]: Metrics {
                margin: [NAN, 0.0, 0.0, 0.0],
                size: Vector2::new(NAN, 1.0),
            },
            layer_metrics[3]: Metrics {
                margin: [1.0, NAN, 1.0, 0.0],
                size: Vector2::new(1.0, NAN),
            },
            subview_metrics[Role::Generic]: Metrics {
                margin: [1.0; 4],
                ..Metrics::default()
            },
        },
        ([#WND.ACTIVE]) (priority = 20500) {
            layer_bg_color[0]: BORDER_COLOR_ACT,
            layer_bg_color[1]: BORDER_COLOR_ACT,
            layer_bg_color[2]: BORDER_COLOR_ACT,
            layer_bg_color[3]: BORDER_COLOR_ACT,
        },

        // Remove the rounded corners on Windows
        ([#SEARCH_FIELD]) (priority = 20000) {
            // Focus ring
            #[dyn] layer_img[0]: Some(himg_figures![rect([0.1, 0.4, 0.8, 1.0]).radius(2.0)]),

            // Background fill
            #[dyn] layer_img[1]: Some(himg_figures![
                rect([0.0, 0.0, 0.0, 0.05]),
                rect([0.0, 0.0, 0.0, 0.15]).margin([0.25; 4]).line_width(0.5),
            ]),
        },
        ([#SEARCH_FIELD.FOCUS]) (priority = 20000) {
            // Make the background opaque so that the focus ring really looks
            // like a ring
            #[dyn] layer_img[1]: None,
            layer_bg_color[1]: RGBAF32::new(1.0, 1.0, 1.0, 1.0),
        },
    }
}

#[cfg(not(target_os = "windows"))]
fn new_custom_platform_stylesheet() -> impl Stylesheet {
    stylesheet! {}
}

pub const ENABLE_BACKDROP_BLUR: bool = cfg!(any(target_os = "windows", target_os = "macos"));

pub fn register_stylesheet(manager: &'static Manager) {
    manager.subscribe_new_sheet_set(Box::new(move |_, _, ctx| {
        ctx.insert_stylesheet(new_custom_stylesheet());
        ctx.insert_stylesheet(new_custom_platform_stylesheet());
    }));
    manager.update_sheet_set();
}
