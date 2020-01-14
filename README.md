# ![Stella 2](docs/images/banner.svg)

[![Build Status](https://yvt.visualstudio.com/Stella2/_apis/build/status/yvt.Stella2?branchName=master)](https://yvt.visualstudio.com/Stella2/_build/latest?definitionId=1&branchName=master)

**Work in progress**

This project aims to create a lightweight instant messaging client.

TODO

## Goals

**Important goals:**

- **Security.** Memory safety is the foundation of almost all security features, and it is the great offering of the Rust programming language. However, as many of us have seen, native libraries written in a traditional way tend to be poor in terms of security and stability, thus may go against this goal. But it is also possible that such libraries are already well-audited and battle-tested through a widespread adoption. Even Rust itself is not a panacea against all memory safety issues. Therefore, we should not dismiss practical and lightweight defensive techniques such as stack protector, control-flow integrity, code signing enforcement, address space layout randomization, and OS-level sandboxing.
- **Small memory footprint** and preferring file mapping over anonymous mapping. It's been repeatedly shown that rewriting a system in Rust leads to a lower memory consumption. But it's actually merely a starting point for this project. Short-sighted coding practices such as using `HashMap` for storing measly one or two elements cause unnecessary code bloat and it takes only a few lines of code to cause that. All possibilities should be taken into consideration. Other techniques to reduce the code size include: A plain `static` generates less code than `lazy_static!`. Static drawing commands are better expressed as data than code. Padding a data structure to 2ⁿ bytes decreases the code size because copying can be done in a single instruction. `LinkedList` produces less code than `Vec` for some usage patterns. SIMD instructions not only reduce the μ-ops count and improve the execution speed, but also reduce the code size *by multiple factors*. Bit flags are more space-efficient than `bool` fields even if they are local variables.
- **Cross platform.**
- **Responsive (fast-reacting) UI.** Updating the UI should not be delayed by something that has nothing to do with updating the UI (this is somewhat ill-defined - what if you need some data from disk to render the UI?).
- **Support for handling multilingual texts.** Some scripts are incredibly hard to process, but most desktop environments already include a provision for doing that.

**Goals:**

- **Reasonable user experience.**
- **Reasonable developer experience.**
- **High quality rendering.**
- **Accurate color reproduction.**
- **Low power consumption.** This is related to some of the other goals: Smaller memory footprint leads to lower power consumption because fewer data has to be moved between different levels of the memory hierarchy. Fast-reacting UI correlates with this, but it is also important that simply doing work in a background thread does not reduce the power consumption. This goal ultimately leads to a better user experience because of a prolonged battery lifetime and less frequent thermal throttling of CPU.
- **Space-efficient UI** that doesn't fill the desktop of an 11-inch netbook.
- **UI localization.** Preferably with platform-independent translation data.
- **Resilience against recoverable I/O errors.** Terminating the application should not leave the stored data in an inconsistent state. The disk running out of space should not corrupt the stored data. Graphics driver failure should not disrupt the application's behavior unless it also breaks the target window system.

**Neutral:**

- **Cold build time.** While I care much about the final executable, I don't care very much about the compilation process. It would be nice to get it to compile faster, though.
- **Stable Rust support.**
- **Using innovative and/or awesome technologies.** Other goals shown here should be taken into consideration when choosing technologies and approaches. Nothing should be preferred simply because it's cool. Nothing should be disregarded simply because it's old-fashioned.
- **"Rusty" Rust code.**

**Anti-goals:**

- **Memory imperialism.** We may be tempted to put everything (e.g., a 2D vector graphics library or even the entirety of a web rendering engine) in the executable by static linking and get rid of runtime dependencies. This practice is harmful for several reasons: (1) It prevents individual components from being updated and causes security problems. (2) It increases the total working set of the system because static linking impedes code sharing between processes. Instead, we should make a liberal use of common/system libraries offered by the target platform. We should leverage the target platform's error reporting facility such as CrashReporter (macOS) and `MiniDumpWriteDump` (Windows).
- **Native-looking widgets.** Attempts to imitate the look and feel of each platform's native widgets are prone to end up with being alien to every platform.
- **Writing UI completely separately for every supported platform.** That makes it hard to make changes to UI and to maintain feature parity between platforms.
- **Supporting the web platform.** It kind of defeats the point of writing a native app.
- **Supporting mobile platforms.** Their UI norms are significantly different from those of desktop apps and practically require us to write a separate UI front-end for each kind of platform.
- **Interpreted language.** The bytecode of an interpreted language is more compact than native machine instructions, but it's utterly inefficient to execute, contradicting many of this project's goals. Furthermore, its control flow is defined purely by read/writable data, meaning its not protected by control-flow hardening techniques. Practical interpreted languages usually have an escape hatch such as FFI that enables memory-unsafe operations. This implies interpreted languages can be less secure than compiled languages which are designed for memory safety.

## Directory structure

    stella2
     │
     ├╴ci               CI configuration
     │
     ├╴stella2          The main program
     │
     ├╴stellca2_assets
     │
     ├╴res              Things related to metadata attached to Stella2's executable
     │  │
     │  ├╴mkmacosbundle A command-line utility for creating a macOS application
     │  │               bundle (This program is invoked from `/build-mac.sh`.)
     │  │
     │  └╴windres       Windows resources (application icon, manifest, etc.)
     │
     ├╴tcw3             TCW3, the GUI framework
     │
     ├╴harmony          A state management library
     │
     ├╴stvg             A library for encoding/decoding vector images
     │
     └╴support          An assortment of supporting libraries

## Testing

TCW3 includes a headless backend named `testing`, which is provided for testing purposes. It's disabled by default because of the additional dependencies it introduces. Use the feature flag `tcw3/testing` to enable it:

     cd tcw3
     cargo test --features testing

When testing the whole workspace, specifying features on individual crates won't work ([rust-lang/cargo#6195]), so pass `--all-features` instead:

     cargo test --workspace --all-features

[rust-lang/cargo#6195]: https://github.com/rust-lang/cargo/issues/6195

## Prerequisites

The nightly Rust compiler is required. Depending on when you are reading this, a stable compiler might work.

When building for a Linux system or using TCW3's `testing` backend, dependent crates expect **GLib**, **Cairo**, and **Pango** development files to be installed on your system. You also need **GTK3**, **GDK3**, and **ATK** development files when building for a Linux system.

Fedora:

```shell
sudo yum install glib2-devel cairo-devel cairo-gobject-devel pango-devel \
     gtk3-devel atk-devel
```

Nix:

```shell
# Assumes `cargo` and the nightly toolchain are already available.
nix-shell -p gtk3 pkgconfig --run 'cargo build --release -p stella2'

# Without GTK3 (e.g., when building on macOS):
nix-shell -p glib pango harfbuzz pkgconfig --run 'cargo build --release -p stella2'
```

Windows isn't supported yet, but you can build and run it anyway if you have GTK+ SDK installed on your system. [gtk-rs's Requirements page](https://gtk-rs.org/docs-src/requirements.html) provides an excellent guide on how to configure a development environment for GTK.

## Third-party software

This source tree includes the following third-party projects:

 - (Git subtree) <https://github.com/yvt/alt_fp-rs> at `support/alt_fp`
 - `stvg_macro` is partly based on [Pathfinder 3](https://github.com/servo/pathfinder), licensed by the Pathfinder Project developers under the Apache License, Version 2.0 or the MIT license. Being a procedural macro, it's not included in the final binary.

## License

The project as a whole is licensed under [the GNU General Public License v3.0] or later.

Some subprojects such as TCW3 are licensed under a more liberal license. Some supporting libraries (especially those which are vendored) such as `alt_fp` are considered independent and have their own license, while other libraries are considered as a part of this project and thus licensed under the GPL 3.0+. Please check the `license` field of `Cargo.toml` to find out their license.

[the GNU General Public License v3.0]: https://www.gnu.org/licenses/gpl-3.0.en.html
