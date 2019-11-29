use codemap::Span;
use codemap_diagnostic::{ColorConfig, Diagnostic, Emitter, Level, SpanLabel, SpanStyle};
use std::{fs::File, io::prelude::*, path::Path, sync::Arc};

pub type FileRef = Arc<codemap::File>;

pub struct Diag {
    codemap: codemap::CodeMap,
    has_error: bool,
}

impl Diag {
    pub fn new() -> Self {
        Self {
            codemap: codemap::CodeMap::new(),
            has_error: false,
        }
    }

    pub fn add_file(&mut self, name: String, source: String) -> FileRef {
        self.codemap.add_file(name, source)
    }

    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
        source: Option<Span>,
    ) -> Result<FileRef, ()> {
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

        Emitter::stderr(ColorConfig::Auto, Some(&self.codemap)).emit(msgs);
    }
}

fn read_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}
