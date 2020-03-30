use crate::*;
use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
};

struct TestDir {
    dir: PathBuf,
    files: Vec<PathBuf>,
}

impl TestDir {
    fn new() -> Self {
        let temp_dir = std::env::temp_dir();
        let mut i = 0;
        loop {
            let dir = temp_dir.join(&format!("icon_baker_test_{}", i));
            println!("Trying to create '{}'...", dir.display());
            match std::fs::create_dir(&dir) {
                Ok(()) => {
                    println!("Successfully created the directory '{}'.", dir.display());
                    return Self {
                        dir,
                        files: Vec::new(),
                    };
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // keep searching
                    i += 1;
                }
                Err(e) => {
                    panic!(
                        "Could not create a directory for storing generated files: {:?}",
                        e
                    );
                }
            }
        }
    }

    fn add_file(&mut self, name: &str) -> PathBuf {
        let file_path = self.dir.join(name);
        self.files.push(file_path.clone());
        file_path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        for path in self.files.iter() {
            println!("Deleting the temporary file '{}'.", path.display());
            std::fs::remove_file(path).unwrap();
        }
        println!("Deleting the temporary directory '{}'.", self.dir.display());
        std::fs::remove_dir(&self.dir).unwrap();
    }
}

fn input_svg_image_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests/deref.svg")
}

#[test]
fn test_ico() {
    let mut dir = TestDir::new();

    let mut file =
        BufWriter::new(File::create(dir.add_file("test.ico")).expect("Couldn't create file"));

    let mut icon = Ico::new();
    let img = SourceImage::from_path(input_svg_image_path()).expect("File not found");

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
    let mut dir = TestDir::new();

    let mut file =
        BufWriter::new(File::create(dir.add_file("test.icns")).expect("Couldn't create file"));

    let mut icon = Icns::new();
    let img = SourceImage::from_path(input_svg_image_path()).expect("File not found");

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
