use codemap_diagnostic::{ColorConfig, Diagnostic, Emitter, Level};
use quote::ToTokens;
use std::{
    borrow::Cow,
    collections::HashSet,
    env, fmt,
    fs::File,
    io::{prelude::*, BufWriter},
    path::{Path, PathBuf},
};

use crate::metadata::Crate;

mod diag;
mod implgen;
mod metagen;
mod parser;
mod resolve;
mod sem;

/// Options for the code generator that generates a meta crate's contents.
#[derive(Default)]
pub struct BuildScriptConfig<'a> {
    in_root_source_file: Option<PathBuf>,
    out_source_file: Option<PathBuf>,
    linked_crates: Vec<(String, Cow<'a, [u8]>)>,
}

impl<'a> BuildScriptConfig<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn root_source_file(self, path: impl AsRef<Path>) -> Self {
        Self {
            in_root_source_file: Some(path.as_ref().to_path_buf()),
            ..self
        }
    }

    pub fn out_source_file(self, path: impl AsRef<Path>) -> Self {
        Self {
            out_source_file: Some(path.as_ref().to_path_buf()),
            ..self
        }
    }

    pub fn link(mut self, name: impl Into<String>, metadata: Cow<'a, [u8]>) -> Self {
        self.linked_crates.push((name.into(), metadata));
        self
    }

    pub fn run_and_exit_on_error(self) {
        if self.run().is_err() {
            std::process::exit(1);
        }
    }

    pub fn run(self) -> Result<(), EmittedError> {
        let result = self.run_inner();
        if let Err(e) = result {
            let mut emitter = Emitter::stderr(ColorConfig::Auto, None);
            emitter.emit(&[Diagnostic {
                level: Level::Error,
                message: if let BuildError::Emitted = e {
                    "Aborting due to previous error(s)".to_string()
                } else {
                    format!("{}", e)
                },
                code: None,
                spans: vec![],
            }]);

            Err(EmittedError)
        } else {
            Ok(())
        }
    }

    fn run_inner(self) -> Result<(), BuildError> {
        let in_root_source_file = if let Some(x) = self.in_root_source_file {
            x
        } else {
            let dir =
                env::var_os("CARGO_MANIFEST_DIR").ok_or(BuildError::CargoManifestDirMissing)?;
            Path::new(&dir).join("lib.tcwdl")
        };

        let out_source_file = if let Some(x) = self.out_source_file {
            x
        } else {
            let out_dir = env::var_os("OUT_DIR").ok_or(BuildError::OutDirMissing)?;
            Path::new(&out_dir).join("designer.rs")
        };

        let mut diag = diag::Diag::new();

        // Parse the input source files
        let mut files = Vec::new();
        {
            let mut queue = vec![(in_root_source_file.clone(), None)];
            let mut found_files = HashSet::new();
            let mut i = 0;

            found_files.insert(in_root_source_file);

            while i < queue.len() {
                let (path, import_span) = queue[i].clone();
                let diag_file = match diag.load_file(&path, import_span) {
                    Ok(f) => f,
                    Err(EmittedError) => {
                        i += 1;
                        continue;
                    }
                };

                let parsed_file = match parser::parse_file(&diag_file, &mut diag) {
                    Ok(f) => f,
                    Err(EmittedError) => {
                        i += 1;
                        continue;
                    }
                };

                // Process `import!` directives
                for item in parsed_file.items.iter() {
                    if let parser::Item::Import(lit) = item {
                        let value = lit.value();
                        let mut new_path = path.clone();
                        new_path.pop();
                        new_path.push(Path::new(&value));

                        if found_files.contains(&new_path) {
                            continue;
                        }

                        found_files.insert(new_path.clone());
                        queue.push((new_path, parser::span_to_codemap(lit.span(), &diag_file)));
                    }
                }

                files.push((parsed_file, diag_file));
                i += 1;
            }
        }

        // Load prelude
        let prelude = resolve::Prelude::new(&mut diag);

        // Resolve paths, meaning they are all expanded to absolute paths
        // as specified by `use` items.
        for (parsed_file, diag_file) in files.iter_mut() {
            resolve::resolve_paths(parsed_file, diag_file, &mut diag, &prelude);
        }

        if diag.has_error() {
            return Err(BuildError::Emitted);
        }

        // Import metadata of dependencies
        let _deps: Vec<(&str, Crate)> = self
            .linked_crates
            .iter()
            .map(|(name, metadata)| {
                Ok((
                    name.as_str(),
                    bincode::deserialize(metadata)
                        .map_err(|e| BuildError::MetadataDeserializationFailure(name.clone(), e))?,
                ))
            })
            .collect::<Result<Vec<_>, BuildError>>()?;

        // Start analysis of this crate
        let mut comps = Vec::new();
        for (parsed_file, diag_file) in files.iter() {
            for item in parsed_file.items.iter() {
                if let parser::Item::Comp(comp) = item {
                    comps.push(sem::analyze_comp(comp, diag_file, &mut diag));
                }
            }
        }

        if diag.has_error() {
            return Err(BuildError::Emitted);
        }

        // Generate metadata (`Crate`) from `comps`
        let mut meta = metagen::gen_crate(&comps);

        // TODO: Analyze `comps` again using all the metadata we have
        // TODO: ... which allows us to handle `#[inject] const`
        // TODO: Now, generate `Crate` again

        // Generate implementation code
        let implgen_ctx = implgen::Ctx {
            crates: vec![],                // TODO
            crate_map: Default::default(), // TODO
        };
        let comp_code_chunks: Vec<_> = comps
            .iter()
            .map(|comp| (comp, implgen::gen_comp(comp, &implgen_ctx, &mut diag)))
            .collect();

        // Remove `pub(in crate::...)`
        crate::metadata::visit_mut::visit_crate_mut(
            &mut metagen::DowngradeRestrictedVisibility,
            &mut meta,
        );

        let meta_bin = bincode::serialize(&meta).unwrap();

        let out_f = File::create(&out_source_file)
            .map_err(|e| BuildError::OutputFileError(out_source_file.clone(), e))?;

        (move || -> std::io::Result<()> {
            let mut out_f = BufWriter::new(out_f);

            writeln!(
                out_f,
                "
                /// Automatically generated by `tcw3_designer`.
                pub static DESIGNER_METADATA: &[u8] = &[{}];
            ",
                DisplayArray(&meta_bin)
            )?;

            writeln!(
                out_f,
                "
                #[macro_export]
                macro_rules! designer_impl {{
            "
            )?;

            for (comp, code_chunk) in comp_code_chunks {
                writeln!(
                    out_f,
                    "
                    ({path}) => {{ {chunk} }};
                ",
                    path = comp.path.to_token_stream(),
                    chunk = code_chunk
                )?;
            }

            writeln!(
                out_f,
                "
                }}
            "
            )?;

            out_f.flush()?;

            Ok(())
        })()
        .map_err(|e| BuildError::OutputFileError(out_source_file.clone(), e))?;

        if diag.has_error() {
            Err(BuildError::Emitted)
        } else {
            Ok(())
        }
    }
}

struct DisplayArray<'a, T>(&'a [T]);

impl<T: fmt::Display> fmt::Display for DisplayArray<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for e in self.0 {
            write!(f, "{}, ", e)?;
        }
        Ok(())
    }
}

/// Represents a build error.
///
/// Most errors are reported through stderr, and in this case, `EmittedError`
/// is returned.
///
/// The doc comments for the variants are converted to a `Display`
/// implementation by `displaydoc`.
#[derive(Debug, displaydoc::Display)]
#[non_exhaustive]
enum BuildError {
    /// `in_root_source_file` is not specified but `CARGO_MANIFEST_DIR` is
    /// missing; are we really in a build script?
    CargoManifestDirMissing,
    /// `out_source_file` is not specified but `OUT_DIR` is missing; are we
    /// really in a build script?
    OutDirMissing,
    /// Failed to import the metadata of `{0}`: {1}
    MetadataDeserializationFailure(String, bincode::Error),
    /// Could not write the output file `{0}`: {1}
    OutputFileError(PathBuf, std::io::Error),
    /// Build failed.
    Emitted,
}

impl From<EmittedError> for BuildError {
    fn from(_: EmittedError) -> Self {
        Self::Emitted
    }
}

/// Represents an error that already has been reported via other means.
#[derive(Debug, Clone, Copy)]
pub struct EmittedError;
