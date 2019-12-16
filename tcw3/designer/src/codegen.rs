use codemap_diagnostic::{ColorConfig, Diagnostic, Emitter, Level};
use log::info;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    env, fmt,
    fs::File,
    io::{prelude::*, BufWriter},
    mem::replace,
    path::{Path, PathBuf},
};

use crate::metadata::Repo;

mod diag;
mod implgen;
mod metagen;
mod parser;
mod resolve;
mod sem;

/// Options for the code generator that generates a meta crate's contents.
pub struct BuildScriptConfig<'a, 'b> {
    in_root_source_file: Option<PathBuf>,
    out_source_file: OutputFile<'b>,
    crate_name: Option<String>,
    linked_crates: Vec<(String, Cow<'a, [u8]>)>,
    tcw3_path: String,
    designer_runtime_path: String,
}

enum OutputFile<'a> {
    /// Guessed from an environment variable.
    FromEnv,
    File(Cow<'a, Path>),
    Custom(&'a mut dyn Write),
}

impl<'a, 'b> BuildScriptConfig<'a, 'b> {
    pub fn new() -> Self {
        Self {
            in_root_source_file: None,
            out_source_file: OutputFile::FromEnv,
            crate_name: None,
            linked_crates: Vec::new(),
            tcw3_path: "::tcw3".to_string(),
            designer_runtime_path: "::tcw3::designer_runtime".to_string(),
        }
    }

    pub fn root_source_file(self, path: impl AsRef<Path>) -> Self {
        Self {
            in_root_source_file: Some(path.as_ref().to_path_buf()),
            ..self
        }
    }

    pub fn out_source_file(self, path: impl AsRef<Path>) -> Self {
        Self {
            out_source_file: OutputFile::File(path.as_ref().to_path_buf().into()),
            ..self
        }
    }

    pub fn out_source_stream(self, path: &'b mut dyn Write) -> Self {
        Self {
            out_source_file: OutputFile::Custom(path),
            ..self
        }
    }

    pub fn crate_name(self, name: impl Into<String>) -> Self {
        Self {
            crate_name: Some(name.into()),
            ..self
        }
    }

    pub fn link(mut self, name: impl Into<String>, metadata: Cow<'a, [u8]>) -> Self {
        self.linked_crates.push((name.into(), metadata));
        self
    }

    /// Set the path of `tcw3` used by the generated implementation code.
    /// Defaults to `::tcw3`.
    pub fn tcw3_path(self, path: impl Into<String>) -> Self {
        Self {
            tcw3_path: path.into(),
            ..self
        }
    }

    /// Set the path of `tcw3_designer_runtime` used by the generated
    /// implementation code. Defaults to `::tcw3::designer_runtime`.
    pub fn designer_runtime_path(self, path: impl Into<String>) -> Self {
        Self {
            designer_runtime_path: path.into(),
            ..self
        }
    }

    /// Run the code generator. Terminate the current process on failure.
    ///
    /// This method automatically sets up a logger using `env_logger`.
    pub fn run_and_exit_on_error(self) {
        let _ = env_logger::try_init();

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
        let crate_name = if let Some(x) = self.crate_name {
            x
        } else {
            let meta_pkg_name =
                env::var("CARGO_PKG_NAME").map_err(|_| BuildError::CrateNameMissing)?;
            info!("CARGO_PKG_NAME = {:?}", meta_pkg_name);
            if meta_pkg_name.ends_with("-meta") || meta_pkg_name.ends_with("_meta") {
                meta_pkg_name[0..meta_pkg_name.len() - 5].to_string()
            } else {
                return Err(BuildError::CrateNameMissing);
            }
        };

        let in_root_source_file = if let Some(x) = self.in_root_source_file {
            x
        } else {
            let dir =
                env::var_os("CARGO_MANIFEST_DIR").ok_or(BuildError::CargoManifestDirMissing)?;
            info!("CARGO_MANIFEST_DIR = {:?}", dir);
            Path::new(&dir).join("lib.tcwdl")
        };

        let out_source_file = if let OutputFile::FromEnv = self.out_source_file {
            let out_dir = env::var_os("OUT_DIR").ok_or(BuildError::OutDirMissing)?;
            info!("OUT_DIR = {:?}", out_dir);
            OutputFile::File(Path::new(&out_dir).join("designer.rs").into())
        } else {
            self.out_source_file
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
        let mut deps: Vec<(&str, Repo)> = self
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

        // Consolidate the metadata of our known universe
        // -------------------------------------------------------------------
        let mut uuids = deps
            .iter()
            .enumerate()
            .map(|(dep_i, e)| {
                e.1.crates
                    .iter()
                    .enumerate()
                    .map(move |(crate_i, cr)| (dep_i, crate_i, cr.uuid))
            })
            .flatten()
            .collect::<Vec<_>>();
        uuids.sort_unstable_by_key(|&(_, _, uuid)| uuid);
        uuids.dedup_by_key(|&mut (_, _, uuid)| uuid);

        let mut repo = Repo {
            main_crate_i: 0, // will be set by `gen_and_push_crate`
            crates: Vec::new(),
        };

        // Prepare to remap crate indices
        let dep_crate_i_maps: Vec<Vec<_>> = deps
            .iter()
            .map(|(_, repo)| {
                repo.crates
                    .iter()
                    .map(|cr| {
                        // Find the new crate index (in `repo`)
                        uuids
                            .binary_search_by_key(&cr.uuid, |&(_, _, uuid)| uuid)
                            .unwrap()
                    })
                    .collect()
            })
            .collect();

        // Put all known crates into `repo.crates`
        for &(dep_i, crate_i, uuid) in uuids.iter() {
            let cr_cell = &mut deps[dep_i].1.crates[crate_i];
            let mut cr = replace(cr_cell, Default::default());

            // `uuid` is the primary information of `uuids`. `(dep_i, crate_i)`
            // is optimization for a faster lookup
            assert_eq!(cr.uuid, uuid);

            // Keep UUID, we'll need those in the next step
            cr_cell.uuid = cr.uuid;

            // Remap crate references from `deps[dep_i]` to `repo`
            crate::metadata::visit_mut::visit_crate_mut(
                &mut metagen::MapCrateIndex(&dep_crate_i_maps[dep_i]),
                &mut cr,
            );

            repo.crates.push(cr);
        }

        // We'll need a map from imported crate names (which might not be
        // identical to orignal crate names) to indices into `repo.crates`
        let imports_crate_i: HashMap<&str, usize> = deps
            .into_iter()
            .map(|(imported_name, repo)| {
                let main_crate_uuid = repo.crates[repo.main_crate_i].uuid;

                let crate_i = uuids
                    .binary_search_by_key(&main_crate_uuid, |&(_, _, uuid)| uuid)
                    .unwrap();

                (imported_name, crate_i)
            })
            .collect();

        // Start analysis of this crate
        // -------------------------------------------------------------------
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
        metagen::gen_and_push_crate(&comps, &imports_crate_i, crate_name, &mut repo, &mut diag);

        if diag.has_error() {
            return Err(BuildError::Emitted);
        }

        // TODO: ... which allows us to handle `#[inject] const`
        // TODO: Now, generate `Crate` again

        // Generate implementation code
        let tcw3_path = self.tcw3_path.as_str();
        let designer_runtime_path = self.designer_runtime_path.as_str();
        let comp_code_chunks: Vec<_> = comps
            .iter()
            .enumerate()
            .map(|(comp_i, comp)| {
                let implgen_ctx = implgen::Ctx {
                    repo: &repo,
                    imports_crate_i: &imports_crate_i,
                    cur_comp: comp,
                    cur_meta_comp_i: comp_i,
                    tcw3_path,
                    designer_runtime_path,
                };
                (
                    comp,
                    implgen::gen_comp(&implgen_ctx, &mut diag).unwrap_or_else(|EmittedError| {
                        "compile_error!(\"code generation failed.\")".to_string()
                    }),
                )
            })
            .collect();

        // Remove `pub(in crate::...)` - actually this is not strictly, but may
        // slightly reduce the metadata by pruning unneeded crates in the future
        crate::metadata::visit_mut::visit_repo_mut(
            &mut metagen::DowngradeRestrictedVisibility,
            &mut repo,
        );

        let meta_bin = bincode::serialize(&repo).unwrap();

        // Move out `PathBuf` from `out_source_file`, leaving `Cow::Borrowed`
        let out_source_file_path_storage;
        let (mut out_source_file, out_source_file_path) = match out_source_file {
            OutputFile::FromEnv => unreachable!(),
            OutputFile::File(file) => {
                out_source_file_path_storage = file;
                let path: &Path = &*out_source_file_path_storage;
                (OutputFile::File(path.into()), Some(path))
            }
            OutputFile::Custom(stream) => (OutputFile::Custom(stream), None),
        };

        (move || -> std::io::Result<()> {
            let mut out_file;

            let out_f: &mut dyn Write = match &mut out_source_file {
                OutputFile::FromEnv => unreachable!(),
                OutputFile::File(file) => {
                    out_file = BufWriter::new(File::create(&**file)?);
                    &mut out_file
                }
                OutputFile::Custom(stream) => *stream,
            };

            writeln!(out_f, "// Please do not edit!")?;
            writeln!(
                out_f,
                "// This source file was automatically @generated by TCW3 Designer."
            )?;
            writeln!(out_f)?;

            writeln!(out_f, "/// Automatically generated by `tcw3_designer`.")?;
            writeln!(
                out_f,
                "pub static DESIGNER_METADATA: &[u8] = &[{}];",
                DisplayArray(&meta_bin)
            )?;
            writeln!(out_f)?;

            writeln!(out_f, "#[macro_export]")?;
            writeln!(out_f, "macro_rules! designer_impl {{")?;

            for (comp, code_chunk) in comp_code_chunks {
                writeln!(
                    out_f,
                    "    ({path}) => {{ {chunk} }};",
                    path = comp.path,
                    chunk = code_chunk
                )?;
            }

            writeln!(out_f, "}}")?;

            out_f.flush()?;

            Ok(())
        })()
        .map_err(|e| {
            if let Some(path) = out_source_file_path {
                BuildError::OutputFileError(path.to_path_buf(), e)
            } else {
                BuildError::OutputStreamError(e)
            }
        })?;

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
    /// Could not guess the crate name from `CARGO_PKG_NAME`; are we really in
    /// a build script?
    CrateNameMissing,
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
    /// Could not write the output stream: {0}
    OutputStreamError(std::io::Error),
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
