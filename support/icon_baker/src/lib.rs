//! A simple solution for encoding common icon file formats,
//!  such as `.ico` and `.icns`. This crate is mostly a wrapper
//!  for other libraries, unifying existing APIs into a single,
//!  cohesive interface.
//!
//! This crate serves as **[IconPie's](https://github.com/GarkGarcia/icon-pie)**
//!  internal library.
//!
//! # Overview
//!
//! An icon stores a collection of small images of different
//!  sizes. Individual images within the icon are bound to a
//!  source image, which is rescaled to fit a particular size
//!  using a resampling filter.
//!
//! Resampling filters are represented by functions that take
//!  a source image and a size and return a rescaled raw RGBA
//!  buffer. This allows the users of this crate to provide
//!  their custom resampling filters. Common resampling filters
//!  are provided by the `resample` module.
//!
//! # Examples
//!
//! ## General Usage
//! ```rust
//! use icon_baker::*;
//!
//! fn example() -> icon_baker::Result<()> {
//!     let mut icon = Ico::new();
//!
//!     match SourceImage::from_path("image.svg") {
//!         Some(img) => icon.add_entry(resample::linear, &img, 32),
//!         None      => Ok(())
//!     }
//! }
//! ```
//!
//! ## Writing to a File
//! ```rust
//! use icon_baker::*;
//! use std::{io, fs::File};
//!
//! fn example() -> io::Result<()> {
//!     let mut icon = Icns::new();
//!
//!     /* Process the icon */
//!
//!     let mut file = File::create("ou.icns")?;
//!     icon.write(&mut file)
//! }
//! ```
//!
//! # Supported Image Formats
//! | Format | Supported?                                         |
//! | ------ | -------------------------------------------------- |
//! | `PNG`  | All supported color types                          |
//! | `JPEG` | Baseline and progressive                           |
//! | `GIF`  | Yes                                                |
//! | `BMP`  | Yes                                                |
//! | `ICO`  | Yes                                                |
//! | `TIFF` | Baseline(no fax support), `LZW`, PackBits          |
//! | `WEBP` | Lossy(Luma channel only)                           |
//! | `PNM ` | `PBM`, `PGM`, `PPM`, standard `PAM`                |
//! | `SVG`  | Limited(flat filled shapes only)                   |

pub extern crate nsvg;

pub use image::{self, DynamicImage, GenericImage, GenericImageView, RgbaImage};
pub use nsvg::SvgImage;
use std::{
    convert::From,
    error,
    fmt::{self, Display},
    io::{self, Write},
    path::Path,
    result,
};

pub use crate::icns::Icns;
pub use crate::ico::Ico;

pub type Size = u32;
pub type Result<T> = result::Result<T, Error>;

mod icns;
mod ico;
pub mod resample;
#[cfg(test)]
mod test;

const INVALID_SIZE_ERROR: &str = "invalid size supplied to the add_entry method";

/// A generic representation of an icon encoder.
pub trait Icon {
    /// Creates a new icon.
    ///
    /// # Example
    /// ```rust
    /// use icon_baker::{Ico, Icon};
    /// let icon = Ico::new();
    /// ```
    fn new() -> Self;

    /// Adds an individual entry to the icon.
    ///
    /// # Arguments
    /// * `filter` The resampling filter that will be used to re-scale `source`.
    /// * `source` A reference to the source image this entry will be based on.
    /// * `size` The target size of the entry in pixels.
    ///
    /// # Return Value
    /// * Returns `Err(Error::InvalidSize(_))` if the dimensions provided in the
    ///  `size` argument are not supported.
    /// * Returns `Err(Error::Image(ImageError::DimensionError))`
    ///  if the resampling filter provided in the `filter` argument produces
    ///  results of dimensions other than the ones specified by `size`.
    /// * Otherwise return `Ok(())`.
    ///
    /// # Example
    /// ```rust
    /// use icon_baker::*;
    ///
    /// fn main() -> icon_baker::Result<()> {
    ///     let mut icon = Ico::new();
    ///
    ///     match SourceImage::from_path("image.svg") {
    ///         Some(img) => icon.add_entry(resample::linear, &img, 32),
    ///         None      => Ok(())
    ///     }
    /// }
    /// ```
    fn add_entry<F: FnMut(&SourceImage, Size) -> Result<RgbaImage>>(
        &mut self,
        filter: F,
        source: &SourceImage,
        size: Size,
    ) -> Result<()>;

    /// Adds a series of entries to the icon.
    /// # Arguments
    /// * `filter` The resampling filter that will be used to re-scale `source`.
    /// * `source` A reference to the source image this entry will be based on.
    /// * `size` A container for the target sizes of the entries in pixels.
    ///
    /// # Return Value
    /// * Returns `Err(Error::InvalidSize(_))` if the dimensions provided in the
    ///  `size` argument are not supported.
    /// * Returns `Err(Error::Image(ImageError::DimensionError))`
    ///  if the resampling filter provided in the `filter` argument produces
    ///  results of dimensions other than the ones specified by `size`.
    /// * Otherwise return `Ok(())`.
    ///
    /// # Example
    /// ```rust
    /// use icon_baker::*;
    ///
    /// fn main() -> icon_baker::Result<()> {
    ///     let mut icon = Icns::new();
    ///
    ///     match SourceImage::from_path("image.svg") {
    ///         Some(img) => icon.add_entries(
    ///             resample::linear,
    ///             &img,
    ///             vec![32, 64, 128]
    ///         ),
    ///         None => Ok(())
    ///     }
    /// }
    /// ```
    fn add_entries<
        F: FnMut(&SourceImage, Size) -> Result<RgbaImage>,
        I: IntoIterator<Item = Size>,
    >(
        &mut self,
        mut filter: F,
        source: &SourceImage,
        sizes: I,
    ) -> Result<()> {
        for size in sizes.into_iter() {
            self.add_entry(|src, size| filter(src, size), source, size)?;
        }

        Ok(())
    }

    /// Writes the contents of the icon to `w`.
    ///
    /// # Example
    /// ```rust,no_run
    /// use icon_baker::*;
    /// use std::{io, fs::File};
    ///
    /// fn main() -> io::Result<()> {
    ///     let mut icon = Icns::new();
    ///
    ///     /* Process the icon */
    ///
    ///     let mut file = File::create("out.icns")?;
    ///     icon.write(&mut file)
    /// }
    /// ```
    fn write<W: Write>(&mut self, w: &mut W) -> io::Result<()>;
}

/// A representation of a source image.
pub enum SourceImage {
    /// A generic raster image.
    Raster(DynamicImage),
    /// A svg-encoded vector image.
    Svg(SvgImage),
}

#[derive(Debug)]
/// The error type for operations of the `Icon` trait.
pub enum Error {
    /// Error from the `nsvg` crate.
    Nsvg(nsvg::Error),
    /// Error from the `image` crate.
    Image(image::ImageError),
    /// An unsupported size was suplied to an `Icon` operation.
    InvalidSize(Size),
    /// Generic I/O error.
    Io(io::Error),
}

impl SourceImage {
    /// Attempts to create a `SourceImage` from a given path.
    ///
    /// The `SourceImage::from<DynamicImage>` and `SourceImage::from<SvgImage>`
    /// methods should always be preferred.
    ///
    /// # Example
    /// ```no_compile
    /// let img = SourceImage::from_path("source.png");
    /// ```
    pub fn from_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        match image::open(&path) {
            Ok(bit) => Some(SourceImage::Raster(bit)),
            Err(_) => match nsvg::parse_file(path.as_ref(), nsvg::Units::Pixel, 96.0) {
                Ok(svg) => Some(SourceImage::Svg(svg)),
                Err(_) => None,
            },
        }
    }

    /// Returns the width of the original image in pixels.
    pub fn width(&self) -> f32 {
        match self {
            SourceImage::Raster(bit) => bit.width() as f32,
            SourceImage::Svg(svg) => svg.width(),
        }
    }

    /// Returns the height of the original image in pixels.
    pub fn height(&self) -> f32 {
        match self {
            SourceImage::Raster(bit) => bit.height() as f32,
            SourceImage::Svg(svg) => svg.height(),
        }
    }

    /// Returns the dimensions of the original image in pixels.
    pub fn dimensions(&self) -> (f32, f32) {
        (self.width(), self.height())
    }
}

impl From<SvgImage> for SourceImage {
    fn from(svg: SvgImage) -> Self {
        SourceImage::Svg(svg)
    }
}

impl From<DynamicImage> for SourceImage {
    fn from(bit: DynamicImage) -> Self {
        SourceImage::Raster(bit)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Nsvg(err) => write!(f, "{}", err),
            Error::Image(err) => write!(f, "{}", err),
            Error::Io(err) => write!(f, "{}", err),
            Error::InvalidSize(_) => write!(f, "{}", INVALID_SIZE_ERROR),
        }
    }
}

impl error::Error for Error {
    // Do not warn on the uses of `description` because we are just delegating
    // to the inner error object
    #[allow(deprecated)]
    fn description(&self) -> &str {
        match self {
            Error::Nsvg(err) => err.description(),
            Error::Image(err) => err.description(),
            Error::Io(err) => err.description(),
            Error::InvalidSize(_) => INVALID_SIZE_ERROR,
        }
    }

    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::Nsvg(err) => err.source(),
            Error::Image(err) => err.source(),
            Error::Io(ref err) => Some(err),
            Error::InvalidSize(_) => None,
        }
    }
}

impl From<nsvg::Error> for Error {
    fn from(err: nsvg::Error) -> Self {
        Error::Nsvg(err)
    }
}

impl From<image::ImageError> for Error {
    fn from(err: image::ImageError) -> Self {
        Error::Image(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}
