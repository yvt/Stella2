use std::{
    fs::{copy, create_dir_all, write, File},
    io::{prelude::*, BufWriter},
    path::{Path, PathBuf},
};
use structopt::StructOpt;

/// A small command-line application for createing a macOS application bundle.
#[derive(Debug, StructOpt)]
struct Opt {
    /// A path to the executable file. Defaults to
    /// `(project root)/target/x86_64-apple-darwin/release/stella2`.
    #[structopt(short = "x")]
    exe_path: Option<PathBuf>,

    /// A path to the directory to store the generated application bundle in.
    /// Defaults to `(project root)/publish`.
    #[structopt(short = "o")]
    out_path: Option<PathBuf>,
}

fn main() {
    let opt = Opt::from_args();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let exe_path = opt
        .exe_path
        .unwrap_or_else(|| manifest_dir.join("../../target/x86_64-apple-darwin/release/stella2"));
    let out_path = opt
        .out_path
        .unwrap_or_else(|| manifest_dir.join("../../publish"));

    let bundle_path = out_path.join("Stella 2.app");

    create_dir_all(bundle_path.join("Contents/MacOS"))
        .expect("failed to create a directory `MacOS`");
    create_dir_all(bundle_path.join("Contents/Resources"))
        .expect("failed to create a directory `Resources`");

    // `Info.plist`
    let info_path = bundle_path.join("Contents/Info.plist");
    write(&info_path, &include_bytes!("Info.plist")[..]).expect("failed to write `Info.plist`");

    // The executable
    let out_exe_path = bundle_path.join("Contents/MacOS/stella2");
    copy(&exe_path, &out_exe_path).expect(&format!(
        "failed to copy the executable from '{}'",
        exe_path.display()
    ));

    // The appllcation icon
    // TODO: `icon_baker` pulls too many dependencies. (Three different
    //       versions of `png`, seriously!?)
    let ico_path = bundle_path.join("Contents/Resources/stella2.icns");
    {
        use icon_baker::{resample, Icon, SvgImage};
        let mut ico = icon_baker::Icns::new();

        // placeholder
        let svgz = include_bytes!("../../stvg/tests/horse.svgz");
        let svg_text_stream = libflate::gzip::Decoder::new(&svgz[..]).unwrap();
        let mut svg_text = String::new();
        { svg_text_stream }.read_to_string(&mut svg_text).unwrap();
        let svg_img = SvgImage::parse_str(&svg_text, icon_baker::nsvg::Units::Pixel, 96.0)
            .unwrap()
            .into();

        // TODO: Some images get corrupted
        ico.add_entry(resample::linear, &svg_img, 16).unwrap();
        ico.add_entry(resample::linear, &svg_img, 32).unwrap();
        ico.add_entry(resample::linear, &svg_img, 64).unwrap();
        ico.add_entry(resample::linear, &svg_img, 128).unwrap();
        ico.add_entry(resample::linear, &svg_img, 256).unwrap();
        ico.add_entry(resample::linear, &svg_img, 512).unwrap();
        ico.add_entry(resample::linear, &svg_img, 1024).unwrap();

        ico.write(&mut BufWriter::new(File::create(&ico_path).unwrap()))
            .expect("faile to write `stella2.icns`");
    }

    println!("{}", bundle_path.display());
}
