use cggeom::box2;
use cgmath::Vector2;
use stella2_assets as assets;
use stvg_tcw3::StvgImg;
use tcw3::{
    images::himg_from_rounded_rect,
    stylesheet,
    ui::theming::{Manager, Metrics, Role, Stylesheet},
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
    }
}

fn new_custom_stylesheet() -> impl Stylesheet {
    use std::f32::NAN;

    // Import IDs (e.g., `#GO_BACK`) into the scope
    use self::elem_id::*;

    const TOOLBAR_IMG_SIZE: Vector2<f32> = Vector2::new(24.0, 16.0);
    const TOOLBAR_IMG_METRICS: Metrics = Metrics {
        margin: [NAN; 4],
        size: TOOLBAR_IMG_SIZE,
    };
    const TOOLBAR_BTN_MIN_SIZE: Vector2<f32> = Vector2::new(30.0, 22.0);

    let himg_from_stvg = |data| StvgImg::new(data).into_himg();

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
            num_layers: 1,
            layer_bg_color[0]: [0.6, 0.6, 0.6, 1.0].into(),
            layer_metrics[0]: Metrics {
                margin: [-100.0, 0.0, 0.0, 0.0],
                ..Default::default()
            },

            subview_metrics[Role::Generic]: Metrics {
                margin: [5.0; 4],
                ..Default::default()
            },
        },

        // Pane background
        ([#SIDEBAR]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: [0.93, 0.93, 0.93, 0.8].into(),

            subview_metrics[Role::Generic]: Metrics {
                margin: [5.0; 4],
                ..Default::default()
            },
        },
        ([#LOG_VIEW]) (priority = 10000) {
            num_layers: 1,
            layer_bg_color[0]: [1.0, 1.0, 1.0, 1.0].into(),

            subview_metrics[Role::Generic]: Metrics {
                margin: [5.0; 4],
                ..Default::default()
            },
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
    }
}

pub fn register_stylesheet(manager: &'static Manager) {
    manager.subscribe_new_sheet_set(Box::new(move |_, _, ctx| {
        ctx.insert_stylesheet(new_custom_stylesheet());
    }));
    manager.update_sheet_set();
}
