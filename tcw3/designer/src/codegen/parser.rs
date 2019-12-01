use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use std::fmt;
use syn::{
    parse::{Parse, ParseStream, Result},
    parse_str,
    punctuated::Punctuated,
    token, Attribute, Error, Expr, FnArg, Ident, ItemUse, LitStr, Path, Token, Type, Visibility,
};

use super::{diag::Diag, EmittedError};

pub fn parse_file(
    file: &codemap::File,
    diag: &mut Diag,
) -> std::result::Result<File, EmittedError> {
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

        // Since we already reported the error through `diag`...
        EmittedError
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

pub mod kw {
    syn::custom_keyword!(comp);
    syn::custom_keyword!(prop);
    syn::custom_keyword!(on);
    syn::custom_keyword!(wire);
    syn::custom_keyword!(get);
    syn::custom_keyword!(set);
    syn::custom_keyword!(watch);
    syn::custom_keyword!(init);
    syn::custom_keyword!(sub);
    syn::custom_keyword!(clone);
    syn::custom_keyword!(borrow);
    syn::custom_keyword!(this);
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
    pub brace_token: token::Brace,
    pub items: Vec<CompItem>,
}

impl Parse for Comp {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let comp_token = input.parse()?;
        let path = input.parse()?;
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
            path,
            brace_token,
            items,
        })
    }
}

/// An item in `Comp`.
pub enum CompItem {
    Field(CompItemField),
    Init(CompItemInit),
    Watch(CompItemWatch),
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
        } else if la.peek(kw::init) {
            CompItem::Init(input.parse()?)
        } else if la.peek(kw::watch) {
            CompItem::Watch(input.parse()?)
        } else if la.peek(kw::on) {
            CompItem::On(input.parse()?)
        } else if la.peek(kw::event) {
            CompItem::Event(input.parse()?)
        } else {
            return Err(la.error());
        };

        let item_attrs = match &mut item {
            CompItem::Field(item) => &mut item.attrs,
            CompItem::Init(item) => &mut item.attrs,
            CompItem::Watch(item) => &mut item.attrs,
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

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FieldType {
    Prop,
    Const,
    Wire,
}

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

#[derive(Debug, Clone, Copy)]
pub enum FieldGetMode {
    Borrow,
    Clone,
}

pub enum FieldWatchMode {
    Sub { method: Ident },
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
        if la.peek(kw::sub) {
            input.parse::<kw::sub>()?;

            let content;
            syn::braced!(content in input);

            let method = content.parse()?;

            if !content.is_empty() {
                return Err(content.error("Unexpected token"));
            }

            Ok(FieldWatchMode::Sub { method })
        } else {
            Err(la.error())
        }
    }
}

/// - `init |this.prop| { statements... };`
pub struct CompItemInit {
    pub attrs: Vec<Attribute>,
    pub init_token: kw::init,
    pub func: Func,
    pub semi_token: Option<Token![;]>,
}

impl Parse for CompItemInit {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let init_token = input.parse()?;
        let func: Func = input.parse()?;

        match vis {
            Visibility::Inherited => {}
            _ => {
                return Err(Error::new_spanned(
                    vis,
                    "visibility specification is not allowed for `init`",
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
            init_token,
            func,
            semi_token,
        })
    }
}

/// - `watch |this.prop| { statements... };`
pub struct CompItemWatch {
    pub attrs: Vec<Attribute>,
    pub watch_token: kw::watch,
    pub func: Func,
    pub semi_token: Option<Token![;]>,
}

impl Parse for CompItemWatch {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let watch_token = input.parse()?;
        let func: Func = input.parse()?;

        match vis {
            Visibility::Inherited => {}
            _ => {
                return Err(Error::new_spanned(
                    vis,
                    "visibility specification is not allowed for `watch`",
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
            watch_token,
            func,
            semi_token,
        })
    }
}

/// - `on (this.const1.event) |this.prop| { statements... }`
pub struct CompItemOn {
    pub attrs: Vec<Attribute>,
    pub on_token: kw::on,
    pub paren_token: token::Paren,
    pub event: Box<Input>,
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
        let event = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("unexpected token"));
        }

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
            event,
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
        let la = input.lookahead1();

        if la.peek(Token![|]) {
            Ok(DynExpr::Func(input.parse()?))
        } else if la.peek(Ident) {
            Ok(DynExpr::ObjInit(input.parse()?))
        } else {
            Err(la.error())
        }
    }
}

/// |&this, this.prop|
pub struct Func {
    pub or1_token: Token![|],
    pub inputs: Vec<FuncInput>,
    pub or2_token: Token![|],
    pub body: Expr,
}

impl Parse for Func {
    fn parse(input: ParseStream) -> Result<Self> {
        let or1_token = input.parse()?;
        let mut first = true;
        let inputs = std::iter::from_fn(|| {
            // delimiter
            let colon = input.parse::<Token![,]>();
            while input.parse::<Token![,]>().is_ok() {}

            if input.peek(Token![|]) {
                None
            } else {
                if let (Err(e), false) = (colon, first) {
                    return Some(Err(e));
                }
                first = false;

                Some(input.parse())
            }
        })
        .collect::<Result<_>>()?;
        let or2_token = input.parse()?;
        let body = input.parse()?;

        Ok(Self {
            or1_token,
            inputs,
            or2_token,
            body,
        })
    }
}

pub struct FuncInput {
    pub by_ref: Option<Token![&]>,
    pub input: Box<Input>,
}

impl Parse for FuncInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let by_ref = input.parse().ok();
        let input = input.parse()?;

        Ok(Self { by_ref, input })
    }
}

/// `this.prop`
pub enum Input {
    Field(InputField),
    This(kw::this),
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut this = Input::This(input.parse()?);

        while input.peek(Token![.]) {
            let dot_token = input.parse()?;
            let member = input.parse()?;

            this = Input::Field(InputField {
                base: Box::new(this),
                dot_token,
                member,
            });
        }

        Ok(this)
    }
}

/// `{base}.member`
pub struct InputField {
    pub base: Box<Input>,
    pub dot_token: Token![.],
    pub member: Ident,
}

/// `FillLayout { ... }`
pub struct ObjInit {
    pub path: Path,
    pub brace_token: token::Brace,
    pub fields: Vec<ObjInitField>,
}

impl Parse for ObjInit {
    fn parse(input: ParseStream) -> Result<Self> {
        let path = input.parse()?;
        let content;
        let brace_token = syn::braced!(content in input);

        let fields = std::iter::from_fn(|| {
            if content.is_empty() {
                None
            } else {
                Some(content.parse())
            }
        })
        .collect::<Result<_>>()?;

        Ok(Self {
            path,
            brace_token,
            fields,
        })
    }
}

/// `prop x = value;`
pub struct ObjInitField {
    /// `prop` or `const`. `wire` is not valid here
    pub field_ty: FieldType,
    pub ident: Ident,
    pub eq_token: Token![=],
    pub dyn_expr: DynExpr,
    pub semi_token: Token![;],
}

impl Parse for ObjInitField {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            field_ty: input.parse()?,
            ident: input.parse()?,
            eq_token: input.parse()?,
            dyn_expr: input.parse()?,
            semi_token: input.parse()?,
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

fn dynexpr_requires_terminator(expr: &DynExpr) -> bool {
    match expr {
        DynExpr::Func(Func { body, .. }) => expr_requires_terminator(body),
        _ => true,
    }
}
