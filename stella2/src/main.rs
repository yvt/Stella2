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

mod config;
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

    // Parse command-line arguments. Exit on parsing error or after displaying
    // a help message.
    let args = config::cmdline::Args::from_env_or_exit();

    // Load the default profile
    let profile = if let Some(profile_path) = &args.profile {
        config::profile::Profile::from_custom_dir(profile_path)
    } else {
        config::profile::Profile::default()
    };
    let profile = Box::leak(Box::new(profile));
    log::info!("Profile: {:?}", profile);
    profile.prepare().unwrap();

    // Prevent multiple instances of the application from running
    let lock_guard = config::lock::try_lock(profile).unwrap();
    if lock_guard.is_none() {
        log::warn!(
            "Exiting because it appears that another application instance \
            using the same profile is already running"
        );
        return;
    }
    std::mem::forget(lock_guard); // let the system do unlocking

    debug!("Initializing WM");
    let wm = pal::Wm::global();

    // Register the application's custom stylesheet
    let style_manager = tcw3::ui::theming::Manager::global(wm);
    stylesheet::register_stylesheet(style_manager);

    let _view = self::view::AppView::new(wm, profile);

    debug!("Entering the main loop");
    wm.enter_main_loop();
}
