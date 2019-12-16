use codemap::Span;
use codemap_diagnostic::{ColorConfig, Diagnostic, Emitter, Level, SpanLabel, SpanStyle};
use std::{fs::File, io::prelude::*, path::Path, sync::Arc};

use super::EmittedError;

pub type FileRef = Arc<codemap::File>;

pub struct Diag<'a> {
    codemap: codemap::CodeMap,
    has_error: bool,
    out_diag: Option<&'a mut (dyn std::io::Write + Send)>,
}

impl<'a> Diag<'a> {
    pub fn new(out_diag: Option<&'a mut (dyn std::io::Write + Send)>) -> Self {
        Self {
            codemap: codemap::CodeMap::new(),
            has_error: false,
            out_diag,
        }
    }

    pub fn add_file(&mut self, name: String, source: String) -> FileRef {
        self.codemap.add_file(name, source)
    }

    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
        source: Option<Span>,
    ) -> Result<FileRef, EmittedError> {
        let path = path.as_ref();

        let source = read_file(path).map_err(|e| {
            self.emit(&[Diagnostic {
                level: Level::Error,
                message: format!("Could not load the input file '{}': {}", path.display(), e),
                code: None,
                spans: source
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);

            // Since we already reported the error through `diag`...
            EmittedError
        })?;

        Ok(self.add_file(path.to_string_lossy().into_owned(), source))
    }

    pub fn has_error(&self) -> bool {
        self.has_error
    }

    pub fn emit(&mut self, msgs: &[Diagnostic]) {
        self.has_error |= msgs
            .iter()
            .any(|m| m.level == Level::Error || m.level == Level::Bug);

        let mut emitter = if let Some(out_diag) = &mut self.out_diag {
            Emitter::new(Box::new(&mut *out_diag), Some(&self.codemap))
        } else {
            Emitter::stderr(ColorConfig::Auto, Some(&self.codemap))
        };
        emitter.emit(msgs);
    }
}

fn read_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}
