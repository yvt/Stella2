use std::env;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
        cc::Build::new()
            .file("src/macos/TCWWindowController.m")
            .file("src/macos/TCWWindowView.m")
            .file("src/macos/TCWGestureHandlerView.m")
            .file("src/macos/Timers.m")
            .flag("-fobjc-arc")
            .flag("-fobjc-weak")
            .compile("tcwsupport_macos");
    } else if env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        // `CompareObjectHandles` is in `WindowsApp.lib`.
        // <https://github.com/retep998/winapi-rs/issues/781>
        println!("cargo:rustc-link-lib=dylib=WindowsApp");

        assert!(cc::Build::new().get_compiler().is_like_msvc());

        cc::Build::new()
            .file("src/windows/comp.cpp")
            .flag("/std:c++17") // assume MSVC
            .compile("tcwsupport_windows");
    } else {
        // Try to match the settings to that of `gtk-sys`
        let gtk_lib = pkg_config::Config::new()
            .atleast_version("3.14")
            .cargo_metadata(false)
            .probe("gtk+-3.0")
            .unwrap();

        let mut build = cc::Build::new();
        for path in gtk_lib.include_paths.iter() {
            build.include(path);
        }

        build.file("src/gtk/wndwidget.c").compile("tcwsupport_gtk");
    }
}
