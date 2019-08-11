use tcw3::pal::{self, prelude::*};

mod crashhandler;
mod model;
mod stylesheet;
mod view;

fn main() {
    crashhandler::init();

    // Enable logging only in debug builds
    #[cfg(debug_assertions)]
    {
        env_logger::init();
    }

    let wm = pal::Wm::global();

    // Register the application's custom stylesheet
    let style_manager = tcw3::ui::theming::Manager::global(wm);
    stylesheet::register_stylesheet(style_manager);

    let _view = self::view::AppView::new(wm);

    wm.enter_main_loop();
}
