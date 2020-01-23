// Windows-specific: Set the subsystem flag to `windows` (from the default
// value `console`). This prevents a new console window from opening on
// application launch.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
// When never type (`!`) is stabilized, `msg_send![ ... ];` will be no longer
// deduced to `()`. Thus a call to `msg_send!` needs a unit value binding
#![allow(clippy::let_unit_value)]
#![allow(clippy::float_cmp)]

use log::debug;
use tcw3::pal::{self, prelude::*};

mod crashhandler;
mod model;
mod stylesheet;
mod view;

#[cfg(target_os = "windows")]
mod windres {
    stella2_windres::attach_windres!();
}

fn main() {
    crashhandler::init();

    // Enable logging only in debug builds
    #[cfg(debug_assertions)]
    {
        #[cfg(target_os = "windows")]
        {
            use std::str::FromStr;

            let cfg = std::env::var("RUST_LOG").ok();
            let cfg = cfg.as_deref().unwrap_or("info");
            if let Ok(level) = log::Level::from_str(&cfg) {
                windebug_logger::init_with_level(level).unwrap();
            } else {
                windebug_logger::init_with_level(log::Level::Info).unwrap();
                log::warn!(
                    "Invalid log level was specified by `RUST_LOG` ({:>}). \
                     Defaulting to `info`",
                    cfg
                );
            }
        }

        #[cfg(not(target_os = "windows"))]
        env_logger::init();
    }

    log::info!("Logging started");

    // Platform-specific initialization
    #[cfg(target_os = "windows")]
    unsafe {
        use std::ptr::null_mut;
        use winapi::um::{libloaderapi, winuser};

        // Register `IDI_ICON` (defined in `stella2.rc`) as the application icon
        let hinstance = libloaderapi::GetModuleHandleW(null_mut());
        pal::windows::set_app_hicon(winuser::LoadIconW(hinstance, 0x101 as _));
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
