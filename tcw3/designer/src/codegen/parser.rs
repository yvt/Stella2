use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use proc_macro2::TokenStream;
use quote::ToTokens;
use std::fmt;
use syn::{
    parse::{Parse, ParseStream, Result},
    parse_str,
    punctuated::Punctuated,
    token, Attribute, Error, Expr, FnArg, Ident, ItemUse, LitStr, Path, Token, Type, Visibility,
};

use super::{diag::Diag, EmittedError};

pub mod visit_mut;

// TODO: Preserve the original span of `VisRestricted::path` somehow

pub fn parse_file(
    file: &codemap::File,
    diag: &mut Diag,
) -> std::result::Result<File, EmittedError> {
    parse_str(file.source()).map_err(|e| {
        emit_syn_errors_as_diag(e, diag, file);

        // Since we already reported the error through `diag`...
        EmittedError
    })
}

/// Convert `syn::Error` to `codemap_diagnostic::Diagnostic`s.
pub fn emit_syn_errors_as_diag(e: syn::Error, diag: &mut Diag, file: &codemap::File) {
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

pub mod kw {
    syn::custom_keyword!(comp);
    syn::custom_keyword!(prop);
    syn::custom_keyword!(on);
    syn::custom_keyword!(wire);
    syn::custom_keyword!(get);
    syn::custom_keyword!(set);
    syn::custom_keyword!(watch);
    syn::custom_keyword!(init);
    syn::custom_keyword!(clone);
    syn::custom_keyword!(borrow);
    syn::custom_keyword!(event);
}

pub struct File {
    pub items: Vec<Item>,
}

pub enum Item {
    Import(LitStr),
    Use(ItemUse),
    Comp(Comp),
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

        let la = ahead.lookahead1();
        let mut item = if la.peek(Token![use]) {
            Item::Use(input.parse()?)
        } else if la.peek(kw::comp) {
            Item::Comp(input.parse()?)
        } else {
            return Err(la.error());
        };

        let item_attrs = match &mut item {
            Item::Use(item) => &mut item.attrs,
            Item::Comp(item) => &mut item.attrs,
            Item::Import(_) => unreachable!(),
        };
        attrs.extend(item_attrs.drain(..));
        *item_attrs = attrs;

        Ok(item)
    }
}

/// A component definition.
pub struct Comp {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub comp_token: kw::comp,
    pub path: Path,
    /// `path` before being resolved by `resolve_paths`
    pub orig_path: Path,
    pub brace_token: token::Brace,
    pub items: Vec<CompItem>,
}

impl Parse for Comp {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let comp_token = input.parse()?;
        let path = input.call(Path::parse_mod_style)?;
        let content;
        let brace_token = syn::braced!(content in input);

        let items = std::iter::from_fn(|| {
            if content.is_empty() {
                None
            } else {
                Some(content.parse())
            }
        })
        .collect::<Result<_>>()?;

        Ok(Self {
            attrs,
            vis,
            comp_token,
            orig_path: path.clone(),
            path,
            brace_token,
            items,
        })
    }
}

/// An item in `Comp`.
pub enum CompItem {
    Field(CompItemField),
    On(CompItemOn),
    Event(CompItemEvent),
}

impl Parse for CompItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = input.call(Attribute::parse_outer)?;
        let ahead = input.fork();
        let _vis: Visibility = ahead.parse()?;

        let la = ahead.lookahead1();
        let mut item = if la.peek(kw::prop) || la.peek(Token![const]) || la.peek(kw::wire) {
            CompItem::Field(input.parse()?)
        } else if la.peek(kw::on) {
            CompItem::On(input.parse()?)
        } else if la.peek(kw::event) {
            CompItem::Event(input.parse()?)
        } else {
            return Err(la.error());
        };

        let item_attrs = match &mut item {
            CompItem::Field(item) => &mut item.attrs,
            CompItem::On(item) => &mut item.attrs,
            CompItem::Event(item) => &mut item.attrs,
        };
        attrs.extend(item_attrs.drain(..));
        *item_attrs = attrs;

        Ok(item)
    }
}

/// - `pub prop class_set: ClassSet { pub set; get borrow; } = expr;`
/// - `pub const vertical: ClassSet = expr;`
/// - `pub wire active: ClassSet = expr;`
pub struct CompItemField {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub field_ty: FieldType,
    pub ident: Ident,
    pub ty: Option<Type>,
    pub accessors: Option<Vec<FieldAccessor>>,
    pub dyn_expr: Option<DynExpr>,
    pub semi_token: Option<Token![;]>,
}

pub use crate::metadata::FieldType;

impl Parse for CompItemField {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let field_ty = input.parse()?;
        let ident = input.parse()?;

        let ty = if input.parse::<Token![:]>().is_ok() {
            Some(input.parse()?)
        } else {
            None
        };

        let accessors = if input.peek(token::Brace) {
            let content;
            syn::braced!(content in input);
            Some(
                std::iter::from_fn(|| {
                    if content.is_empty() {
                        None
                    } else {
                        Some(content.parse())
                    }
                })
                .collect::<Result<_>>()?,
            )
        } else {
            None
        };

        let dyn_expr = if input.parse::<Token![=]>().is_ok() {
            Some(input.parse()?)
        } else {
            None
        };

        // A semicolon is required if it's terminated by `DynExpr` or `Type`.
        let semi_token = if !accessors.is_some() || dyn_expr.is_some() {
            Some(input.parse()?)
        } else {
            None
        };

        Ok(Self {
            attrs,
            vis,
            field_ty,
            ident,
            ty,
            accessors,
            dyn_expr,
            semi_token,
        })
    }
}

impl fmt::Display for FieldType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FieldType::Const => "const",
            FieldType::Wire => "wire",
            FieldType::Prop => "prop",
        })
    }
}

impl Parse for FieldType {
    fn parse(input: ParseStream) -> Result<Self> {
        let la = input.lookahead1();
        if la.peek(kw::prop) {
            input.parse::<kw::prop>().map(|_| FieldType::Prop)
        } else if la.peek(Token![const]) {
            input.parse::<Token![const]>().map(|_| FieldType::Const)
        } else if la.peek(kw::wire) {
            input.parse::<kw::wire>().map(|_| FieldType::Wire)
        } else {
            Err(la.error())
        }
    }
}

pub enum FieldAccessor {
    Set {
        set_token: kw::set,
        vis: Visibility,
    },
    Get {
        get_token: kw::get,
        vis: Visibility,
        mode: Option<FieldGetMode>,
    },
    Watch {
        watch_token: kw::watch,
        vis: Visibility,
        mode: FieldWatchMode,
    },
}

pub use crate::metadata::FieldGetMode;

pub enum FieldWatchMode {
    Event { event: Ident },
}

impl Parse for FieldAccessor {
    fn parse(input: ParseStream) -> Result<Self> {
        let vis = input.parse()?;

        let la = input.lookahead1();
        let this = if la.peek(kw::set) {
            let set_token = input.parse::<kw::set>()?;
            FieldAccessor::Set { set_token, vis }
        } else if la.peek(kw::get) {
            let get_token = input.parse::<kw::get>()?;
            FieldAccessor::Get {
                get_token,
                vis,
                mode: if input.peek(Token![;]) {
                    None
                } else {
                    Some(input.parse()?)
                },
            }
        } else if la.peek(kw::watch) {
            let watch_token = input.parse::<kw::watch>()?;
            FieldAccessor::Watch {
                watch_token,
                vis,
                mode: input.parse()?,
            }
        } else {
            return Err(la.error());
        };

        input.parse::<Token![;]>()?;
        Ok(this)
    }
}

impl Parse for FieldGetMode {
    fn parse(input: ParseStream) -> Result<Self> {
        let la = input.lookahead1();
        if la.peek(kw::clone) {
            input.parse::<kw::clone>().map(|_| FieldGetMode::Clone)
        } else if la.peek(kw::borrow) {
            input.parse::<kw::borrow>().map(|_| FieldGetMode::Borrow)
        } else {
            Err(la.error())
        }
    }
}

impl Parse for FieldWatchMode {
    fn parse(input: ParseStream) -> Result<Self> {
        let la = input.lookahead1();
        if la.peek(kw::event) {
            input.parse::<kw::event>()?;

            let content;
            syn::parenthesized!(content in input);

            let event = content.parse()?;

            if !content.is_empty() {
                return Err(content.error("Unexpected token"));
            }

            Ok(FieldWatchMode::Event { event })
        } else {
            Err(la.error())
        }
    }
}

/// - `on (this.const1.event, init, this.prop1) |this.prop| { statements... }`
pub struct CompItemOn {
    pub attrs: Vec<Attribute>,
    pub on_token: kw::on,
    pub paren_token: token::Paren,
    pub triggers: Punctuated<Trigger, Token![,]>,
    pub func: Func,
    pub semi_token: Option<Token![;]>,
}

impl Parse for CompItemOn {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let on_token = input.parse()?;

        let content;
        let paren_token = syn::parenthesized!(content in input);
        let triggers = content.parse_terminated(Trigger::parse)?;

        let func: Func = input.parse()?;

        match vis {
            Visibility::Inherited => {}
            _ => {
                return Err(Error::new_spanned(
                    vis,
                    "visibility specification is not allowed for `on`",
                ))
            }
        }

        // The semicolon is elidable on certain cases
        let semi_token = if expr_requires_terminator(&func.body) {
            Some(input.parse()?)
        } else {
            input.parse().ok()
        };

        Ok(Self {
            attrs,
            on_token,
            paren_token,
            triggers,
            func,
            semi_token,
        })
    }
}

/// - `pub event activated(pal::Wm);`
pub struct CompItemEvent {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub event_token: kw::event,
    pub ident: Ident,
    pub paren_token: token::Paren,
    pub inputs: Punctuated<FnArg, Token![,]>,
    pub semi_token: Option<Token![;]>,
}

impl Parse for CompItemEvent {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let event_token = input.parse()?;
        let ident = input.parse()?;

        let content;
        let paren_token = syn::parenthesized!(content in input);
        let inputs = content.parse_terminated(FnArg::parse)?;

        let semi_token = input.parse()?;

        Ok(Self {
            attrs,
            vis,
            event_token,
            ident,
            paren_token,
            inputs,
            semi_token,
        })
    }
}

pub enum DynExpr {
    Func(Func),
    ObjInit(ObjInit),
}

impl Parse for DynExpr {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Ident) || input.peek(Token![crate]) {
            // Recognize obj-init only at the top-level for now
            let is_obj_init = if let Ok(m) = input.fork().parse::<syn::Macro>() {
                m.path.segments.len() > 1 && m.path.segments.last().unwrap().ident == "new"
            } else {
                false
            };

            if is_obj_init {
                Ok(DynExpr::ObjInit(input.parse()?))
            } else {
                Ok(DynExpr::Func(input.parse()?))
            }
        } else {
            Ok(DynExpr::Func(input.parse()?))
        }
    }
}

/// `42`, `get!(val) + 1`
pub struct Func {
    pub body: Expr,
}

impl Parse for Func {
    fn parse(input: ParseStream) -> Result<Self> {
        let body = input.parse()?;

        Ok(Self { body })
    }
}

pub enum Trigger {
    Init(kw::init),
    Input(Box<Input>),
}

impl Parse for Trigger {
    fn parse(input: ParseStream) -> Result<Self> {
        let la = input.lookahead1();

        if la.peek(kw::init) {
            Ok(Trigger::Init(input.parse()?))
        } else if la.peek(Ident) || la.peek(Token![&]) {
            Ok(Trigger::Input(input.parse()?))
        } else {
            Err(la.error())
        }
    }
}

pub struct FuncInput {
    pub by_ref: Option<Token![&]>,
    pub input: Box<Input>,
    pub rename: Option<(Token![as], Ident)>,
}

impl Parse for FuncInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let by_ref = input.parse().ok();
        let input_parsed = input.parse()?;
        let rename = if input.peek(Token![as]) {
            Some((input.parse()?, input.parse()?))
        } else {
            None
        };

        Ok(Self {
            by_ref,
            input: input_parsed,
            rename,
        })
    }
}

impl FuncInput {
    /// Parse `FuncInput` for inline inputs (`get!(...)`).
    pub fn parse_inline(input: ParseStream) -> Result<Self> {
        let by_ref = input.parse().ok();
        let input_parsed = input.parse()?;

        Ok(Self {
            by_ref,
            input: input_parsed,
            rename: None,
        })
    }
}

/// `this.prop`
pub struct Input {
    pub selectors: Vec<InputSelector>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut selectors = Vec::new();

        let ident: Ident = if let Ok(this) = input.parse::<token::SelfValue>() {
            // `Ident::parse` doesn't accept `self` but we still want to reject
            // other keywords, so special-case `self`
            Ident::new("self", this.span)
        } else {
            input.parse()?
        };
        selectors.push(InputSelector::Field {
            dot_token: None,
            ident,
        });

        loop {
            if input.peek(Token![.]) {
                let dot_token = input.parse()?;
                let ident = input.parse()?;
                selectors.push(InputSelector::Field {
                    dot_token: Some(dot_token),
                    ident,
                });
            } else {
                break;
            }
        }

        Ok(Input { selectors })
    }
}

impl ToTokens for Input {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        for sel in self.selectors.iter() {
            sel.to_tokens(tokens);
        }
    }
}

pub enum InputSelector {
    Field {
        /// Elided for the first selector
        dot_token: Option<Token![.]>,
        ident: Ident,
    }, // TODO: array indexing
}

impl InputSelector {
    #[allow(irrefutable_let_patterns)]
    pub(crate) fn is_field_with_ident(&self, x: impl AsRef<str>) -> bool {
        if let InputSelector::Field { ident, .. } = self {
            *ident == x
        } else {
            false
        }
    }
}

impl ToTokens for InputSelector {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Field { dot_token, ident } => {
                if let Some(x) = dot_token {
                    x.to_tokens(tokens);
                }
                ident.to_tokens(tokens);
            }
        }
    }
}

/// `FillLayout::new! { ... }`
pub struct ObjInit {
    /// Includes `::new`
    pub path: Path,
    /// `path` before being resolved by `resolve_paths`
    pub orig_path: Path,
    pub bang_token: Token![!],
    pub brace_token: token::Brace,
    pub fields: Punctuated<ObjInitField, Token![,]>,
}

impl Parse for ObjInit {
    fn parse(input: ParseStream) -> Result<Self> {
        let mac: syn::Macro = input.parse()?;
        let brace_token = match mac.delimiter {
            syn::MacroDelimiter::Brace(brace) => brace,
            _ => {
                return Err(Error::new_spanned(
                    mac,
                    "Invalid delimiter for object initialization literal",
                ))
            }
        };

        if mac.path.segments.last().unwrap().ident != "new" {
            return Err(Error::new_spanned(
                &mac.path.segments.last().unwrap().ident,
                "Expected `new`",
            ));
        }

        if mac.path.segments.len() <= 1 {
            return Err(Error::new_spanned(
                &mac.path,
                "Expected a component path followed by `::new`",
            ));
        }

        let fields = mac.parse_body_with(Punctuated::parse_terminated)?;

        Ok(Self {
            orig_path: mac.path.clone(),
            path: mac.path,
            bang_token: mac.bang_token,
            brace_token,
            fields,
        })
    }
}

/// `x = value` `x`
pub struct ObjInitField {
    pub ident: Ident,
    pub value: Option<ObjInitFieldValue>,
}

impl Parse for ObjInitField {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            ident: input.parse()?,
            value: if input.peek(Token![=]) {
                Some(input.parse()?)
            } else {
                None
            },
        })
    }
}

pub struct ObjInitFieldValue {
    pub eq_token: Token![=],
    pub dyn_expr: DynExpr,
}

impl Parse for ObjInitFieldValue {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            eq_token: input.parse()?,
            dyn_expr: input.parse()?,
        })
    }
}

/// Taken from `syn`'s private function
fn expr_requires_terminator(expr: &Expr) -> bool {
    // see https://github.com/rust-lang/rust/blob/eb8f2586e/src/libsyntax/parse/classify.rs#L17-L37
    match expr {
        Expr::Unsafe(..)
        | Expr::Block(..)
        | Expr::If(..)
        | Expr::Match(..)
        | Expr::While(..)
        | Expr::Loop(..)
        | Expr::ForLoop(..)
        | Expr::Async(..)
        | Expr::TryBlock(..) => false,
        _ => true,
    }
}
