use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use syn::{
    parse::{Parse, ParseStream, Result},
    parse_str, Attribute, Error, ItemUse, LitStr, Token, Visibility,
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

pub fn span_to_codemap(span: proc_macro2::Span, file: &codemap::File) -> Option<codemap::Span> {
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
    Use(ItemUse),

    // TODO
    __Nonexhaustive,
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

            return Ok(item);
        }

        let mut attrs = input.call(Attribute::parse_outer)?;
        let ahead = input.fork();
        let _vis: Visibility = ahead.parse()?;

        let mut item = if input.peek(Token![use]) {
            Item::Use(check_use_syntax(input.parse()?)?)
        } else {
            return Err(input.error("Unexpected token"));
        };

        let item_attrs = match &mut item {
            Item::Use(item) => &mut item.attrs,
            _ => unreachable!(),
        };
        attrs.extend(item_attrs.drain(..));
        *item_attrs = attrs;

        Ok(item)
    }
}

/// Reject unsupported syntax in a given `use` item.
fn check_use_syntax(node: ItemUse) -> Result<ItemUse> {
    let mut out_errors = Vec::new();
    check_use_syntax_inner(&node.tree, &mut out_errors);

    fn check_use_syntax_inner(node: &syn::UseTree, out_errors: &mut Vec<Error>) {
        match node {
            syn::UseTree::Path(path) => {
                check_use_syntax_inner(&path.tree, out_errors);
            }
            syn::UseTree::Name(_) | syn::UseTree::Rename(_) => {}
            syn::UseTree::Glob(_) => {
                out_errors.push(Error::new_spanned(node, "Glob imports are not supported"));
            }
            syn::UseTree::Group(gr) => {
                for item in gr.items.iter() {
                    check_use_syntax_inner(&item, out_errors);
                }
            }
        }
    }

    if let Some(mut error) = out_errors.pop() {
        for e in out_errors {
            error.combine(e);
        }
        Err(error)
    } else {
        Ok(node)
    }
}
