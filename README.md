# Stella2

TODO

## Prerequisites

The nightly Rust compiler is required. Depending on when you are reading this, a stable compiler might work.

When building for a Linux system, dependent crates expect **GLib**, **Cairo**, and **Pango** development files to be installed on your system.

Fedora:

```shell
sudo yum install glib2-devel cairo-devel cairo-gobject-devel pango-devel
```

## Third-party software

This source tree includes the following third-party projects:

 - (Git subtree) <https://github.com/yvt/alt_fp-rs> at `support/alt_fp`
 - `stvg_macro` is partly based on [Pathfinder 3](https://github.com/servo/pathfinder), licensed by the Pathfinder Project developers under the Apache License, Version 2.0 or the MIT license.

## License

TBD

