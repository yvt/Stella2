use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use syn::spanned::Spanned;

use super::{diag::Diag, parser, parser::span_to_codemap};

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

pub struct CompDef<'a> {
    pub flags: CompFlags,
    pub vis: syn::Visibility,
    pub path: syn::Path,
    pub items: Vec<CompItemDef<'a>>,
    pub syn: &'a parser::Comp,
}

pub use crate::metadata::CompFlags;

pub enum CompItemDef<'a> {
    Field(FieldDef<'a>),
    On(OnDef<'a>),
    Event(EventDef<'a>),
}

pub struct FieldDef<'a> {
    pub vis: syn::Visibility, // FIXME: maybe not needed
    pub field_ty: FieldType,
    pub flags: FieldFlags,
    pub ident: Ident,
    /// The type of the field's value. Set by the analysis unless there was
    /// an error.
    pub ty: Option<syn::Type>,
    pub accessors: FieldAccessors,
    /// - Can be `None` unless `field_ty` is `Wire`. For `Const`, `None` means
    ///   the value must be supplied via the constructor, and for `Prop` it
    ///   means it's initialized by `Default::default()`.
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
    fn default_const(vis: syn::Visibility) -> Self {
        Self {
            set: Some(FieldSetter { vis: vis.clone() }),
            get: Some(FieldGetter {
                vis,
                mode: FieldGetMode::Borrow,
            }),
            watch: None,
        }
    }

    fn default_prop(vis: syn::Visibility) -> Self {
        Self {
            set: Some(FieldSetter { vis: vis.clone() }),
            get: Some(FieldGetter {
                vis,
                mode: FieldGetMode::Clone,
            }),
            watch: None,
        }
    }

    fn default_wire(vis: syn::Visibility) -> Self {
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
    pub vis: syn::Visibility,
}

#[derive(Clone)]
pub struct FieldGetter {
    pub vis: syn::Visibility,
    pub mode: FieldGetMode,
}

pub use self::parser::FieldGetMode;

pub struct FieldWatcher {
    pub vis: syn::Visibility,
    pub event: Ident,
}

pub struct OnDef<'a> {
    pub triggers: Vec<Trigger>,
    pub func: Func,
    pub syn: &'a parser::CompItemOn,
}

pub struct EventDef<'a> {
    pub vis: syn::Visibility,
    pub ident: Ident,
    pub inputs: Vec<syn::FnArg>,
    pub syn: &'a parser::CompItemEvent,
}

pub enum DynExpr {
    Func(Func),
    ObjInit(ObjInit),
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
    Init,
    Input(Input),
}

pub struct Input {
    pub origin: InputOrigin,
    pub selectors: Vec<Ident>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOrigin {
    This,
    Event,
}

pub struct ObjInit {
    pub path: syn::Path,
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
    AnalyzeCtx { file, diag }.analyze_comp(comp)
}

struct AnalyzeCtx<'a> {
    file: &'a codemap::File,
    diag: &'a mut Diag,
}

impl AnalyzeCtx<'_> {
    fn analyze_comp<'a>(&mut self, comp: &'a parser::Comp) -> CompDef<'a> {
        let mut lifted_fields = Vec::new();

        let mut this = CompDef {
            flags: CompFlags::empty(),
            vis: comp.vis.clone(),
            path: comp.path.clone(),
            items: comp
                .items
                .iter()
                .map(|i| self.analyze_comp_item(i, &mut lifted_fields))
                .collect(),
            syn: comp,
        };

        this.items
            .extend(lifted_fields.into_iter().map(CompItemDef::Field));

        this
    }

    fn analyze_comp_item<'a>(
        &mut self,
        item: &'a parser::CompItem,
        out_lifted_fields: &mut Vec<FieldDef<'a>>,
    ) -> CompItemDef<'a> {
        match item {
            parser::CompItem::Field(i) => {
                CompItemDef::Field(self.analyze_field(i, out_lifted_fields))
            }
            parser::CompItem::On(i) => CompItemDef::On(self.analyze_on(i)),
            parser::CompItem::Event(i) => CompItemDef::Event(self.analyze_event(i)),
        }
    }

    fn analyze_field<'a>(
        &mut self,
        item: &'a parser::CompItemField,
        out_lifted_fields: &mut Vec<FieldDef<'a>>,
    ) -> FieldDef<'a> {
        let mut accessors;
        let default_accessors = match item.field_ty {
            FieldType::Const => FieldAccessors::default_const(item.vis.clone()),
            FieldType::Prop => FieldAccessors::default_prop(item.vis.clone()),
            FieldType::Wire => FieldAccessors::default_wire(item.vis.clone()),
        };
        if let Some(syn_accessors) = &item.accessors {
            accessors = FieldAccessors::default_none();

            let mut spans: [Vec<_>; 3] = Default::default();

            for syn_accessor in syn_accessors.iter() {
                let (acc_ty, span) = match syn_accessor {
                    parser::FieldAccessor::Set { set_token, vis } => {
                        accessors.set = Some(FieldSetter { vis: vis.clone() });
                        (0, set_token.span())
                    }
                    parser::FieldAccessor::Get {
                        get_token,
                        vis,
                        mode,
                    } => {
                        accessors.get = Some(FieldGetter {
                            vis: vis.clone(),
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
                        accessors.watch = Some(FieldWatcher {
                            vis: vis.clone(),
                            event: match mode {
                                parser::FieldWatchMode::Event { event } => {
                                    Ident::from_syn(event, self.file)
                                }
                            },
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

        let ty = if let Some(ty) = item.ty.clone() {
            Some(ty)
        } else if let Some(parser::DynExpr::ObjInit(init)) = &item.dyn_expr {
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

        FieldDef {
            vis: item.vis.clone(),
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
            vis: item.vis.clone(),
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
            parser::Trigger::Init(_) => Trigger::Init,
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
            selectors = &selectors[1..];
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
        }
    }

    fn analyze_obj_init(
        &mut self,
        init: &parser::ObjInit,
        out_lifted_fields: &mut Vec<FieldDef<'_>>,
    ) -> ObjInit {
        ObjInit {
            path: init.path.clone(),
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
            vis: syn::Visibility::Inherited,
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
}
