extern crate tar;
extern crate nsvg;

use crate::{Icon, SourceImage, Size, Result, Error};
use std::{io::{self, Write}, path::Path, collections::{HashMap, BTreeSet}};
use nsvg::image::{png::PNGEncoder, RgbaImage, ImageError, ColorType};

const MIN_PNG_SIZE: Size = 1;
const STD_CAPACITY: usize = 7;

/// A collection of images stored in a single `.tar` file.
#[derive(Clone, Debug)]
pub struct FavIcon {
    images: HashMap<Size, BTreeSet<Vec<u8>>>
}

impl Icon for FavIcon {
    fn new() -> Self {
        FavIcon { images: HashMap::with_capacity(STD_CAPACITY) }
    }

    fn add_entry<F: FnMut(&SourceImage, Size) -> Result<RgbaImage>>(
        &mut self,
        mut filter: F,
        source: &SourceImage,
        size: Size
    ) -> Result<()> {
        if size < MIN_PNG_SIZE {
            return Err(Error::InvalidSize(size));
        }

        let icon = filter(source, size)?;
        if icon.width() != size || icon.height() != size {
            return Err(Error::Image(ImageError::DimensionError));
        }
    
        // Encode the pixel data as PNG and store it in a Vec<u8>
        let mut data = Vec::with_capacity(icon.len());
        let encoder = PNGEncoder::new(&mut data);
        encoder.encode(&icon.into_raw(), size, size, ColorType::RGBA(8))
            .map_err(|err| Error::Io(err))?;

        self.images.entry(size).or_default().insert(data);
        Ok(())
    }

    fn write<W: Write>(&mut self, w: &mut W) -> io::Result<()> {
        let mut tar_builder = tar::Builder::new(w);
        let mut xml: Vec<u8> = Vec::with_capacity(63 * self.images.len() + 54);

        write!(xml, "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n")?;

        for (size, images) in &self.images {
            if images.len() == 1 {
                let path = format!("./icons/favicon-{0}x{0}.png", size);
                write!(xml, "<link rel=\"icon\" href={0} sizes\"{1}x{1}\">\n", path, size)?;

                for image in images { append_data(&mut tar_builder, image, path)?; break; }
            } else {
                let mut c = 0;

                for image in images {
                    let path = format!("./icons/favicon-{}@{}.png", size, c);

                    write!(
                        xml,
                        "<link rel=\"icon\" href={0} sizes\"{1}x{1}\">",
                        path, size
                    )?;

                    append_data(&mut tar_builder, image, path)?;
                    c += 1;
                }
            }
        }
        
        append_data(&mut tar_builder, &xml, "./favicon.xml")
    }
}

#[inline]
fn append_data<W: Write, P: AsRef<Path>>(
    builder: &mut tar::Builder<W>,
    data: &Vec<u8>,
    path: P
) -> io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_cksum();

    builder.append_data::<_, &[u8]>(&mut header, path, data.as_ref())
}
