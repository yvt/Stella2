//! A collection of commonly used resampling filters.

use crate::{Error, Result, Size, SourceImage};
use image::{imageops, DynamicImage, GenericImageView, RgbaImage};
use nsvg::SvgImage;

/// [Linear resampling filter](https://en.wikipedia.org/wiki/Linear_interpolation).
pub fn linear(source: &SourceImage, size: Size) -> Result<RgbaImage> {
    match source {
        SourceImage::Raster(bit) => Ok(scale(bit, size, imageops::FilterType::Triangle).to_rgba()),
        SourceImage::Svg(svg) => svg_linear(svg, size),
    }
}

/// [Lanczos resampling filter](https://en.wikipedia.org/wiki/Lanczos_resampling).
pub fn cubic(source: &SourceImage, size: Size) -> Result<RgbaImage> {
    match source {
        SourceImage::Raster(bit) => Ok(scale(bit, size, imageops::FilterType::Lanczos3).to_rgba()),
        SourceImage::Svg(svg) => svg_linear(svg, size),
    }
}

/// [Nearest-Neighbor resampling filter](https://en.wikipedia.org/wiki/Nearest-neighbor_interpolation).
pub fn nearest(source: &SourceImage, size: Size) -> Result<RgbaImage> {
    match source {
        SourceImage::Raster(bit) => Ok(nearest::resample(bit, size)),
        SourceImage::Svg(svg) => svg_linear(svg, size),
    }
}

mod nearest {
    use super::{overfit, scale};
    use crate::Size;
    use image::{imageops, DynamicImage, GenericImageView, RgbaImage};

    pub fn resample(source: &DynamicImage, size: Size) -> RgbaImage {
        let scaled = if source.width() < size as u32 && source.height() < size as u32 {
            scale_integer(source, size)
        } else {
            scale(source, size, imageops::FilterType::Nearest)
        };

        overfit(&scaled, size)
    }

    fn scale_integer(source: &DynamicImage, size: Size) -> DynamicImage {
        let (w, h) = source.dimensions();

        let scale = if w > h { size / w } else { size / h };
        let (nw, nh) = (w * scale, h * scale);

        DynamicImage::ImageRgba8(imageops::resize(
            source,
            nw,
            nh,
            imageops::FilterType::Nearest,
        ))
    }
}

fn scale(source: &DynamicImage, size: Size, filter: imageops::FilterType) -> DynamicImage {
    let (w, h) = source.dimensions();

    let (nw, nh) = if w > h {
        (size, (size * h) / w)
    } else {
        ((size * w) / h, size)
    };

    DynamicImage::ImageRgba8(imageops::resize(source, nw, nh, filter))
}

fn overfit(source: &DynamicImage, size: Size) -> RgbaImage {
    let mut output = DynamicImage::new_rgba8(size, size);

    let dx = (output.width() - source.width()) / 2;
    let dy = (output.height() - source.height()) / 2;

    imageops::overlay(&mut output, source, dx, dy);
    output.to_rgba()
}

fn svg_linear(source: &SvgImage, size: Size) -> Result<RgbaImage> {
    let (w, h) = (source.width(), source.height());
    let size_f = size as f32;

    let scale = if w > h { size_f / w } else { size_f / h };

    source
        .rasterize_to_raw_rgba(scale)
        .map(|(width, height, data)| {
            Ok(overfit(
                &DynamicImage::ImageRgba8(RgbaImage::from_raw(width, height, data).unwrap()),
                size,
            ))
        })
        .map_err(|err| Error::Nsvg(err))?
}
