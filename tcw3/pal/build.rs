fn main() {
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
}
