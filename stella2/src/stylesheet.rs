use cggeom::box2;
use cgmath::{Rad, Vector2};
use std::f32::{consts::PI, NAN};
use stella2_assets as assets;
#[allow(unused_imports)]
use tcw3::{
    images::{himg_figures, HImg},
    pal::{LayerFlags, SysFontType, RGBAF32},
    stvg::StvgImg,
    stylesheet,
    ui::theming::{roles, LayerXform, Manager, Metrics, Stylesheet},
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

                , TABBAR
                , TABBAR_TAB
                , TABBAR_TAB_CLOSE
                , TABBAR_CLOSE

                , PREF_HEADER
                , PREF_MAIN
                , PREF_TAB_BAR
                , PREF_TAB_GENERAL
                , PREF_TAB_ACCOUNTS
                , PREF_TAB_CONNECTION
                , PREF_TAB_ADVANCED
                , PREF_TAB_ABOUT

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
fn himg_from_stvg_col(data: (&'static [u8], [f32; 2]), c: tcw3::pal::RGBAF32) -> HImg {
    StvgImg::new(data)
        .with_color_xform(tcw3::stvg::replace_color(c))
        .into_himg()
}

/// Construct a `HImg` for a tab icon.
fn himg_tab_icon(data: &(&'static [u8], [f32; 2])) -> HImg {
    himg_from_stvg_col(*data, [0.0, 0.0, 0.0, 1.0].into())
}

/// Construct a `HImg` for an active tab icon.
fn himg_tab_icon_act(data: &(&'static [u8], [f32; 2])) -> HImg {
    himg_from_stvg_col(*data, [0.2, 0.5, 0.9, 1.0].into())
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

            subview_metrics[roles::GENERIC]: Metrics {
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
        ([#TABBAR]) (priority = 10000) {
            num_layers: 2,
            layer_bg_color[0]: RGBAF32::new(0.93, 0.93, 0.93, 1.0),

            layer_bg_color[1]: RGBAF32::new(0.0, 0.0, 0.0, 0.13),
            layer_metrics[1]: Metrics {
                margin: [NAN, 0.0, -0.5, 0.0],
                size: Vector2::new(NAN, 1.35),
            },
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
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        ([#TABBAR] .. [#WND.ACTIVE]) (priority = 10500) {
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

            subview_metrics[roles::GENERIC]: Metrics {
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

        // Tabbar
        ([#TABBAR_TAB]) (priority = 10000) {
            num_layers: 3,

            // Shadow
            layer_bg_color[0]: RGBAF32::new(0.0, 0.0, 0.0, 0.4),
            layer_metrics[0]: Metrics {
                margin: [0.0, -0.5, 0.0, -0.5],
                ..Metrics::default()
            },
            layer_opacity[0]: 0.0,

            // Face
            layer_bg_color[1]: RGBAF32::new(0.95, 0.95, 0.95, 1.0),
            layer_opacity[1]: 0.0,
            layer_metrics[1]: Metrics {
                // Make sure no gap between the tabbar and the toolbar
                margin: [0.0, 0.0, -1.0, 0.0],
                ..Metrics::default()
            },

            // Highlight color
            layer_bg_color[2]: RGBAF32::new(0.63, 0.07, 0.93, 1.0),
            layer_opacity[2]: 0.0,
            layer_metrics[2]: Metrics {
                margin: [0.0, 0.0, NAN, 0.0],
                size: Vector2::new(NAN, 3.0),
            },

            allow_grow: [false, false],
            min_size: Vector2::new(0.0, 32.0),

            // Label
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [NAN, 32.0, NAN, 10.0],
                ..Metrics::default()
            },

            // Close button
            subview_metrics[roles::BULLET]: Metrics {
                margin: [NAN, 7.0, NAN, NAN],
                ..Metrics::default()
            },
        },
        ([#TABBAR_TAB] .. [#WND.ACTIVE]) (priority = 10500) {
            layer_bg_color[1]: RGBAF32::new(0.9, 0.9, 0.9, 1.0),
        },
        ([#TABBAR_TAB.ACTIVE]) (priority = 10500) {
            layer_opacity[0]: 1.0,
            layer_opacity[1]: 1.0,
            layer_opacity[2]: 1.0,
        },
        ([#TABBAR_TAB.HOVER:not(.ACTIVE)]) (priority = 10500) {
            layer_bg_color[1]: RGBAF32::new(0.0, 0.0, 0.0, 0.1),
            layer_opacity[1]: 1.0,
        },

        ([#TABBAR_TAB_CLOSE]) (priority = 10000) {
            num_layers: 2,
            layer_bg_color[0]: RGBAF32::new(0.0, 0.0, 0.0, 1.0),
            layer_opacity[0]: 0.0,
            #[dyn] layer_img[1]: Some(
                himg_from_stvg_col(assets::CLOSE, [0.3, 0.3, 0.3, 1.0].into()),
            ),
            layer_metrics[1]: Metrics {
                margin: [NAN; 4],
                size: Vector2::new(16.0, 16.0),
            },
            min_size: Vector2::new(16.0, 16.0),
            allow_grow: [false, false],
        },
        ([#TABBAR_TAB_CLOSE.HOVER]) (priority = 10500) {
            layer_opacity[0]: 0.1,
        },
        ([#TABBAR_TAB_CLOSE.HOVER.ACTIVE]) (priority = 10500) {
            layer_opacity[0]: 0.2,
        },

        #[cfg(not(target_os = "macos"))]
        ([#TABBAR_CLOSE]) (priority = 10000) {
            num_layers: 2,
            layer_bg_color[0]: RGBAF32::new(0.9, 0.06, 0.14, 1.0),
            layer_opacity[0]: 0.0,
            #[dyn] layer_img[1]: Some(
                himg_from_stvg_col(assets::CLOSE, [0.3, 0.3, 0.3, 1.0].into()),
            ),
            layer_metrics[1]: Metrics {
                margin: [NAN; 4],
                size: Vector2::new(16.0, 16.0),
            },
            min_size: Vector2::new(48.0, 0.0),
            allow_grow: [false, true],
        },
        #[cfg(not(target_os = "macos"))]
        ([#TABBAR_CLOSE.HOVER]) (priority = 10500) {
            layer_opacity[0]: 1.0,
            #[dyn] layer_img[1]: Some(
                himg_from_stvg_col(assets::CLOSE, [1.0, 1.0, 1.0, 1.0].into()),
            ),
        },
        #[cfg(not(target_os = "macos"))]
        ([#TABBAR_CLOSE.HOVER.ACTIVE]) (priority = 10500) {
            layer_bg_color[0]: RGBAF32::new(1.0, 0.5, 0.5, 1.0),
        },

        // Search field
        ([#SEARCH_FIELD_WRAP]) (priority = 10000) {
            subview_metrics[roles::GENERIC]: Metrics {
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

            subview_metrics[roles::GENERIC]: Metrics {
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

            subview_metrics[roles::GENERIC]: Metrics {
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
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [NAN, NAN, NAN, 25.0],
                ..Metrics::default()
            },
            // bullet (open/close)
            subview_metrics[roles::BULLET]: Metrics {
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
            subview_metrics[roles::GENERIC]: Metrics {
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

        // -------------------------------------------------------------------
        // "Preferences" window

        // Header region
        ([#PREF_HEADER]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(0.93, 0.93, 0.93, 1.0),
            layer_metrics[0]: Metrics {
                // fill the gap between `#PREF_HEADER` and `#PREF_MAIN`
                margin: [0.0, 0.0, -1.0, 0.0],
                ..Metrics::default()
            },
            min_size: Vector2::new(500.0, 20.0),
        },
        // Backdrop blur isn't supported by the GTK backend. The translucent
        // region looks awkward without backdrop blur, so we disable
        // transparency in this case.
        // See also: `self::ENABLE_BACKDROP_BLUR`
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        ([#PREF_HEADER] .. [#WND.ACTIVE]) (priority = 10500) {
            layer_bg_color[0]: RGBAF32::new(0.93, 0.93, 0.93, 0.8),
            layer_flags[0]: LayerFlags::BACKDROP_BLUR,
        },

        // Main region background
        ([#PREF_MAIN]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(0.95, 0.95, 0.95, 1.0),
            min_size: Vector2::new(0.0, 200.0),
        },

        // All tabs - each of them has something like `PREF_TAB_GENERAL`
        //  - Default: Uncolored, 60% opacity
        //  - Hover: Default + 80% opacity
        //  - Active: Default + 100% opacity
        //  - Selected: Colored, 100% opacity, colored underline
        //  - Default + inactive window: Uncolored, 40% opacity
        //  - Default + inactive window: Uncolored, 60% opacity, grayed underline
        ([] < [#PREF_TAB_BAR]) (priority = 10000) {
            num_layers: 2,

            // An underline (displayed only when active)
            layer_bg_color[0]: RGBAF32::new(0.2, 0.5, 0.9, 1.0),
            layer_opacity[0]: 0.0,
            layer_metrics[0]: Metrics {
                margin: [NAN, 12.0, 0.0, 12.0],
                size: Vector2::new(NAN, 2.0),
            },

            // layer[1] is used for displaying an icon
            layer_metrics[1]: Metrics {
                margin: [11.0, NAN, NAN, NAN],
                size: Vector2::new(26.0, 26.0),
            },
            layer_opacity[1]: 0.6,

            // Label (`[.LABEL] .. [#PREF_TAB_BAR]`)
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [45.0, NAN, 8.0, NAN],
                ..Metrics::default()
            },

            min_size: Vector2::new(80.0, 0.0),
        },
        ([.HOVER] < [#PREF_TAB_BAR]) (priority = 10100) {
            layer_opacity[1]: 0.8,
        },
        ([.ACTIVE] < [#PREF_TAB_BAR]) (priority = 10200) {
            layer_opacity[1]: 1.0,
        },
        ([.CHECKED] < [#PREF_TAB_BAR]) (priority = 10300) {
            // Display the underline
            layer_opacity[0]: 1.0,
            layer_opacity[1]: 1.0,
        },

        ([] < [#PREF_TAB_BAR] .. [#WND:not(.ACTIVE)]) (priority = 10400) {
            // The window is inactive, so gray out the underline
            layer_bg_color[0]: RGBAF32::new(0.0, 0.0, 0.0, 0.5),
            layer_opacity[1]: 0.4,
        },
        ([.CHECKED] < [#PREF_TAB_BAR] .. [#WND:not(.ACTIVE)]) (priority = 10400) {
            layer_opacity[1]: 0.6,
        },

        ([.LABEL] .. [] < [#PREF_TAB_BAR]) (priority = 10000) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 0.6),
            font: SysFontType::Small,
        },
        ([.LABEL] .. [.HOVER] < [#PREF_TAB_BAR]) (priority = 10100) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 0.8),
        },
        ([.LABEL] .. [.ACTIVE] < [#PREF_TAB_BAR]) (priority = 10200) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 1.0),
        },
        ([.LABEL] .. [.CHECKED] < [#PREF_TAB_BAR]) (priority = 10300) {
            fg_color: RGBAF32::new(0.2, 0.5, 0.9, 1.0),
        },

        ([.LABEL] .. [] < [#PREF_TAB_BAR] .. [#WND:not(.ACTIVE)]) (priority = 10400) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 0.4),
        },
        ([.LABEL] .. [.CHECKED] < [#PREF_TAB_BAR] .. [#WND:not(.ACTIVE)]) (priority = 10400) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 0.6),
        },

        // Individual tabs
        ([#PREF_TAB_GENERAL] /* < [#PREF_TAB_BAR] */) (priority = 10000) {
            #[dyn] layer_img[1]: Some(himg_tab_icon(&assets::pref::TAB_GENERAL)),
        },
        ([#PREF_TAB_GENERAL.CHECKED] /* < [#PREF_TAB_BAR] */ .. [#WND.ACTIVE]) (priority = 10300) {
            #[dyn] layer_img[1]: Some(himg_tab_icon_act(&assets::pref::TAB_GENERAL)),
        },

        ([#PREF_TAB_ACCOUNTS] /* < [#PREF_TAB_BAR] */) (priority = 10000) {
            #[dyn] layer_img[1]: Some(himg_tab_icon(&assets::pref::TAB_ACCOUNTS)),
        },
        ([#PREF_TAB_ACCOUNTS.CHECKED] /* < [#PREF_TAB_BAR] */ .. [#WND.ACTIVE]) (priority = 10300) {
            #[dyn] layer_img[1]: Some(himg_tab_icon_act(&assets::pref::TAB_ACCOUNTS)),
        },

        ([#PREF_TAB_CONNECTION] /* < [#PREF_TAB_BAR] */) (priority = 10000) {
            #[dyn] layer_img[1]: Some(himg_tab_icon(&assets::pref::TAB_CONNECTION)),
        },
        ([#PREF_TAB_CONNECTION.CHECKED] /* < [#PREF_TAB_BAR] */ .. [#WND.ACTIVE]) (priority = 10300) {
            #[dyn] layer_img[1]: Some(himg_tab_icon_act(&assets::pref::TAB_CONNECTION)),
        },

        ([#PREF_TAB_ADVANCED] /* < [#PREF_TAB_BAR] */) (priority = 10000) {
            #[dyn] layer_img[1]: Some(himg_tab_icon(&assets::pref::TAB_ADVANCED)),
        },
        ([#PREF_TAB_ADVANCED.CHECKED] /* < [#PREF_TAB_BAR] */ .. [#WND.ACTIVE]) (priority = 10300) {
            #[dyn] layer_img[1]: Some(himg_tab_icon_act(&assets::pref::TAB_ADVANCED)),
        },

        ([#PREF_TAB_ABOUT] /* < [#PREF_TAB_BAR] */) (priority = 10000) {
            #[dyn] layer_img[1]: Some(himg_tab_icon(&assets::pref::TAB_ABOUT)),
        },
        ([#PREF_TAB_ABOUT.CHECKED] /* < [#PREF_TAB_BAR] */ .. [#WND.ACTIVE]) (priority = 10300) {
            #[dyn] layer_img[1]: Some(himg_tab_icon_act(&assets::pref::TAB_ABOUT)),
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
            subview_metrics[roles::GENERIC]: Metrics {
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
