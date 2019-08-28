use std::env;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
        #[cfg(not(feature = "macos_winit"))]
        {
            cc::Build::new()
                .file("src/macos/TCWWindowController.m")
                .file("src/macos/TCWWindowView.m")
                .file("src/macos/TCWGestureHandlerView.m")
                .flag("-fobjc-arc")
                .flag("-fobjc-weak")
                .compile("tcwsupport");
        }

        #[cfg(feature = "macos_winit")]
        {
            cc::Build::new()
                .file("src/macos/TCWWinitView.m")
                .flag("-fobjc-arc")
                .flag("-fobjc-weak")
                .compile("tcwsupport_winit");
        }
    }
}
