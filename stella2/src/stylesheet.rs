use cggeom::box2;
use cgmath::{Deg, Vector2};
use std::f32::NAN;
use stella2_assets as assets;
use stvg_tcw3::StvgImg;
#[allow(unused_imports)]
use tcw3::{
    images::{himg_from_rounded_rect, HImg},
    pal::{LayerFlags, SysFontType},
    stylesheet,
    ui::theming::{LayerXform, Manager, Metrics, Role, Stylesheet},
};

/// Define styling ID values.
pub mod elem_id {
    use tcw3::ui::theming::ClassSet;

    iota::iota! {
        pub const GO_BACK: ClassSet = ClassSet::id(iota + 1);
                , GO_FORWARD
                , SIDEBAR_SHOW
                , SIDEBAR_HIDE

                , SEARCH_FIELD

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

// Import IDs (e.g., `#GO_BACK`) into the scope
use self::elem_id::*;

/// Construct a `HImg` from an StVG image.
fn himg_from_stvg(data: (&'static [u8], [f32; 2])) -> HImg {
    StvgImg::new(data).into_himg()
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
    const TOOLBAR_BTN_MIN_SIZE: Vector2<f32> = Vector2::new(30.0, 22.0);

    stylesheet! {
        ([.SPLITTER]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: [0.85, 0.85, 0.85, 0.8].into(),
            min_size: [1.0, 1.0].into(),
        },
        ([#EDITOR_SPLIT.SPLITTER]) (priority = 10000) {
            min_size: [0.0, 0.0].into(),
        },

        // Toolbar and titlebar background
        ([#TOOLBAR]) (priority = 10000) {
            num_layers: 2,
            layer_bg_color[0]: [0.8, 0.8, 0.8, 1.0].into(),
            layer_metrics[0]: Metrics {
                margin: [-100.0, 0.0, 0.0, 0.0],
                ..Default::default()
            },

            layer_bg_color[1]: [0.3, 0.3, 0.3, 0.35].into(),
            layer_metrics[1]: Metrics {
                margin: [NAN, 0.0, 0.0, 0.0],
                size: [NAN, 0.65].into(),
            },

            subview_metrics[Role::Generic]: Metrics {
                margin: [5.0; 4],
                ..Default::default()
            },
        },
        ([#TOOLBAR] .. [#WND.ACTIVE]) (priority = 10500) {
            layer_bg_color[0]: [0.6, 0.6, 0.6, 1.0].into(),
        },

        // Pane background
        ([#SIDEBAR]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: [0.93, 0.93, 0.93, 1.0].into(),
        },
        // Backdrop blur isn't supported by the GTK backend. The translucent
        // sidebar looks awkward without backdrop blur, so we disable
        // transparency in this case.
        // See also: `self::ENABLE_BACKDROP_BLUR`
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        ([#SIDEBAR] .. [#WND.ACTIVE]) (priority = 10500) {
            layer_bg_color[0]: [0.93, 0.93, 0.93, 0.8].into(),
            layer_flags[0]: LayerFlags::BACKDROP_BLUR,
        },
        ([#LOG_VIEW]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: [1.0, 1.0, 1.0, 1.0].into(),
        },
        ([#EDITOR]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: [0.93, 0.93, 0.93, 1.0].into(),

            subview_metrics[Role::Generic]: Metrics {
                margin: [5.0; 4],
                ..Default::default()
            },
        },

        // Toolbar buttons
        ([#GO_BACK.BUTTON]) (priority = 10000) {
            num_layers: 2,
            layer_img[1]: Some(himg_from_stvg(assets::toolbar::GO_BACK)),
            layer_metrics[1]: TOOLBAR_IMG_METRICS,
            min_size: TOOLBAR_BTN_MIN_SIZE,
        },
        ([#GO_FORWARD.BUTTON]) (priority = 10000) {
            num_layers: 2,
            layer_img[1]: Some(himg_from_stvg(assets::toolbar::GO_FORWARD)),
            layer_metrics[1]: TOOLBAR_IMG_METRICS,
            min_size: TOOLBAR_BTN_MIN_SIZE,
        },
        ([#SIDEBAR_SHOW.BUTTON]) (priority = 10000) {
            num_layers: 2,
            layer_img[1]: Some(himg_from_stvg(assets::toolbar::SIDEBAR_SHOW)),
            layer_metrics[1]: TOOLBAR_IMG_METRICS,
            min_size: TOOLBAR_BTN_MIN_SIZE,
        },
        ([#SIDEBAR_HIDE.BUTTON]) (priority = 10000) {
            num_layers: 2,
            layer_img[1]: Some(himg_from_stvg(assets::toolbar::SIDEBAR_HIDE)),
            layer_metrics[1]: TOOLBAR_IMG_METRICS,
            min_size: TOOLBAR_BTN_MIN_SIZE,
        },

        // Toolbar elements
        ([#SEARCH_FIELD]) (priority = 10000) {
            num_layers: 2,

            layer_img[0]: Some(
                himg_from_rounded_rect([0.0, 0.0, 0.0, 0.2].into(), [[3.0; 2]; 4])
            ),
            layer_center[0]: box2! { point: [0.5, 0.5] },

            layer_img[1]: Some(himg_from_stvg(assets::SEARCH)),
            layer_metrics[1]: Metrics {
                margin: [NAN, NAN, NAN, 4.0],
                size: [16.0, 16.0].into(),
            },

            min_size: [150.0, TOOLBAR_BTN_MIN_SIZE.y].into(),

            subview_metrics[Role::Generic]: Metrics {
                margin: [2.0, 2.0, 2.0, 22.0],
                ..Default::default()
            },
        },
        ([.LABEL] < [#SEARCH_FIELD]) (priority = 10000) {
            fg_color: [1.0, 1.0, 1.0, 0.6].into(),
        },

        // Composing area
        ([#EDITOR_FIELD]) (priority = 10000) {
            num_layers: 1,

            layer_img[0]: Some(
                himg_from_rounded_rect([1.0; 4].into(), [[3.0; 2]; 4])
            ),
            layer_center[0]: box2! { point: [0.5, 0.5] },

            subview_metrics[Role::Generic]: Metrics {
                margin: [3.0; 4],
                ..Default::default()
            },
        },
        ([.LABEL] < [#EDITOR_FIELD]) (priority = 10000) {
            fg_color: [0.0, 0.0, 0.0, 0.4].into(),
        },

        // Sidebar
        ([#SIDEBAR_GROUP_HEADER]) (priority = 10000) {
            // label
            subview_metrics[Role::Generic]: Metrics {
                margin: [NAN, NAN, NAN, 25.0],
                ..Default::default()
            },
            // bullet (open/close)
            subview_metrics[Role::Bullet]: Metrics {
                margin: [NAN, NAN, NAN, 5.0],
                size: [16.0, 16.0].into(),
            },
        },
        ([.LABEL] < [#SIDEBAR_GROUP_HEADER]) (priority = 10000) {
            fg_color: [0.0, 0.0, 0.0, 0.4].into(),
            font: SysFontType::Emph,
        },

        ([#SIDEBAR_GROUP_BULLET]) (priority = 10000) {
            num_layers: 1,
            layer_img[0]: Some(himg_from_stvg(assets::LIST_GROUP_OPEN)),
            layer_metrics[0]: Metrics {
                margin: [NAN, NAN, NAN, 4.0],
                size: [12.0, 12.0].into(),
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
                rotate: Deg(-90.0).into(),
                ..Default::default()
            },
        },

        ([#SIDEBAR_ITEM]) (priority = 10000) {
            subview_metrics[Role::Generic]: Metrics {
                margin: [NAN, NAN, NAN, 25.0],
                ..Default::default()
            },
        },
        ([#SIDEBAR_ITEM.ACTIVE]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: [0.3, 0.3, 0.3, 0.3].into(),
        },
        ([#SIDEBAR_ITEM.ACTIVE] .. [#WND.ACTIVE]) (priority = 10500) {
            layer_bg_color[0]: [0.1, 0.3, 0.6, 0.9].into(),
        },
        ([.LABEL] < [#SIDEBAR_ITEM.ACTIVE]) (priority = 10000) {
            fg_color: [1.0, 1.0, 1.0, 1.0].into(),
        },
    }
}

#[cfg(target_os = "windows")]
fn new_custom_platform_stylesheet() -> impl Stylesheet {
    stylesheet! {
        ([.BUTTON:not(.HOVER)]) (priority = 20000) {
            layer_opacity[0]: 0.3,
        },
        ([.BUTTON]) (priority = 20000) {
            layer_bg_color[0]: [0.7, 0.7, 0.7, 1.0].into(),
            layer_img[0]: None,
        },
        ([.BUTTON.ACTIVE]) (priority = 21000) {
            layer_bg_color[0]: [0.2, 0.4, 0.9, 1.0].into(),
            layer_img[0]: None,
        },

        ([#TOOLBAR]) (priority = 20000) {
            layer_bg_color[0]: [1.0; 4].into(),
        },

        ([#SEARCH_FIELD]) (priority = 20000) {
            num_layers: 3,

            layer_img[0]: None,
            layer_bg_color[0]: [0.0, 0.0, 0.0, 0.2].into(),
            layer_img[1]: None,
            layer_bg_color[1]: [1.0; 4].into(),
            layer_metrics[1]: Metrics {
                margin: [1.0, 1.0, 1.0, 1.0],
                ..Default::default()
            },
            layer_center[1]: box2! { point: [0.5, 0.5] },

            layer_img[2]: Some(himg_from_stvg_col(assets::SEARCH, [0.4, 0.4, 0.4, 1.0].into())),
            layer_metrics[2]: Metrics {
                margin: [NAN, NAN, NAN, 4.0],
                size: [16.0, 16.0].into(),
            },
        },
        ([.LABEL] < [#SEARCH_FIELD]) (priority = 20000) {
            fg_color: [0.0, 0.0, 0.0, 0.6].into(),
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
