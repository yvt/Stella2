fn main() {
    cc::Build::new()
        .file("src/pal/macos/TCWWindow.m")
        .compile("tcwsupport");
}
