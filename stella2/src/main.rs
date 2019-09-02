// Windows-specific: Set the subsystem flag to `windows` (from the default
// value `console`). This prevents a new console window from opening on
// application launch.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use log::debug;
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

    debug!("Initializing WM");
    let wm = pal::Wm::global();

    // Register the application's custom stylesheet
    let style_manager = tcw3::ui::theming::Manager::global(wm);
    stylesheet::register_stylesheet(style_manager);

    let _view = self::view::AppView::new(wm);

    debug!("Entering the main loop");
    wm.enter_main_loop();
}
