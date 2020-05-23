use arrayvec::ArrayVec;
use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::{collections::HashMap, fmt};
use syn::{
    parse::{Parse, ParseStream, Result},
    spanned::Spanned,
};
use try_match::try_match;

use super::{
    diag::Diag,
    parser,
    parser::{emit_syn_errors_as_diag, span_to_codemap},
};

#[derive(Debug, Clone)]
pub struct Ident {
    pub sym: String,
    pub span: Option<codemap::Span>,
}

impl Ident {
    pub fn from_syn(i: &syn::Ident, file: &codemap::File) -> Self {
        Self {
            sym: i.to_string(),
            span: parser::span_to_codemap(i.span(), file),
        }
    }
}

#[derive(Clone)]
pub enum Visibility {
    Public {
        span: Option<codemap::Span>,
    },
    Crate {
        span: Option<codemap::Span>,
    },
    Restricted {
        span: Option<codemap::Span>,
        path: Box<Path>,
    },
}

impl Visibility {
    pub fn from_syn(i: &syn::Visibility, default_path: &Box<Path>, file: &codemap::File) -> Self {
        match i {
            syn::Visibility::Public(_) => Visibility::Public {
                span: parser::span_to_codemap(i.span(), file),
            },
            syn::Visibility::Crate(_) => Visibility::Crate {
                span: parser::span_to_codemap(i.span(), file),
            },
            syn::Visibility::Restricted(r) => Visibility::Restricted {
                span: parser::span_to_codemap(i.span(), file),
                path: Box::new(Path::from_syn(&r.path, file)),
            },
            syn::Visibility::Inherited => Visibility::Restricted {
                span: parser::span_to_codemap(i.span(), file),
                path: default_path.clone(),
            },
        }
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public { .. } => write!(f, "pub"),
            Self::Crate { .. } => write!(f, "pub(crate)"),
            Self::Restricted { path, .. } => write!(f, "pub(in {})", path),
        }
    }
}

#[derive(Clone)]
pub struct Path {
    pub syn_path: syn::Path,
    pub span: Option<codemap::Span>,
}

impl Path {
    pub fn from_syn(i: &syn::Path, file: &codemap::File) -> Self {
        Self {
            syn_path: i.clone(),
            span: parser::span_to_codemap(i.span(), file),
        }
    }

    pub fn from_syn_with_span_of(
        i: &syn::Path,
        with_span_of: &syn::Path,
        file: &codemap::File,
    ) -> Self {
        Self {
            syn_path: i.clone(),
            span: parser::span_to_codemap(with_span_of.span(), file),
        }
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.syn_path.to_token_stream())
    }
}

#[derive(Debug, Clone)]
pub struct DocAttr {
    pub text: String,
    pub span: Option<codemap::Span>,
}

impl DocAttr {
    pub fn from_syn(i: &syn::Attribute, file: &codemap::File) -> Result<Self> {
        let text = match i.parse_meta()? {
            syn::Meta::NameValue(syn::MetaNameValue {
                lit: syn::Lit::Str(text),
                ..
            }) => text.value(),
            _ => {
                return Err(syn::Error::new_spanned(i, "Invalid doc comment"));
            }
        };
        Ok(Self {
            text,
            span: parser::span_to_codemap(i.span(), file),
        })
    }
}

/// Represents a set of `use` items which are used to resolve paths in
/// dynamic expressions.
pub struct ImportScope<'a> {
    file: &'a parser::File,
}

impl<'a> ImportScope<'a> {
    pub fn iter_use_items(&self) -> impl Iterator<Item = &'a syn::ItemUse> + '_ {
        self.file
            .items
            .iter()
            .filter_map(|item| try_match!(parser::Item::Use(x) = item).ok())
    }
}

pub struct CompDef<'a> {
    pub flags: CompFlags,
    pub vis: Visibility,
    pub doc_attrs: Vec<DocAttr>,
    pub path: Path,
    /// The last component of `path`.
    pub ident: Ident,
    pub items: Vec<CompItemDef<'a>>,
    pub syn: &'a parser::Comp,
    pub import_scope: ImportScope<'a>,
}

pub use crate::metadata::CompFlags;

pub enum CompItemDef<'a> {
    Field(FieldDef<'a>),
    On(OnDef<'a>),
    Event(EventDef<'a>),
}

impl<'a> CompItemDef<'a> {
    pub fn field(&self) -> Option<&FieldDef<'a>> {
        try_match!(Self::Field(x) = self).ok()
    }

    pub fn on(&self) -> Option<&OnDef<'a>> {
        try_match!(Self::On(x) = self).ok()
    }

    pub fn event(&self) -> Option<&EventDef<'a>> {
        try_match!(Self::Event(x) = self).ok()
    }

    pub fn field_mut(&mut self) -> Option<&mut FieldDef<'a>> {
        try_match!(Self::Field(x) = self).ok()
    }

    pub fn ident(&self) -> Option<&Ident> {
        match self {
            CompItemDef::Field(field) => Some(&field.ident),
            CompItemDef::Event(event) => Some(&event.ident),
            CompItemDef::On(_) => None,
        }
    }
}

pub struct FieldDef<'a> {
    pub vis: Visibility, // FIXME: maybe not needed
    pub doc_attrs: Vec<DocAttr>,
    pub field_ty: FieldType,
    pub flags: FieldFlags,
    pub ident: Ident,
    /// The type of the field's value. Set by the analysis unless there was
    /// an error.
    pub ty: Option<syn::Type>,
    pub accessors: FieldAccessors,
    /// - Can be `None` unless `field_ty` is `Wire`. `None` means
    ///   the value must be supplied via the constructor.
    /// - Can be `Some(ObjInit(_))` only if `field_ty` is `Const`.
    ///
    /// For a `PROTOTYPE_ONLY` component, whether this is `Some` or `None` still
    /// matters, whereas the inner value of `Some` doesn't.
    pub value: Option<DynExpr>,
    pub syn: Option<&'a parser::CompItemField>,
}

pub use self::parser::FieldType;

pub use crate::metadata::FieldFlags;

pub struct FieldAccessors {
    /// Valid for `prop` and `const`. For `const`, it refers to a construction
    /// parameter.
    pub set: Option<FieldSetter>,
    /// Valid for all field types
    pub get: Option<FieldGetter>,
    /// Valid only for `prop` and `wire`
    pub watch: Option<FieldWatcher>,
}

impl FieldAccessors {
    fn default_const(vis: Visibility) -> Self {
        Self {
            set: None,
            get: Some(FieldGetter {
                vis,
                mode: FieldGetMode::Borrow,
            }),
            watch: None,
        }
    }

    fn default_prop(vis: Visibility) -> Self {
        Self {
            set: Some(FieldSetter { vis: vis.clone() }),
            get: Some(FieldGetter {
                vis,
                mode: FieldGetMode::Clone,
            }),
            watch: None,
        }
    }

    fn default_wire(vis: Visibility) -> Self {
        Self {
            set: None,
            get: Some(FieldGetter {
                vis,
                mode: FieldGetMode::Clone,
            }),
            watch: None,
        }
    }

    fn default_none() -> Self {
        Self {
            set: None,
            get: None,
            watch: None,
        }
    }
}

pub struct FieldSetter {
    pub vis: Visibility,
}

#[derive(Clone)]
pub struct FieldGetter {
    pub vis: Visibility,
    pub mode: FieldGetMode,
}

pub use self::parser::FieldGetMode;

pub struct FieldWatcher {
    pub vis: Visibility,
    pub event_item_i: usize,
    pub event_span: Option<codemap::Span>,
}

pub struct OnDef<'a> {
    pub triggers: Vec<Trigger>,
    pub func: Func,
    pub syn: &'a parser::CompItemOn,
}

pub struct EventDef<'a> {
    pub vis: Visibility,
    pub doc_attrs: Vec<DocAttr>,
    pub ident: Ident,
    pub inputs: Vec<syn::FnArg>,
    pub syn: &'a parser::CompItemEvent,
}

pub enum DynExpr {
    Func(Func),
    ObjInit(ObjInit),
}

impl DynExpr {
    pub fn func(&self) -> Option<&Func> {
        try_match!(Self::Func(x) = self).ok()
    }

    pub fn obj_init(&self) -> Option<&ObjInit> {
        try_match!(Self::ObjInit(x) = self).ok()
    }
}

pub struct Func {
    pub inputs: Vec<FuncInput>,
    pub body: syn::Expr,
}

pub struct FuncInput {
    pub by_ref: bool,
    pub input: Input,
    /// The identifier used to refer to the input value inside the function
    /// body.
    pub ident: Ident,
}

pub enum Trigger {
    Init(InitTrigger),
    Input(Input),
}

impl Trigger {
    pub fn input(&self) -> Option<&Input> {
        try_match!(Self::Input(x) = self).ok()
    }
}

pub struct InitTrigger {
    pub span: Option<codemap::Span>,
}

pub struct Input {
    pub origin: InputOrigin,
    pub selectors: Vec<Ident>,
    pub span: Option<codemap::Span>,

    /// A sequence number that is unique within `CompDef`. Used by `implgen`
    /// to attach analysis results.
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOrigin {
    This,
    /// An event parameter. In this case, `Input::selectors` must contain
    /// exactly one item.
    Event,
}

pub struct ObjInit {
    pub path: Path,
    pub fields: Vec<ObjInitField>,
}

pub struct ObjInitField {
    /// `prop` or `const`. `wire` is not valid here
    pub ident: Ident,
    pub value: Func,
}

/// Convert the AST to a slightly-higher-level representation. See the code
/// comments to figure out what is done and what is not.
pub fn analyze_comp<'a>(
    comp: &'a parser::Comp,
    parser_file: &'a parser::File,
    file: &codemap::File,
    diag: &mut Diag,
) -> CompDef<'a> {
    AnalyzeCtx {
        file,
        diag,
        next_input_index: 0,
    }
    .analyze_comp(comp, ImportScope { file: parser_file })
}

struct AnalyzeCtx<'a, 'b> {
    file: &'a codemap::File,
    diag: &'a mut Diag<'b>,

    next_input_index: usize,
}

enum CompReloc {
    FieldWatchEvent { item_i: Option<usize>, ident: Ident },
}

impl CompReloc {
    fn with_item_i(self, item_i: usize) -> Self {
        match self {
            CompReloc::FieldWatchEvent { ident, .. } => CompReloc::FieldWatchEvent {
                item_i: Some(item_i),
                ident,
            },
        }
    }
}

impl AnalyzeCtx<'_, '_> {
    fn analyze_comp<'a>(
        &mut self,
        comp: &'a parser::Comp,
        import_scope: ImportScope<'a>,
    ) -> CompDef<'a> {
        let mut lifted_fields = Vec::new();
        let mut relocs = Vec::new();

        let path = Path::from_syn_with_span_of(&comp.path, &comp.orig_path, self.file);

        let parent_path = {
            let mut path = path.clone();
            path_remove_last_segment(&mut path.syn_path).unwrap();
            Box::new(path)
        };

        let mut this = CompDef {
            flags: CompFlags::empty(),
            vis: Visibility::from_syn(&comp.vis, &parent_path, self.file),
            doc_attrs: Vec::new(),
            path,
            ident: Ident::from_syn(&comp.path.segments.last().unwrap().ident, self.file),
            items: comp
                .items
                .iter()
                .enumerate()
                .map(|(item_i, item)| {
                    self.analyze_comp_item(
                        item,
                        &mut lifted_fields,
                        |reloc| relocs.push(reloc.with_item_i(item_i)),
                        &parent_path,
                    )
                })
                .collect(),
            syn: comp,
            import_scope,
        };

        this.items
            .extend(lifted_fields.into_iter().map(CompItemDef::Field));

        for reloc in relocs {
            match reloc {
                CompReloc::FieldWatchEvent { item_i, ident } => {
                    let item_i = item_i.unwrap();
                    let event_item_i = this.items.iter().position(|item| match item {
                        CompItemDef::Event(ev) => ev.ident.sym == ident.sym,
                        _ => false,
                    });

                    if let Some(event_item_i) = event_item_i {
                        let field = this.items[item_i].field_mut().unwrap();
                        let watcher = field.accessors.watch.as_mut().unwrap();
                        watcher.event_item_i = event_item_i;
                    } else {
                        self.diag.emit(&[Diagnostic {
                            level: Level::Error,
                            message: format!(
                                "The component does not have an event named `{}`",
                                ident.sym
                            ),
                            code: None,
                            spans: ident
                                .span
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
            }
        }

        for attr in comp.attrs.iter() {
            if attr.path.is_ident("prototype_only") {
                this.flags |= CompFlags::PROTOTYPE_ONLY;
            } else if attr.path.is_ident("widget") {
                this.flags |= CompFlags::WIDGET;
            } else if attr.path.is_ident("doc") {
                match DocAttr::from_syn(attr, self.file) {
                    Ok(da) => this.doc_attrs.push(da),
                    Err(e) => emit_syn_errors_as_diag(e, self.diag, self.file),
                }
            } else if attr.path.is_ident("builder") {
                self.analyze_comp_builder_attr(&mut this, attr.tokens.clone());
            } else {
                self.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: "Unknown component attribute".to_string(),
                    code: None,
                    spans: span_to_codemap(attr.path.span(), self.file)
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

        if this.flags.contains(CompFlags::SIMPLE_BUILDER)
            && !this.flags.contains(CompFlags::PROTOTYPE_ONLY)
        {
            self.diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "`#[builder(simple)]` requires `#[prototype_only]`".to_string(),
                code: None,
                spans: span_to_codemap(comp.orig_path.span(), self.file)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);
        }

        {
            // Definite field values aren't allowed in `#[prototype_only]`
            // components because Designer will never actually use them.
            //
            // For other components, indefinite field values aren't allowed
            // because Designer will need definite values.
            let needs_indefinite_values = this.flags.contains(CompFlags::PROTOTYPE_ONLY);

            let bad_fields: Vec<_> = comp
                .items
                .iter()
                .filter_map(|item| try_match!(parser::CompItem::Field(field) = item).ok())
                .filter(|field| match &field.dyn_expr {
                    Some(parser::FieldInit::Definite(_)) if needs_indefinite_values => true,
                    Some(parser::FieldInit::Indefinite { .. }) if !needs_indefinite_values => true,
                    _ => false,
                })
                .collect();

            if !bad_fields.is_empty() {
                self.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: if needs_indefinite_values {
                        "Fields cannot have a definite value in a `#[prototype_only]` component"
                    }  else {
                        "Fields cannot have a indefinite value in a non-`#[prototype_only]` component"
                    }
                            .to_string(),
                    code: None,
                    spans: bad_fields.iter().filter_map(|field|span_to_codemap(field.ident.span(), self.file))
                    .map(|span|SpanLabel {
                            span,
                            label: None,
                            style: SpanStyle::Primary,
                        }).collect(),
                }]);
            }
        }

        this
    }

    fn analyze_comp_builder_attr(
        &mut self,
        this: &mut CompDef<'_>,
        input: proc_macro2::TokenStream,
    ) {
        struct BuilderAttr;

        mod kw {
            syn::custom_keyword!(simple);
        }

        impl Parse for BuilderAttr {
            fn parse(input: ParseStream) -> Result<Self> {
                let content;
                syn::parenthesized!(content in input);

                // currently, only `builder(simple)` is supported
                let _: kw::simple = content.parse()?;

                if !content.is_empty() {
                    return Err(content.error("Unexpected token"));
                }
                Ok(BuilderAttr)
            }
        }

        if let Err(e) = syn::parse2::<BuilderAttr>(input) {
            emit_syn_errors_as_diag(e, self.diag, self.file);
        }

        this.flags |= CompFlags::SIMPLE_BUILDER;
    }

    fn analyze_comp_item<'a>(
        &mut self,
        item: &'a parser::CompItem,
        out_lifted_fields: &mut Vec<FieldDef<'a>>,
        reloc: impl FnMut(CompReloc),
        default_vis_path: &Box<Path>,
    ) -> CompItemDef<'a> {
        match item {
            parser::CompItem::Field(i) => CompItemDef::Field(self.analyze_field(
                i,
                out_lifted_fields,
                reloc,
                default_vis_path,
            )),
            parser::CompItem::On(i) => CompItemDef::On(self.analyze_on(i)),
            parser::CompItem::Event(i) => {
                CompItemDef::Event(self.analyze_event(i, default_vis_path))
            }
        }
    }

    fn analyze_field<'a>(
        &mut self,
        item: &'a parser::CompItemField,
        out_lifted_fields: &mut Vec<FieldDef<'a>>,
        mut reloc: impl FnMut(CompReloc),
        default_vis_path: &Box<Path>,
    ) -> FieldDef<'a> {
        let mut accessors;
        let default_accessors = match item.field_ty {
            FieldType::Const => FieldAccessors::default_const(Visibility::from_syn(
                &item.vis,
                default_vis_path,
                self.file,
            )),
            FieldType::Prop => FieldAccessors::default_prop(Visibility::from_syn(
                &item.vis,
                default_vis_path,
                self.file,
            )),
            FieldType::Wire => FieldAccessors::default_wire(Visibility::from_syn(
                &item.vis,
                default_vis_path,
                self.file,
            )),
        };
        if let Some(syn_accessors) = &item.accessors {
            accessors = FieldAccessors::default_none();

            let mut spans: [Vec<_>; 3] = Default::default();

            for syn_accessor in syn_accessors.iter() {
                let (acc_ty, span) = match syn_accessor {
                    parser::FieldAccessor::Set { set_token, vis } => {
                        accessors.set = Some(FieldSetter {
                            vis: Visibility::from_syn(vis, default_vis_path, self.file),
                        });
                        (0, set_token.span())
                    }
                    parser::FieldAccessor::Get {
                        get_token,
                        vis,
                        mode,
                    } => {
                        accessors.get = Some(FieldGetter {
                            vis: Visibility::from_syn(vis, default_vis_path, self.file),
                            mode: mode
                                .unwrap_or_else(|| default_accessors.get.as_ref().unwrap().mode),
                        });
                        (1, get_token.span())
                    }
                    parser::FieldAccessor::Watch {
                        watch_token,
                        vis,
                        mode,
                    } => {
                        let event_span = match mode {
                            parser::FieldWatchMode::Event { event } => {
                                // Lookup the event name later
                                reloc(CompReloc::FieldWatchEvent {
                                    item_i: None,
                                    ident: Ident::from_syn(event, self.file),
                                });

                                span_to_codemap(event.span(), self.file)
                            }
                        };

                        accessors.watch = Some(FieldWatcher {
                            vis: Visibility::from_syn(vis, default_vis_path, self.file),
                            event_item_i: 0, // set later with `CompReloc::FieldWatchEvent`
                            event_span,
                        });

                        (2, watch_token.span())
                    }
                };

                spans[acc_ty].push(span);
            }

            // Deny accessors disallowed for the field type
            let deny_acc_ty: &[usize] = match item.field_ty {
                FieldType::Const => &[2],
                FieldType::Prop => &[],
                FieldType::Wire => &[0],
            };

            // Deny duplicate accessors
            let acc_names = ["set", "get", "watch"];
            for (i, (spans, acc_name)) in spans.iter().zip(&acc_names).enumerate() {
                let codemap_spans = spans
                    .iter()
                    .filter_map(|&span| span_to_codemap(span, self.file))
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect();

                if spans.len() > 1 {
                    self.diag.emit(&[Diagnostic {
                        level: Level::Error,
                        message: format!("`{}` accessor is specified for multiple times", acc_name),
                        code: None,
                        spans: codemap_spans,
                    }]);
                } else if spans.len() > 0 && deny_acc_ty.contains(&i) {
                    self.diag.emit(&[Diagnostic {
                        level: Level::Error,
                        message: format!(
                            "`{}` accessor is not allowed for `{}`",
                            acc_name, item.field_ty
                        ),
                        code: None,
                        spans: codemap_spans,
                    }]);
                }
            }
        } else {
            accessors = default_accessors;
        }

        if item.field_ty == FieldType::Prop && accessors.set.is_none() {
            self.diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "Props must have a setter".to_string(),
                code: None,
                spans: span_to_codemap(item.ident.span(), self.file)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);
        } else if item.field_ty != FieldType::Wire
            && accessors.set.is_none()
            && item.dyn_expr.is_none()
        {
            // `const` and `prop` without a default value nor a setter are impossible
            // to initialize
            self.diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "Must have a default value or a setter \
                          because otherwise it's impossible to initialize"
                    .to_string(),
                code: None,
                spans: span_to_codemap(item.ident.span(), self.file)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);
        }

        let ty = if let Some(mut ty) = item.ty.clone() {
            if let Some(parser::FieldInit::Definite(parser::DynExpr::ObjInit(_))) = item.dyn_expr {
                // Because we can't check if the type is compatible with
                // the object literal in a reliable way.
                self.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: "Type mustn't be specified if the initializer is an object literal"
                        .to_string(),
                    code: None,
                    spans: span_to_codemap(item.ident.span(), self.file)
                        .map(|span| SpanLabel {
                            span,
                            label: None,
                            style: SpanStyle::Primary,
                        })
                        .into_iter()
                        .collect(),
                }]);
            }

            // `'static` lifetime elision
            syn::visit_mut::visit_type_mut(
                &mut InferStaticLifetime {
                    bound: HashMap::new(),
                    file: self.file,
                    diag: self.diag,
                },
                &mut ty,
            );

            Some(ty)
        } else if let Some(parser::FieldInit::Definite(parser::DynExpr::ObjInit(init))) =
            &item.dyn_expr
        {
            if accessors.set.is_some() {
                self.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: "Can't have a setter if the initializer is an object literal"
                        .to_string(),
                    code: None,
                    spans: span_to_codemap(item.ident.span(), self.file)
                        .map(|span| SpanLabel {
                            span,
                            label: None,
                            style: SpanStyle::Primary,
                        })
                        .into_iter()
                        .collect(),
                }]);
            }

            let mut path = init.path.clone();
            path_remove_trailing_new(&mut path);

            Some(syn::Type::Path(syn::TypePath { qself: None, path }))
        } else {
            self.diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "Type must be specified unless the initializer is an object literal"
                    .to_string(),
                code: None,
                spans: span_to_codemap(item.ident.span(), self.file)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);

            None
        };

        if item.dyn_expr.is_none() && item.field_ty == parser::FieldType::Wire {
            self.diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "A value is required for this field type".to_string(),
                code: None,
                spans: span_to_codemap(item.ident.span(), self.file)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);
        }

        let mut doc_attrs = Vec::new();

        for attr in item.attrs.iter() {
            if attr.path.is_ident("doc") {
                match DocAttr::from_syn(attr, self.file) {
                    Ok(da) => doc_attrs.push(da),
                    Err(e) => emit_syn_errors_as_diag(e, self.diag, self.file),
                }
            } else {
                self.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: "Unknown field attribute".to_string(),
                    code: None,
                    spans: span_to_codemap(attr.path.span(), self.file)
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

        FieldDef {
            vis: Visibility::from_syn(&item.vis, default_vis_path, self.file),
            doc_attrs,
            field_ty: item.field_ty,
            flags: FieldFlags::empty(),
            ident: Ident::from_syn(&item.ident, self.file),
            ty,
            accessors,
            value: item.dyn_expr.as_ref().map(|init| match init {
                parser::FieldInit::Definite(d) => {
                    if item.field_ty == FieldType::Const {
                        match d {
                            parser::DynExpr::Func(func) => DynExpr::Func(self.analyze_func(func)),
                            parser::DynExpr::ObjInit(init) => DynExpr::ObjInit(
                                self.analyze_obj_init(init, default_vis_path, out_lifted_fields),
                            ),
                        }
                    } else {
                        // `ObjInit` is not allowed for non-`const` fields
                        DynExpr::Func(self.analyze_dyn_expr_as_func(
                            d,
                            default_vis_path,
                            out_lifted_fields,
                        ))
                    }
                }
                parser::FieldInit::Indefinite { .. } => {
                    // Assign a dummy expression
                    DynExpr::Func(Func {
                        inputs: Vec::new(),
                        body: syn::Expr::Verbatim(proc_macro2::TokenStream::new()),
                    })
                }
            }),
            syn: Some(item),
        }
    }

    fn analyze_on<'a>(&mut self, item: &'a parser::CompItemOn) -> OnDef<'a> {
        OnDef {
            triggers: item
                .triggers
                .iter()
                .map(|tr| self.analyze_trigger(tr))
                .collect(),
            func: self.analyze_func(&item.func),
            syn: item,
        }
    }

    fn analyze_event<'a>(
        &mut self,
        item: &'a parser::CompItemEvent,
        default_vis_path: &Box<Path>,
    ) -> EventDef<'a> {
        let mut doc_attrs = Vec::new();

        for attr in item.attrs.iter() {
            if attr.path.is_ident("doc") {
                match DocAttr::from_syn(attr, self.file) {
                    Ok(da) => doc_attrs.push(da),
                    Err(e) => emit_syn_errors_as_diag(e, self.diag, self.file),
                }
            } else {
                self.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: "Unknown event attribute".to_string(),
                    code: None,
                    spans: span_to_codemap(attr.path.span(), self.file)
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

        EventDef {
            vis: Visibility::from_syn(&item.vis, default_vis_path, self.file),
            doc_attrs,
            ident: Ident::from_syn(&item.ident, self.file),
            inputs: item.inputs.iter().cloned().collect(),
            syn: item,
        }
    }

    // Lower `DynExpr` in a context where `Func` is required.
    fn analyze_dyn_expr_as_func(
        &mut self,
        d: &parser::DynExpr,
        default_vis_path: &Box<Path>,
        out_lifted_fields: &mut Vec<FieldDef<'_>>,
    ) -> Func {
        match d {
            parser::DynExpr::Func(func) => self.analyze_func(func),
            parser::DynExpr::ObjInit(init) => {
                self.analyze_obj_init_as_func(init, default_vis_path, out_lifted_fields)
            }
        }
    }

    fn analyze_func(&mut self, func: &parser::Func) -> Func {
        let mut this = Func {
            inputs: Vec::new(),
            body: func.body.clone(),
        };

        // Create `FuncInput` from occurrences of `get!`
        struct ReplaceInlineInput<'a, 'b> {
            inputs: &'a mut Vec<parser::FuncInput>,
            file: &'a codemap::File,
            diag: &'a mut Diag<'b>,
        }

        fn inline_input_name(i: usize) -> String {
            format!("_input_{}", i)
        }

        impl ReplaceInlineInput<'_, '_> {
            fn replace_get(&mut self, input: TokenStream) -> syn::Ident {
                let name = inline_input_name(self.inputs.len());
                let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());

                struct InlineFuncInput(parser::FuncInput);

                impl Parse for InlineFuncInput {
                    fn parse(input: ParseStream) -> Result<Self> {
                        input.call(parser::FuncInput::parse_inline).map(Self)
                    }
                }

                let mut input = match syn::parse2::<InlineFuncInput>(input) {
                    Ok(parsed) => parsed.0,
                    Err(e) => {
                        emit_syn_errors_as_diag(e, self.diag, self.file);
                        return ident;
                    }
                };

                // Give an explicit name for the input
                input.rename = Some((
                    syn::Token![as](proc_macro2::Span::call_site()),
                    ident.clone(),
                ));

                // TODO: Disallow referencing an event by an inline input

                self.inputs.push(input);

                ident
            }

            fn visit_token_stream(&mut self, i: &mut TokenStream) {
                let tokens = std::mem::replace(i, TokenStream::new());
                let mut new_tokens = TokenStream::new();

                // The state machine to detect `get` `!` `(...)`
                let mut pending = ArrayVec::<[TokenTree; 2]>::new();

                for tree in tokens {
                    match tree {
                        TokenTree::Ident(ref idt) if *idt == "get" => {
                            new_tokens.extend(pending.drain(..));
                            pending.push(tree);
                        }
                        TokenTree::Punct(ref pun) if pun.as_char() == '!' && pending.len() == 1 => {
                            pending.push(tree);
                        }
                        TokenTree::Group(ref g)
                            if g.delimiter() == proc_macro2::Delimiter::Parenthesis
                                && pending.len() == 2 =>
                        {
                            // Found `get!(...)`
                            let ident = self.replace_get(g.stream());
                            new_tokens.extend(Some(TokenTree::Ident(ident)));
                            pending.clear();
                        }
                        TokenTree::Group(ref g) => {
                            new_tokens.extend(pending.drain(..));

                            // Recurse into the group
                            let mut inner_tokens = g.stream();
                            self.visit_token_stream(&mut inner_tokens);
                            let mut new_g = proc_macro2::Group::new(g.delimiter(), inner_tokens);
                            new_g.set_span(g.span());

                            new_tokens.extend(Some(TokenTree::Group(new_g)));
                        }
                        _ => {
                            new_tokens.extend(pending.drain(..));
                            new_tokens.extend(Some(tree));
                        }
                    }
                }

                new_tokens.extend(pending.drain(..));

                *i = new_tokens;
            }
        }

        impl syn::visit_mut::VisitMut for ReplaceInlineInput<'_, '_> {
            fn visit_expr_mut(&mut self, i: &mut syn::Expr) {
                if let syn::Expr::Macro(m) = i {
                    let m = &mut m.mac;
                    if m.path.is_ident("get")
                        && try_match!(syn::MacroDelimiter::Paren(_) = &m.delimiter).is_ok()
                    {
                        // Replace this `get!(...)`
                        let ident = self.replace_get(std::mem::take(&mut m.tokens));

                        *i = syn::Expr::Path(syn::ExprPath {
                            attrs: vec![],
                            qself: None,
                            path: ident.into(),
                        });
                    } else if m.path.segments.len() > 1
                        && m.path.segments.last().unwrap().ident == "new"
                    {
                        // obj-init literal is not allowed to appear as an
                        // arbitrary subexpression (for now)
                        self.diag.emit(&[Diagnostic {
                            level: Level::Error,
                            message: "`Component::new!` is unsupported in this position"
                                .to_string(),
                            code: None,
                            spans: span_to_codemap(m.path.span(), self.file)
                                .map(|span| SpanLabel {
                                    span,
                                    label: None,
                                    style: SpanStyle::Primary,
                                })
                                .into_iter()
                                .collect(),
                        }]);
                    } else {
                        // `get!` may be used inside this macro invocation.
                        self.visit_token_stream(&mut m.tokens);
                    }
                } else {
                    syn::visit_mut::visit_expr_mut(self, i);
                }
            }
        }

        let mut inline_inputs: Vec<parser::FuncInput> = Vec::new();

        syn::visit_mut::VisitMut::visit_expr_mut(
            &mut ReplaceInlineInput {
                inputs: &mut inline_inputs,
                file: self.file,
                diag: self.diag,
            },
            &mut this.body,
        );

        this.inputs
            .extend(inline_inputs.iter().map(|i| self.analyze_func_input(i)));

        this
    }

    fn analyze_func_input(&mut self, func: &parser::FuncInput) -> FuncInput {
        let ident = if let Some((_, i)) = &func.rename {
            Ident::from_syn(i, self.file)
        } else {
            // Use the last component as `ident`
            match func.input.selectors.last().unwrap() {
                parser::InputSelector::Field { ident, .. } => Ident::from_syn(ident, self.file),
            }
        };

        FuncInput {
            by_ref: func.by_ref.is_some(),
            input: self.analyze_input(&func.input),
            ident,
        }
    }

    fn analyze_trigger(&mut self, tr: &parser::Trigger) -> Trigger {
        match tr {
            parser::Trigger::Init(i) => Trigger::Init(InitTrigger {
                span: span_to_codemap(i.span(), self.file),
            }),
            parser::Trigger::Input(i) => Trigger::Input(self.analyze_input(i)),
        }
    }

    fn analyze_input(&mut self, input: &parser::Input) -> Input {
        let mut selectors = &input.selectors[..];
        let origin = if selectors[0].is_field_with_ident("self") {
            // e.g., `this.prop1`
            selectors = &selectors[1..];
            InputOrigin::This
        } else if selectors[0].is_field_with_ident("event") {
            // e.g., `event.mouse_position`

            if selectors.len() == 0 {
                self.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: "Parameter name must be specified (e.g., `event.wm`)".to_string(),
                    code: None,
                    spans: span_to_codemap(input.span(), self.file)
                        .map(|span| SpanLabel {
                            span,
                            label: None,
                            style: SpanStyle::Primary,
                        })
                        .into_iter()
                        .collect(),
                }]);
            } else if selectors.len() > 2 {
                self.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: "Can't specify subfields of an event parameter".to_string(),
                    code: None,
                    spans: span_to_codemap(selectors[2].span(), self.file)
                        .map(|span| SpanLabel {
                            span,
                            label: None,
                            style: SpanStyle::Primary,
                        })
                        .into_iter()
                        .collect(),
                }]);
            }

            selectors = &selectors[1..2];
            InputOrigin::Event
        } else {
            // The origin can be elided like `prop1`
            InputOrigin::This
        };

        Input {
            origin,
            selectors: selectors
                .iter()
                .map(|s| match s {
                    parser::InputSelector::Field { ident, .. } => Ident::from_syn(ident, self.file),
                })
                .collect(),
            index: self.get_next_input_index(),
            span: span_to_codemap(input.span(), self.file),
        }
    }

    fn analyze_obj_init(
        &mut self,
        init: &parser::ObjInit,
        default_vis_path: &Box<Path>,
        out_lifted_fields: &mut Vec<FieldDef<'_>>,
    ) -> ObjInit {
        let mut path = Path::from_syn_with_span_of(&init.path, &init.orig_path, self.file);

        path_remove_trailing_new(&mut path.syn_path);

        ObjInit {
            path,
            fields: init
                .fields
                .iter()
                .map(|field| ObjInitField {
                    ident: Ident::from_syn(&field.ident, self.file),
                    value: if let Some(value) = &field.value {
                        self.analyze_dyn_expr_as_func(
                            &value.dyn_expr,
                            default_vis_path,
                            out_lifted_fields,
                        )
                    } else {
                        self.mk_func_with_named_input(field.ident.clone())
                    },
                })
                .collect(),
        }
    }

    fn analyze_obj_init_as_func(
        &mut self,
        init: &parser::ObjInit,
        default_vis_path: &Box<Path>,
        out_lifted_fields: &mut Vec<FieldDef<'_>>,
    ) -> Func {
        let obj_init = self.analyze_obj_init(init, default_vis_path, out_lifted_fields);

        // Instantiate the given `ObjInit` as a `const` field, which is the
        // only place where `ObjInit` is allowed to appear.
        //
        //   Before:
        //     pub const label = HView { const layout = EmptyLayout {} };
        //   After:
        //     pub const label = HView { const layout = |this.__lifted_0| __lifted_0 };
        //     const __lifted_0 = EmptyLayout {};
        //
        let lifted_field_name = format!("__lifted_{}", out_lifted_fields.len());

        let mut ty_path = init.path.clone();
        path_remove_trailing_new(&mut ty_path);

        out_lifted_fields.push(FieldDef {
            vis: Visibility::Restricted {
                span: None,
                path: default_vis_path.clone(),
            },
            doc_attrs: Vec::new(),
            field_ty: FieldType::Const,
            flags: FieldFlags::empty(),
            ident: Ident {
                sym: lifted_field_name.clone(),
                span: None,
            },
            ty: Some(syn::Type::Path(syn::TypePath {
                qself: None,
                path: ty_path,
            })),
            accessors: FieldAccessors::default_none(),
            value: Some(DynExpr::ObjInit(obj_init)),
            syn: None,
        });

        // `|this.__lifted_N| __lifted_N`
        Func {
            inputs: vec![FuncInput {
                by_ref: false,
                input: Input {
                    origin: InputOrigin::This,
                    selectors: vec![Ident {
                        span: None,
                        sym: lifted_field_name.clone(),
                    }],
                    index: self.get_next_input_index(),
                    span: None,
                },
                ident: Ident {
                    span: None,
                    sym: lifted_field_name.clone(),
                },
            }],
            body: syn::Expr::Path(syn::ExprPath {
                attrs: vec![],
                qself: None,
                path: syn::Ident::new(&lifted_field_name, proc_macro2::Span::call_site()).into(),
            }),
        }
    }

    /// Construct a `Func` that looks like it was created from `get!(ident)`.
    fn mk_func_with_named_input(&mut self, ident: syn::Ident) -> Func {
        // `|this.ident as ident| ident`
        Func {
            inputs: vec![FuncInput {
                by_ref: false,
                input: Input {
                    origin: InputOrigin::This,
                    selectors: vec![Ident::from_syn(&ident, self.file)],
                    index: self.get_next_input_index(),
                    span: None,
                },
                ident: Ident::from_syn(&ident, self.file),
            }],
            body: syn::Expr::Path(syn::ExprPath {
                attrs: vec![],
                qself: None,
                path: ident.into(),
            }),
        }
    }

    fn get_next_input_index(&mut self) -> usize {
        let ret = self.next_input_index;
        self.next_input_index += 1;
        ret
    }
}

/// Implements `syn::visit_mut::VisitMut` to deduce all unspecified lifetimes
/// in a `Type` as `'static`. Ignores lifetime positions which are automatically
/// quantified by the compiler (e.g., everything inside `fn`). Elided lifetime
/// parameters (e.g., `std::cell::Ref<T>`; unidiomatic in Rust 2018) are left
/// untouched because we have no access to the type information.
pub struct InferStaticLifetime<'a, 'b> {
    bound: HashMap<syn::Ident, usize>,
    file: &'a codemap::File,
    diag: &'a mut Diag<'b>,
}

impl syn::visit_mut::VisitMut for InferStaticLifetime<'_, '_> {
    fn visit_lifetime_mut(&mut self, i: &mut syn::Lifetime) {
        if i.ident == "_" {
            *i = syn::Lifetime::new("'static", proc_macro2::Span::call_site());
        } else if i.ident != "static" {
            // Is the variable bound?
            if self.bound.get(&i.ident).cloned().unwrap_or(0) > 0 {
                return;
            }

            self.diag.emit(&[Diagnostic {
                level: Level::Error,
                message: format!("Undefined lifetime variable `{}`", i.to_token_stream()),
                code: None,
                spans: span_to_codemap(i.ident.span(), self.file)
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

    fn visit_type_reference_mut(&mut self, i: &mut syn::TypeReference) {
        syn::visit_mut::visit_type_reference_mut(self, i);

        if i.lifetime.is_none() {
            i.lifetime = Some(syn::Lifetime::new(
                "'static",
                proc_macro2::Span::call_site(),
            ));
        }
    }

    fn visit_type_bare_fn_mut(&mut self, _: &mut syn::TypeBareFn) {
        // Stop recursion; the Rust compiler automatically adds `for <'a>`
        // for `fn(&T) -> &S`
    }

    fn visit_parenthesized_generic_arguments_mut(
        &mut self,
        _: &mut syn::ParenthesizedGenericArguments,
    ) {
        // Stop recursion; the Rust compiler automatically adds `for <'a>`
        // for `FnMut(&T) -> &S`
    }

    fn visit_trait_bound_mut(&mut self, tb: &mut syn::TraitBound) {
        if let Some(lifetimes) = &tb.lifetimes {
            for lifetime_def in lifetimes.lifetimes.iter() {
                *self
                    .bound
                    .entry(lifetime_def.lifetime.ident.clone())
                    .or_default() += 1;
            }
            self.visit_path_mut(&mut tb.path);
            for lifetime_def in lifetimes.lifetimes.iter() {
                *self.bound.get_mut(&lifetime_def.lifetime.ident).unwrap() -= 1;
            }
        } else {
            syn::visit_mut::visit_trait_bound_mut(self, tb);
        }
    }
}

/// Remove the trailing `::new` from a given path.
fn path_remove_trailing_new(path: &mut syn::Path) {
    assert_eq!(path_remove_last_segment(path).unwrap().ident, "new");
}

/// Remove the last segment from a given path.
fn path_remove_last_segment(path: &mut syn::Path) -> Option<syn::PathSegment> {
    // Remove the last segment
    let seg = path.segments.pop()?.into_value();

    // Remove the trailing `::`
    let last = path.segments.pop().unwrap().into_value();
    path.segments.push_value(last);

    Some(seg)
}
