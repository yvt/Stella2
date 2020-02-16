with import <nixpkgs> {};
with lib;

runCommand "dummy" rec {
  nativeBuildInputs = [
    rustup pkgconfig
  ];

  buildInputs = [
    # needed by TCW3's testing backend
    glib pango harfbuzz
  ] ++ optionals stdenv.isDarwin (with darwin.apple_sdk.frameworks; [
    CoreText
    Foundation
    AppKit
  ]) ++ optionals (!stdenv.isDarwin) [
    # needed by TCW3's GTK backend
    gtk3
  ];
} ""
