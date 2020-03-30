use crate::*;
use std::{fs::File, io::BufWriter};
use nsvg::image::{png::PNGEncoder, ColorType};

macro_rules! png {
    ($r: expr, $s: expr, $w:expr) => {
        match $r(&$s, 32) {
            Ok(scaled) => {
                let (w, h) = scaled.dimensions();
                let encoder = PNGEncoder::new($w);

                encoder.encode(&scaled.into_raw(), w, h, ColorType::RGBA(8))
                    .expect("Could not encode or save the png output");
            },
            Err(err) => panic!("{:?}", err)
        }
    };
}

#[test]
fn test_resample() {
    let mut file_near   = File::create("tests/test_near.png")
        .expect("Couldn't create file");

    let mut file_linear = File::create("tests/test_linear.png")
        .expect("Couldn't create file");

    let mut file_cubic  = File::create("tests/test_cubic.png")
        .expect("Couldn't create file");

    let img = SourceImage::from_path("tests/hydra.png")
        .expect("File not found");

    png!(resample::nearest, &img, &mut file_near);
    png!(resample::linear , &img, &mut file_linear);
    png!(resample::cubic  , &img, &mut file_cubic);
}

#[test]
fn test_ico() {
    let mut file = BufWriter::new(File::create("tests/test.ico")
        .expect("Couldn't create file"));

    let mut icon = Ico::new();
    let img = SourceImage::from_path("tests/hydra.png")
        .expect("File not found");

    if let Err(err) = icon.add_entries(resample::nearest, &img, vec![32, 64]) {
        panic!("{:?}", err);
    }

    if let Err(err) = icon.add_entry(resample::nearest, &img, 128) {
        panic!("{:?}", err);
    }

    if let Err(err) = icon.add_entry(resample::nearest, &img, 32) {
        panic!("{:?}", err);
    }

    if let Err(err) = icon.write(&mut file) {
        panic!("{:?}", err);
    }
}

#[test]
fn test_icns() {
    let mut file = BufWriter::new(File::create("tests/test.icns")
        .expect("Couldn't create file"));

    let mut icon = Icns::new();
    let img = SourceImage::from_path("tests/hydra.png")
        .expect("File not found");

    if let Err(err) = icon.add_entries(resample::nearest, &img, vec![32, 64]) {
        panic!("{:?}", err);
    }

    if let Err(err) = icon.add_entry(resample::nearest, &img, 128) {
        panic!("{:?}", err);
    }

    if let Err(err) = icon.add_entry(resample::nearest, &img, 32) {
        panic!("{:?}", err);
    }

    if let Err(err) = icon.write(&mut file) {
        panic!("{:?}", err);
    }
}

#[test]
fn test_png() {
    let mut file = File::create("tests/test.tar")
        .expect("Couldn't create file");

    let mut icon = PngSequence::new();
    let img = SourceImage::from_path("tests/hydra.png")
        .expect("File not found");

    if let Err(err) = icon.add_entries(resample::linear, &img, vec![32, 64]) {
        panic!("{:?}", err);
    }

    if let Err(err) = icon.write(&mut file) {
        panic!("{:?}", err);
    }
}