# IconBaker
[![Crate](https://img.shields.io/crates/v/icon_baker.svg)](https://crates.io/crates/icon_baker)
[![API](https://docs.rs/icon_baker/badge.svg)](https://docs.rs/icon_baker)
[![Minimum rustc version](https://img.shields.io/badge/rustc-1.32+-lightgray.svg)](https://github.com/rust-random/rand#rust-version-requirements)

A simple solution for encoding common icon file formats, such as `.ico` and `.icns`. This crate is mostly a wrapper for other libraries, unifying existing APIs into a single, cohesive interface.

This crate serves as **[IconPie's](https://github.com/GarkGarcia/icon-pie)** internal library.

## Overview

An icon stores a collection of small images of different sizes. Individual images within the icon are bound to a source image, which is rescaled to fit a particular size using a resampling filter.

Resampling filters are represented by functions that take a source image and a size and return a rescaled raw RGBA buffer. This allows the users of this crate to provide their custom resampling filters. Common resampling filters are provided by the `resample` module.

## Examples

### General Usage
```rust
use icon_baker::*;
 
fn example() -> icon_baker::Result<()> {
    let icon = Ico::new();

    match SourceImage::from_path("image.svg") {
        Some(img) => icon.add_entry(resample::linear, &img, 32),
        None      => Ok(())
    }
}
```

### Writing to a File
```rust
use icon_baker::*;
use std::{io, fs::File};
 
fn example() -> io::Result<()> {
    let icon = PngSequence::new();

    /* Process the icon */

    let file = File::create("ou.icns")?;
    icon.write(file)
}
```

## Limitations
There are two main limitations in this crate: both `ICNS` and `SVG` are not fully supported. Due to the use of external dependencies, the author of this crate is not able to fully support the formal specifications of those two file formats.

However, the coverage provided by these external dependencies should be enough for most use cases.

### Supported Image Formats
| Format | Supported?                                         | 
| ------ | -------------------------------------------------- | 
| `PNG`  | All supported color types                          | 
| `JPEG` | Baseline and progressive                           | 
| `GIF`  | Yes                                                | 
| `BMP`  | Yes                                                | 
| `ICO`  | Yes                                                | 
| `TIFF` | Baseline(no fax support), `LZW`, PackBits          | 
| `WEBP` | Lossy(Luma channel only)                           | 
| `PNM ` | `PBM`, `PGM`, `PPM`, standard `PAM`                |
| `SVG`  | Limited(flat filled shapes only)                   |

### ICNS Support

**Icon Baker** uses the `icns` crate for generating `.icns` files. The [supported icon types](https://github.com/mdsteele/rust-icns/blob/master/README.md#supported-icon-types) are specified by the creators of such crate as follows:

| OSType | Description                             | Supported? |
|--------|-----------------------------------------|------------|
| `ICON` | 32×32 1-bit icon                        | No         |
| `ICN#` | 32×32 1-bit icon with 1-bit mask        | No         |
| `icm#` | 16×12 1-bit icon with 1-bit mask        | No         |
| `icm4` | 16×12 4-bit icon                        | No         |
| `icm8` | 16×12 8-bit icon                        | No         |
| `ics#` | 16×16 1-bit mask                        | No         |
| `ics4` | 16×16 4-bit icon                        | No         |
| `ics8` | 16x16 8-bit icon                        | No         |
| `is32` | 16×16 24-bit icon                       | Yes        |
| `s8mk` | 16x16 8-bit mask                        | Yes        |
| `icl4` | 32×32 4-bit icon                        | No         |
| `icl8` | 32×32 8-bit icon                        | No         |
| `il32` | 32x32 24-bit icon                       | Yes        |
| `l8mk` | 32×32 8-bit mask                        | Yes        |
| `ich#` | 48×48 1-bit mask                        | No         |
| `ich4` | 48×48 4-bit icon                        | No         |
| `ich8` | 48×48 8-bit icon                        | No         |
| `ih32` | 48×48 24-bit icon                       | Yes        |
| `h8mk` | 48×48 8-bit mask                        | Yes        |
| `it32` | 128×128 24-bit icon                     | Yes        |
| `t8mk` | 128×128 8-bit mask                      | Yes        |
| `icp4` | 16x16 32-bit PNG/JP2 icon               | PNG only   |
| `icp5` | 32x32 32-bit PNG/JP2 icon               | PNG only   |
| `icp6` | 64x64 32-bit PNG/JP2 icon               | PNG only   |
| `ic07` | 128x128 32-bit PNG/JP2 icon             | PNG only   |
| `ic08` | 256×256 32-bit PNG/JP2 icon             | PNG only   |
| `ic09` | 512×512 32-bit PNG/JP2 icon             | PNG only   |
| `ic10` | 512x512@2x "retina" 32-bit PNG/JP2 icon | PNG only   |
| `ic11` | 16x16@2x "retina" 32-bit PNG/JP2 icon   | PNG only   |
| `ic12` | 32x32@2x "retina" 32-bit PNG/JP2 icon   | PNG only   |
| `ic13` | 128x128@2x "retina" 32-bit PNG/JP2 icon | PNG only   |
| `ic14` | 256x256@2x "retina" 32-bit PNG/JP2 icon | PNG only   |

### SVG Support

**IconBaker** uses the `nsvg` crate to rasterize `.svg` files. According to the authors of the crate:

> Like NanoSVG, the rasterizer only renders flat filled shapes. It is not particularly fast or accurate, but it is a simple way to bake vector graphics into textures.

The author of `icon_baker` is inclined to search for alternatives to `nsvg` if inquired to. Help would be appreciated. 

## License

Licensed under MIT license([LICENSE-MIT](https://github.com/GarkGarcia/icon_baker/blob/master/LICENSE) or http://opensource.org/licenses/MIT).

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you shall be licensed as above, without any additional terms or conditions.
