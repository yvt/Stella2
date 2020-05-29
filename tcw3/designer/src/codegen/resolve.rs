use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use std::collections::HashMap;
use syn::{
    punctuated::Punctuated, spanned::Spanned, Ident, ItemUse, Path, PathArguments, PathSegment,
    Token, Type, UseTree,
};

use super::{
    diag::Diag,
    parser::{span_to_codemap, visit_mut, File, Func, Item},
};

/// Replace all `Path`s in the given AST with absolute paths
/// (`::cratename::item`) or `crate::` paths.
///
/// The postcondition of the resulting paths is defined by
/// `is_path_rooted_or_crate`. It might not be upheld if there were any errors,
/// which will be reported through `diag`.
///
/// A path inside `syn::Type::Path` might be left unprocessed if it resolves to
/// a built-in type.
pub fn resolve_paths(
    file: &mut File,
    codemap_file: &codemap::File,
    diag: &mut Diag,
    prelude: &Prelude,
) {
    // Import prelude
    let mut alias_map = prelude.alias_map.clone();

    for item in file.items.iter() {
        if let Item::Use(u) = item {
            process_use(&mut alias_map, codemap_file, diag, u);
        }
    }

    // Report duplicate imports
    for (ident, aliases) in alias_map.iter_mut() {
        // Imports from prelude may be overridden
        if aliases.len() > 1 && aliases[0].from_prelude {
            aliases.swap_remove(0);
        }

        if aliases.len() > 1 {
            diag.emit(&[Diagnostic {
                level: Level::Error,
                message: format!("`{}` is imported for multiple times", ident),
                code: None,
                spans: aliases
                    .iter()
                    .filter_map(|a| a.span)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .collect(),
            }]);
        }
    }

    struct PathResolver<'a, 'b> {
        codemap_file: &'a codemap::File,
        diag: &'a mut Diag<'b>,
        alias_map: &'a HashMap<Ident, Vec<Alias>>,
    }

    impl syn::visit_mut::VisitMut for PathResolver<'_, '_> {
        fn visit_item_use_mut(&mut self, _: &mut syn::ItemUse) {}

        fn visit_attribute_mut(&mut self, _: &mut syn::Attribute) {}

        fn visit_type_mut(&mut self, i: &mut Type) {
            if let Type::Path(type_path) = i {
                // Look for `u32[::...]`
                let first_ident = &type_path.path.segments[0].ident;
                if type_path.path.leading_colon.is_none()
                    && type_path.qself.is_none()
                    && is_builtin_type_ident(first_ident)
                {
                    // Built-in types can be shadowed by imports. Surprisingly,
                    // Rust works this way
                    if !self.alias_map.contains_key(first_ident) {
                        return;
                    }

                    // A resolved path never refers to a built-in type
                }
            }

            syn::visit_mut::visit_type_mut(self, i);
        }

        fn visit_path_mut(&mut self, i: &mut Path) {
            // Pathes may have generic arguments, which in turn may contain
            // even more pathes
            syn::visit_mut::visit_path_mut(self, i);

            let mut applied_map_list: Vec<(&Ident, &Alias)> = Vec::new();
            let path_span = span_to_codemap(i.span(), self.codemap_file);

            loop {
                if is_path_rooted_or_crate(i) {
                    // The path is already rooted, no need to resolve
                    return;
                }

                let root_ident = i.segments.first().unwrap().ident.to_string();
                if root_ident == "super" {
                    let span = i.segments.first().unwrap().ident.span();
                    self.diag.emit(&[Diagnostic {
                        level: Level::Error,
                        message: "`super` is not allowed to use".to_string(),
                        code: None,
                        spans: span_to_codemap(span, self.codemap_file)
                            .into_iter()
                            .map(|span| SpanLabel {
                                span,
                                label: None,
                                style: SpanStyle::Primary,
                            })
                            .collect(),
                    }]);
                    break;
                }
                let root_is_self = root_ident == "self";

                let first_ident_i = root_is_self as usize;
                let first_ident = &i.segments[first_ident_i].ident;

                if applied_map_list.iter().any(|(i, _)| *i == first_ident) {
                    // Detected a cycle
                    let mut spans: Vec<_> = path_span
                        .map(|span| SpanLabel {
                            span,
                            label: Some("while resolving this".to_string()),
                            style: SpanStyle::Primary,
                        })
                        .into_iter()
                        .collect();

                    spans.extend(
                        applied_map_list
                            .iter()
                            .zip(1..)
                            .filter_map(|((_, alias), k)| {
                                alias.span.map(|span| SpanLabel {
                                    span,
                                    label: Some(format!("({})", k)),
                                    style: SpanStyle::Secondary,
                                })
                            }),
                    );

                    self.diag.emit(&[Diagnostic {
                        level: Level::Error,
                        message: "Detected a cycle while resolving a path".to_string(),
                        code: None,
                        spans,
                    }]);
                    break;
                }

                if let Some((ident, aliases)) = self.alias_map.get_key_value(&first_ident) {
                    let alias = &aliases[0];

                    // Leave breadcrumbs to detect a cycle
                    applied_map_list.push((ident, alias));

                    // e.g., `self::a<T>::b::c` is mapped by `use self::f::g as a;`.
                    // `new_path` = `self::f::g`
                    let mut new_path = alias.path.clone();

                    // Attach `<T>` to the last component, `g`
                    let head = new_path.segments.last_mut().unwrap();
                    head.arguments = i.segments.first().unwrap().arguments.clone();

                    // Append `::b::c` to finally get `self::f::g<T>::b::c`
                    for k in (first_ident_i + 1)..i.segments.len() {
                        new_path.segments.push(i.segments[k].clone());
                    }

                    *i = new_path;
                } else if root_is_self {
                    // `i` is `self::hoge` but `hoge` is not in the scope
                    let spans = vec![
                        span_to_codemap(first_ident.span(), self.codemap_file).map(|span| {
                            SpanLabel {
                                span,
                                label: None,
                                style: SpanStyle::Primary,
                            }
                        }),
                        path_span.map(|span| SpanLabel {
                            span,
                            label: Some("referenced from here".to_string()),
                            style: SpanStyle::Primary,
                        }),
                    ]
                    .into_iter()
                    .filter_map(|x| x)
                    .collect();

                    self.diag.emit(&[Diagnostic {
                        level: Level::Error,
                        message: format!("Could not resolve `{}`", first_ident),
                        code: None,
                        spans,
                    }]);
                    break;
                } else {
                    // The input path turned out to be a rooted path.
                    let span = i.segments[0].span();
                    i.leading_colon = Some(Token![::](span));
                    break;
                }
            }
        }
    }

    impl visit_mut::TcwdlVisitMut for PathResolver<'_, '_> {
        fn visit_func_mut(&mut self, _: &mut Func) {
            // Ignore `i.inputs` because it does not include a path.
            // Ignore `i.body` because it's inserted to the implementation code
            // verbatim.
        }
    }

    visit_mut::visit_file_mut(
        &mut PathResolver {
            codemap_file,
            diag,
            alias_map: &alias_map,
        },
        file,
    );
}

fn is_path_rooted_or_crate(path: &Path) -> bool {
    if path.leading_colon.is_some() {
        true
    } else {
        let first = path.segments.first().unwrap().ident.to_string();
        first == "crate"
    }
}

fn is_builtin_type_ident(ident: &Ident) -> bool {
    [
        "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64", "bool",
        "char", "str", "usize", "isize",
    ]
    .iter()
    .any(|&s| *ident == s)
}

#[derive(Clone)]
struct Alias {
    path: Path,
    span: Option<codemap::Span>,
    from_prelude: bool,
}

fn process_use(
    out_aliases: &mut HashMap<Ident, Vec<Alias>>,
    codemap_file: &codemap::File,
    diag: &mut Diag,
    item: &ItemUse,
) {
    let mut empty_path = Path {
        leading_colon: item.leading_colon.clone(),
        segments: Punctuated::new(),
    };

    process_use_tree(
        &mut empty_path,
        &item.tree,
        codemap_file,
        diag,
        &mut |ident, alias| {
            out_aliases.entry(ident).or_default().push(alias);
        },
    );
}

fn process_use_tree(
    path: &mut Path,
    use_tree: &UseTree,
    codemap_file: &codemap::File,
    diag: &mut Diag,
    f: &mut impl FnMut(Ident, Alias),
) {
    match use_tree {
        UseTree::Path(t) => {
            path.segments.push(PathSegment {
                ident: t.ident.clone(),
                arguments: PathArguments::None,
            });
            process_use_tree(path, &t.tree, codemap_file, diag, f);
            path.segments.pop();
        }
        UseTree::Name(t) => {
            let rename = if t.ident == "self" {
                if let Some(last) = path.segments.last().cloned() {
                    last.ident
                } else {
                    // This case is not supported. The error is reported during
                    // the recursive call to `process_use_tree`
                    t.ident.clone()
                }
            } else {
                t.ident.clone()
            };

            process_use_tree(
                path,
                &UseTree::Rename(syn::UseRename {
                    ident: t.ident.clone(),
                    as_token: Token![as](proc_macro2::Span::call_site()),
                    rename,
                }),
                codemap_file,
                diag,
                f,
            );
        }
        UseTree::Rename(t) => {
            let mut path = path.clone();
            if t.ident == "self" {
                if path.segments.is_empty() {
                    diag.emit(&[Diagnostic {
                        level: Level::Error,
                        message: "Importing `self` is not allowed".to_string(),
                        code: None,
                        spans: span_to_codemap(t.ident.span(), codemap_file)
                            .map(|span| SpanLabel {
                                span,
                                label: None,
                                style: SpanStyle::Primary,
                            })
                            .into_iter()
                            .collect(),
                    }]);
                    return;
                }
            } else {
                path.segments.push(PathSegment {
                    ident: t.ident.clone(),
                    arguments: PathArguments::None,
                });
            }

            f(
                t.rename.clone(),
                Alias {
                    path,
                    span: span_to_codemap(t.rename.span(), codemap_file),
                    from_prelude: false,
                },
            );
        }
        UseTree::Glob(t) => {
            diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "`*` is not supported".to_string(),
                code: None,
                spans: span_to_codemap(t.star_token.span(), codemap_file)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);
        }
        UseTree::Group(t) => {
            for item in t.items.iter() {
                process_use_tree(path, item, codemap_file, diag, f);
            }
        }
    }
}

pub struct Prelude {
    alias_map: HashMap<Ident, Vec<Alias>>,
}

impl Prelude {
    /// Load the prelude module. It shouldn't generate any errors, but this
    /// method still needs `diag` because it shares the same subroutines with
    /// normal processing.
    pub fn new(diag: &mut Diag) -> Self {
        let source = include_str!("prelude.txt");

        let diag_file = diag.add_file("<prelude>".to_owned(), source.to_owned());
        let parsed_file = super::parser::parse_file(&diag_file, diag).unwrap();

        let mut alias_map = HashMap::new();
        for item in parsed_file.items.iter() {
            if let Item::Use(u) = item {
                process_use(&mut alias_map, &diag_file, diag, u);
            }
        }

        for aliases in alias_map.values_mut() {
            assert_eq!(aliases.len(), 1);
            aliases[0].from_prelude = true;
        }

        Self { alias_map }
    }
}
