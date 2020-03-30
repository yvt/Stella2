extern crate icns;

use crate::{Error, Icon, Result, Size, SourceImage};
use nsvg::image::{ImageError, RgbaImage};
use std::{
    fmt::{self, Debug, Formatter},
    io::{self, Write},
    result,
};

/// A collection of entries stored in a single `.icns` file.
pub struct Icns {
    icon_family: icns::IconFamily,
}

impl Icon for Icns {
    fn new() -> Self {
        Icns {
            icon_family: icns::IconFamily::new(),
        }
    }

    fn add_entry<F: FnMut(&SourceImage, Size) -> Result<RgbaImage>>(
        &mut self,
        mut filter: F,
        source: &SourceImage,
        size: Size,
    ) -> Result<()> {
        let icon = filter(source, size)?;

        // The Image::from_data method only fails when the specified
        // image dimensions do not fit the buffer length
        let image = icns::Image::from_data(icns::PixelFormat::RGBA, size, size, icon.into_vec())
            .map_err(|_| Error::Image(ImageError::DimensionError))?;

        // The IconFamily::add_icon method only fails when the
        // specified image dimensions are not supported by ICNS
        self.icon_family
            .add_icon(&image)
            .map_err(|_| Error::InvalidSize(size))
    }

    fn write<W: Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.icon_family.write(w)
    }
}

impl Clone for Icns {
    fn clone(&self) -> Self {
        let mut icon_family = icns::IconFamily {
            elements: Vec::with_capacity(self.icon_family.elements.len()),
        };

        for element in &self.icon_family.elements {
            let clone = icns::IconElement::new(element.ostype, element.data.clone());

            icon_family.elements.push(clone);
        }

        Icns { icon_family }
    }
}

macro_rules! element {
    ($elm:expr) => {
        format!(
            "IconElement {{ ostype: {:?}, data: {:?} }}",
            $elm.ostype, $elm.data
        )
    };
}

impl Debug for Icns {
    fn fmt(&self, f: &mut Formatter) -> result::Result<(), fmt::Error> {
        let entries_strs: Vec<String> = self
            .icon_family
            .elements
            .iter()
            .map(|element| element!(element))
            .collect();

        let icon_dir = format!(
            "icns::IconFamily {{ elements: [{}] }}",
            entries_strs.join(", ")
        );

        write!(f, "icon_baker::Icns {{ icon_family: {} }} ", icon_dir)
    }
}
