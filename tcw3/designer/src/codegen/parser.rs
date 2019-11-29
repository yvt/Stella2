use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use syn::{
    parse::{Parse, ParseStream, Result},
    parse_str, Error, LitStr, Token,
};

use super::diag::Diag;

pub fn parse_file(file: &codemap::File, diag: &mut Diag) -> std::result::Result<File, ()> {
    parse_str(file.source()).map_err(|e| {
        // Convert `syn::Error` to `codemap_diagnostic::Diagnostic`s
        for error in e {
            diag.emit(&[Diagnostic {
                level: Level::Error,
                message: format!("{}", error),
                code: None,
                spans: span_to_codemap(error.span(), file)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);
        }
    })
}

fn span_to_codemap(span: proc_macro2::Span, file: &codemap::File) -> Option<codemap::Span> {
    let start = line_column_to_span(span.start(), file);
    let end = line_column_to_span(span.end(), file);
    Some(start.merge(end))
}

fn line_column_to_span(lc: proc_macro2::LineColumn, file: &codemap::File) -> codemap::Span {
    let line_span = file.line_span(lc.line - 1);
    line_span.subspan(lc.column as u64, lc.column as u64)
}

pub struct File {
    pub items: Vec<Item>,
}

pub enum Item {
    Import(LitStr),
}

impl Parse for File {
    fn parse(input: ParseStream) -> Result<Self> {
        let items = std::iter::from_fn(|| {
            if input.is_empty() {
                None
            } else {
                Some(input.parse())
            }
        })
        .collect::<Result<_>>()?;

        Ok(Self { items })
    }
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(syn::Ident) && input.peek2(Token![!]) {
            let mac: syn::Macro = input.parse()?;

            let bad_macro_delim = match mac.delimiter {
                syn::MacroDelimiter::Paren(_) => None,
                syn::MacroDelimiter::Brace(brace) => Some(brace.span),
                syn::MacroDelimiter::Bracket(bracket) => Some(bracket.span),
            };

            if let Some(span) = bad_macro_delim {
                return Err(Error::new(span, "Unexpected delimiter"));
            }

            let item = if mac.path.is_ident("import") {
                Item::Import(mac.parse_body()?)
            } else {
                return Err(Error::new_spanned(mac.path, "Unknown directive"));
            };

            input.parse::<Token![;]>()?;

            Ok(item)
        } else {
            Err(input.error("Unexpected token"))
        }
    }
}
