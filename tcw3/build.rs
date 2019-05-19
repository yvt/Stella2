fn main() {
    cc::Build::new()
        .file("src/pal/macos/TCWWindowController.m")
        .file("src/pal/macos/TCWWindowView.m")
        .file("src/pal/macos/TCWGestureHandlerView.m")
        .flag("-fobjc-arc")
        .flag("-fobjc-weak")
        .compile("tcwsupport");
}
