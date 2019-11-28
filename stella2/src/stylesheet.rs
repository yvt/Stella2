use cgmath::Vector2;
use stella2_assets as assets;
use stvg_tcw3::StvgImg;
use tcw3::{
    stylesheet,
    ui::theming::{Manager, Metrics, Stylesheet},
};

/// Define styling ID values.
pub mod elem_id {
    use tcw3::ui::theming::ClassSet;

    pub const GO_BACK: ClassSet = ClassSet::id(1);
    pub const GO_FORWARD: ClassSet = ClassSet::id(2);
    pub const SIDEBAR_SHOW: ClassSet = ClassSet::id(3);
    pub const SIDEBAR_HIDE: ClassSet = ClassSet::id(4);
}

fn new_custom_stylesheet() -> impl Stylesheet {
    // Import IDs (e.g., `#GO_BACK`) into the scope
    use self::elem_id::*;

    const TOOLBAR_IMG_SIZE: Vector2<f32> = Vector2::new(24.0, 16.0);
    const TOOLBAR_IMG_METRICS: Metrics = Metrics {
        margin: [std::f32::NAN; 4],
        size: TOOLBAR_IMG_SIZE,
    };
    const TOOLBAR_BTN_MIN_SIZE: Vector2<f32> = Vector2::new(30.0, 20.0);

    let himg_from_stvg = |data| StvgImg::new(data).into_himg();

    stylesheet! {
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
    }
}

pub fn register_stylesheet(manager: &'static Manager) {
    manager.subscribe_new_sheet_set(Box::new(move |_, _, ctx| {
        ctx.insert_stylesheet(new_custom_stylesheet());
    }));
    manager.update_sheet_set();
}
