extern crate ico;

use crate::{Error, Icon, Result, Size, SourceImage};
use nsvg::image::{ImageError, RgbaImage};
use std::{
    fmt::{self, Debug, Formatter},
    io::{self, Write},
    result,
};

const MIN_ICO_SIZE: Size = 1;
const MAX_ICO_SIZE: Size = 256;

/// A collection of entries stored in a single `.ico` file.
#[derive(Clone)]
pub struct Ico {
    icon_dir: ico::IconDir,
}

impl Icon for Ico {
    fn new() -> Self {
        Ico {
            icon_dir: ico::IconDir::new(ico::ResourceType::Icon),
        }
    }

    fn add_entry<F: FnMut(&SourceImage, Size) -> Result<RgbaImage>>(
        &mut self,
        mut filter: F,
        source: &SourceImage,
        size: Size,
    ) -> Result<()> {
        if size < MIN_ICO_SIZE || size > MAX_ICO_SIZE {
            return Err(Error::InvalidSize(size));
        }

        let icon = filter(source, size)?;
        if icon.width() != size || icon.height() != size {
            return Err(Error::Image(ImageError::DimensionError));
        }

        let size = icon.width();
        let data = ico::IconImage::from_rgba_data(size, size, icon.into_vec());

        let entry = ico::IconDirEntry::encode(&data).map_err(|err| Error::Io(err))?;
        self.icon_dir.add_entry(entry);

        Ok(())
    }

    fn write<W: Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.icon_dir.write(w)
    }
}

impl Debug for Ico {
    fn fmt(&self, f: &mut Formatter) -> result::Result<(), fmt::Error> {
        let n_entries = self.icon_dir.entries().len();
        let mut entries_str = String::with_capacity(42 * n_entries);

        for _ in 0..n_entries {
            entries_str.push_str("ico::IconDirEntry {{ /* fields omitted */ }}, ");
        }

        let icon_dir = format!(
            "ico::IconDir {{ restype: ico::ResourceType::Icon, entries: [{:?}] }}",
            entries_str
        );

        write!(f, "icon_baker::Ico {{ icon_dir: {} }} ", icon_dir)
    }
}
