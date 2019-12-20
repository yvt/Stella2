use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use quote::ToTokens;
use std::fmt;
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
    Inherited,
}

impl Visibility {
    pub fn from_syn(i: &syn::Visibility, file: &codemap::File) -> Self {
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
            syn::Visibility::Inherited => Visibility::Inherited,
        }
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public { .. } => write!(f, "pub"),
            Self::Crate { .. } => write!(f, "pub(crate)"),
            Self::Restricted { path, .. } => write!(f, "pub(in {})", path),
            Self::Inherited => Ok(()),
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

pub struct CompDef<'a> {
    pub flags: CompFlags,
    pub vis: Visibility,
    pub path: Path,
    /// The last component of `path`.
    pub ident: Ident,
    pub items: Vec<CompItemDef<'a>>,
    pub syn: &'a parser::Comp,
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
    pub field_ty: FieldType,
    pub ident: Ident,
    pub value: Func,
}

/// Convert the AST to a slightly-higher-level representation. See the code
/// comments to figure out what is done and what is not.
pub fn analyze_comp<'a>(
    comp: &'a parser::Comp,
    file: &codemap::File,
    diag: &mut Diag,
) -> CompDef<'a> {
    AnalyzeCtx {
        file,
        diag,
        next_input_index: 0,
    }
    .analyze_comp(comp)
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
    fn analyze_comp<'a>(&mut self, comp: &'a parser::Comp) -> CompDef<'a> {
        let mut lifted_fields = Vec::new();
        let mut relocs = Vec::new();

        let mut this = CompDef {
            flags: CompFlags::empty(),
            vis: Visibility::from_syn(&comp.vis, self.file),
            path: Path::from_syn_with_span_of(&comp.path, &comp.orig_path, self.file),
            ident: Ident::from_syn(&comp.path.segments.last().unwrap().ident, self.file),
            items: comp
                .items
                .iter()
                .enumerate()
                .map(|(item_i, item)| {
                    self.analyze_comp_item(item, &mut lifted_fields, |reloc| {
                        relocs.push(reloc.with_item_i(item_i))
                    })
                })
                .collect(),
            syn: comp,
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
                // TODO: handle doc comments
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
                message: "`#[builder(simple)]` requires `#[prototype_only]".to_string(),
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
    ) -> CompItemDef<'a> {
        match item {
            parser::CompItem::Field(i) => {
                CompItemDef::Field(self.analyze_field(i, out_lifted_fields, reloc))
            }
            parser::CompItem::On(i) => CompItemDef::On(self.analyze_on(i)),
            parser::CompItem::Event(i) => CompItemDef::Event(self.analyze_event(i)),
        }
    }

    fn analyze_field<'a>(
        &mut self,
        item: &'a parser::CompItemField,
        out_lifted_fields: &mut Vec<FieldDef<'a>>,
        mut reloc: impl FnMut(CompReloc),
    ) -> FieldDef<'a> {
        let mut accessors;
        let default_accessors = match item.field_ty {
            FieldType::Const => {
                FieldAccessors::default_const(Visibility::from_syn(&item.vis, self.file))
            }
            FieldType::Prop => {
                FieldAccessors::default_prop(Visibility::from_syn(&item.vis, self.file))
            }
            FieldType::Wire => {
                FieldAccessors::default_wire(Visibility::from_syn(&item.vis, self.file))
            }
        };
        if let Some(syn_accessors) = &item.accessors {
            accessors = FieldAccessors::default_none();

            let mut spans: [Vec<_>; 3] = Default::default();

            for syn_accessor in syn_accessors.iter() {
                let (acc_ty, span) = match syn_accessor {
                    parser::FieldAccessor::Set { set_token, vis } => {
                        accessors.set = Some(FieldSetter {
                            vis: Visibility::from_syn(vis, self.file),
                        });
                        (0, set_token.span())
                    }
                    parser::FieldAccessor::Get {
                        get_token,
                        vis,
                        mode,
                    } => {
                        accessors.get = Some(FieldGetter {
                            vis: Visibility::from_syn(vis, self.file),
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
                            vis: Visibility::from_syn(vis, self.file),
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

        // `const` and `prop` without a default value nor a setter are impossible
        // to initialize
        if item.field_ty != FieldType::Wire && accessors.set.is_none() && item.dyn_expr.is_none() {
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

        let ty = if let Some(ty) = item.ty.clone() {
            if let Some(parser::DynExpr::ObjInit(_)) = item.dyn_expr {
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

            Some(ty)
        } else if let Some(parser::DynExpr::ObjInit(init)) = &item.dyn_expr {
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

            Some(syn::Type::Path(syn::TypePath {
                qself: None,
                path: init.path.clone(),
            }))
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

        for attr in item.attrs.iter() {
            if attr.path.is_ident("doc") {
                // TODO: handle doc comments
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
            vis: Visibility::from_syn(&item.vis, self.file),
            field_ty: item.field_ty,
            flags: FieldFlags::empty(),
            ident: Ident::from_syn(&item.ident, self.file),
            ty,
            accessors,
            value: item.dyn_expr.as_ref().map(|d| {
                if item.field_ty == FieldType::Const {
                    match d {
                        parser::DynExpr::Func(func) => DynExpr::Func(self.analyze_func(func)),
                        parser::DynExpr::ObjInit(init) => {
                            DynExpr::ObjInit(self.analyze_obj_init(init, out_lifted_fields))
                        }
                    }
                } else {
                    // `ObjInit` is not allowed for non-`const` fields
                    DynExpr::Func(self.analyze_dyn_expr_as_func(d, out_lifted_fields))
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

    fn analyze_event<'a>(&mut self, item: &'a parser::CompItemEvent) -> EventDef<'a> {
        EventDef {
            vis: Visibility::from_syn(&item.vis, self.file),
            ident: Ident::from_syn(&item.ident, self.file),
            inputs: item.inputs.iter().cloned().collect(),
            syn: item,
        }
    }

    // Lower `DynExpr` in a context where `Func` is required.
    fn analyze_dyn_expr_as_func(
        &mut self,
        d: &parser::DynExpr,
        out_lifted_fields: &mut Vec<FieldDef<'_>>,
    ) -> Func {
        match d {
            parser::DynExpr::Func(func) => self.analyze_func(func),
            parser::DynExpr::ObjInit(init) => {
                self.analyze_obj_init_as_func(init, out_lifted_fields)
            }
        }
    }

    fn analyze_func(&mut self, func: &parser::Func) -> Func {
        // TODO: Check `FuncInput::rename` collision
        Func {
            inputs: func
                .inputs
                .iter()
                .map(|i| self.analyze_func_input(i))
                .collect(),
            body: func.body.clone(),
        }
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
        let origin = if selectors[0].is_field_with_ident("this") {
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
        out_lifted_fields: &mut Vec<FieldDef<'_>>,
    ) -> ObjInit {
        ObjInit {
            path: Path::from_syn_with_span_of(&init.path, &init.orig_path, self.file),
            fields: init
                .fields
                .iter()
                .map(|field| ObjInitField {
                    field_ty: field.field_ty,
                    ident: Ident::from_syn(&field.ident, self.file),
                    value: self.analyze_dyn_expr_as_func(&field.dyn_expr, out_lifted_fields),
                })
                .collect(),
        }
    }

    fn analyze_obj_init_as_func(
        &mut self,
        init: &parser::ObjInit,
        out_lifted_fields: &mut Vec<FieldDef<'_>>,
    ) -> Func {
        let obj_init = self.analyze_obj_init(init, out_lifted_fields);

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

        out_lifted_fields.push(FieldDef {
            vis: Visibility::Inherited,
            field_ty: FieldType::Const,
            flags: FieldFlags::empty(),
            ident: Ident {
                sym: lifted_field_name.clone(),
                span: None,
            },
            ty: Some(syn::Type::Path(syn::TypePath {
                qself: None,
                path: init.path.clone(),
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

    fn get_next_input_index(&mut self) -> usize {
        let ret = self.next_input_index;
        self.next_input_index += 1;
        ret
    }
}
