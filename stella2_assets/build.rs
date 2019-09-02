use std::{
    env,
    fs::{copy, read, File},
    io::{BufWriter, Read},
    path::Path,
};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        add_windows_resource();
    }
}

/// Build and link a Windows resource for the Stella2 application.
///
/// When an executable is linked to this crate, the resource generated here
/// is added to the executable.
fn add_windows_resource() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // The main script file
    let rc_path = Path::new(&out_dir).join("stella2.rc");
    copy("windows/stella2.rc", &rc_path).unwrap();

    // The appllcation icon
    // TODO: `icon_baker` pulls too many dependencies. (Three different
    //       versions of `png`, seriously!?)
    let ico_path = Path::new(&out_dir).join("stella2.ico");
    {
        use icon_baker::{resample, Icon, SvgImage};
        let mut ico = icon_baker::Ico::new();

        // placeholder
        let svgz = read("../stvg/tests/horse.svgz").unwrap();
        let svg_text_stream = libflate::gzip::Decoder::new(&svgz[..]).unwrap();
        let mut svg_text = String::new();
        { svg_text_stream }.read_to_string(&mut svg_text).unwrap();
        let svg_img = SvgImage::parse_str(&svg_text, icon_baker::nsvg::Units::Pixel, 96.0)
            .unwrap()
            .into();

        ico.add_entry(resample::linear, &svg_img, 16).unwrap();
        ico.add_entry(resample::linear, &svg_img, 32).unwrap();
        ico.add_entry(resample::linear, &svg_img, 48).unwrap();
        ico.add_entry(resample::linear, &svg_img, 64).unwrap();

        ico.write(&mut BufWriter::new(File::create(&ico_path).unwrap()))
            .unwrap();
    }

    // The application manifest
    let manifest_path = Path::new(&out_dir).join("stella2.manifest");
    copy("windows/stella2.manifest", &manifest_path).unwrap();

    embed_resource::compile(&rc_path);
}
