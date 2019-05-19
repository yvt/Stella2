fn main() {
    cc::Build::new()
        .file("src/pal/macos/TCWWindow.m")
        .file("src/pal/macos/TCWGestureHandlerView.m")
        .compile("tcwsupport");
}
