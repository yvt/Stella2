# Stella2

TODO

## Directory structutre

    stella2
     ├╴stella2          The main program
     ├╴stella2_assets
     ├╴res              Metadata attached to Stella2's executable
     │  └╴windres       Windows resources (application icon, manifest, etc.)
     ├╴tcw3             TCW3, a GUI framework
     ├╴harmony          A state management library
     ├╴stvg             A library for encoding/decoding vector images
     └╴support          An assortment of supporting libraries

## Prerequisites

The nightly Rust compiler is required. Depending on when you are reading this, a stable compiler might work.

When building for a Linux system, dependent crates expect **GLib**, **Cairo**, and **Pango** development files to be installed on your system.

Fedora:

```shell
sudo yum install glib2-devel cairo-devel cairo-gobject-devel pango-devel
```

## Development Tips

A software-based compositor (`swrast`) is used when other backends are not available for some reason. This is horrendously slow on debug builds and hurts developer experience. For this reason, it's recommended to override the compositor's build option using the [`profile-overrides`] unstable Cargo feature. To use this feature, modify the root `Cargo.toml` as following:

```diff
+cargo-features = ["profile-overrides"]

 [workspace]

      ⋮

 [profile.bench]
 lto = true
 debug = true

+[profile.dev.overrides.tcw3_pal]
+opt-level = 3
```

[`profile-overrides`]: https://doc.rust-lang.org/cargo/reference/unstable.html#profile-overrides

## Third-party software

This source tree includes the following third-party projects:

 - (Git subtree) <https://github.com/yvt/alt_fp-rs> at `support/alt_fp`
 - `stvg_macro` is partly based on [Pathfinder 3](https://github.com/servo/pathfinder), licensed by the Pathfinder Project developers under the Apache License, Version 2.0 or the MIT license.

## License

TBD

