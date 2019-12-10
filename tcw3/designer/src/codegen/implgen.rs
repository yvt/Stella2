//! Implementation code generation
use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use quote::ToTokens;
use std::{collections::HashMap, fmt, fmt::Write};

use super::{diag::Diag, sem};
use crate::metadata;

mod analysis;
mod buildergen;
mod evalgen;
mod initgen;
mod iterutils;

/// Paths to standard library items.
mod paths {
    pub const BOX: &str = "::std::boxed::Box";
    pub const CLONE: &str = "::std::clone::Clone";
    pub const OPTION: &str = "::std::option::Option";
    pub const SOME: &str = "::std::option::Option::Some";
    pub const RC: &str = "::std::rc::Rc";
    pub const CELL: &str = "::std::cell::Cell";
    pub const REF_CELL: &str = "::std::cell::RefCell";
    pub const DEFAULT: &str = "::std::default::Default";
    pub const FN: &str = "::std::ops::Fn";
    pub const SUB_LIST: &str = "::tcw3::designer_runtime::SubscriberList";
    pub const UNSET: &str = "::tcw3::designer_runtime::Unset";
}

mod fields {
    pub const SHARED: &str = "shared";
    pub const STATE: &str = "state";
}

pub struct Ctx<'a> {
    /// Contains all loaded crates.
    pub repo: &'a metadata::Repo,

    /// Mapping from imported crate names to indices into `repo.crates`.
    pub imports_crate_i: &'a HashMap<&'a str, usize>,
}

pub fn gen_comp(
    comp: &sem::CompDef<'_>,
    meta_comp: &metadata::CompDef,
    ctx: &Ctx,
    diag: &mut Diag,
) -> String {
    if comp.flags.contains(sem::CompFlags::PROTOTYPE_ONLY) {
        return r#"compile_error!(
            "`designer_impl!` can't generate code because the component is defined with #[prototype_only]"
        )"#.to_string();
    }

    let mut out = String::new();

    let comp_ident = &comp.path.syn_path.segments.last().unwrap().ident;

    // String → index into `comp.items`
    // This also checks duplicate item names.
    let item_name_map = make_name_map(
        comp.items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match item {
                sem::CompItemDef::Field(item) => Some((i, item.ident.clone())),
                sem::CompItemDef::Event(item) => Some((i, item.ident.clone())),
                sem::CompItemDef::On(_) => None,
            }),
        diag,
    );

    if diag.has_error() {
        // Duplicate item names cause may false errors down below, so
        // return early.
        return out;
    }

    // index into `meta_comp.items` → index into `comp.items`
    let item_meta2sem_map: Vec<_> = meta_comp
        .items
        .iter()
        .map(|i| *item_name_map.get(i.ident()).unwrap())
        .collect();

    // Analyze input references
    let analysis = analysis::Analysis::new(comp, meta_comp, ctx, diag);

    // `struct ComponentType`
    // -------------------------------------------------------------------
    writeln!(out, "#[derive(Clone)]").unwrap();
    writeln!(
        out,
        "{vis} struct {ty} {{",
        vis = comp.vis,
        ty = CompTy(comp_ident)
    )
    .unwrap();
    writeln!(
        out,
        "    {field}: {rc}<{ty}>,",
        field = fields::SHARED,
        rc = paths::RC,
        ty = CompSharedTy(comp_ident)
    )
    .unwrap();
    writeln!(out, "}}").unwrap();

    // `struct ComponentTypeShared`
    // -------------------------------------------------------------------
    writeln!(out, "struct {} {{", CompSharedTy(comp_ident)).unwrap();
    writeln!(
        out,
        "    {field}: {cell}<{ty}>,",
        field = fields::STATE,
        cell = paths::REF_CELL,
        ty = CompStateTy(comp_ident)
    )
    .unwrap();

    for item in comp.items.iter() {
        match item {
            sem::CompItemDef::Field(item) => match item.field_ty {
                sem::FieldType::Const => {
                    writeln!(
                        out,
                        "    {ident}: {ty},",
                        ident = InnerValueField(&item.ident.sym),
                        ty = item.ty.to_token_stream()
                    )
                    .unwrap();
                }
                sem::FieldType::Wire => {}
                sem::FieldType::Prop => {
                    // TODO
                }
            },
            sem::CompItemDef::Event(item) => {
                writeln!(
                    out,
                    "    {ident}: {cell}<{list}<{handler}>>,",
                    ident = EventInnerSubList(&item.ident.sym),
                    cell = paths::REF_CELL,
                    list = paths::SUB_LIST,
                    handler = EventBoxHandlerTy(&item.inputs)
                )
                .unwrap();
            }
            sem::CompItemDef::On(_) => {
                // TODO
            }
        }
    }

    writeln!(out, "}}").unwrap();

    // `struct ComponentTypeState`
    // -------------------------------------------------------------------
    writeln!(out, "struct {} {{", CompStateTy(comp_ident)).unwrap();
    for item in comp.items.iter() {
        match item {
            sem::CompItemDef::Field(item) => match item.field_ty {
                sem::FieldType::Const => {}
                sem::FieldType::Wire | sem::FieldType::Prop => {
                    writeln!(
                        out,
                        "    {ident}: {ty},",
                        ident = InnerValueField(&item.ident.sym),
                        ty = item.ty.to_token_stream()
                    )
                    .unwrap();
                }
            },
            sem::CompItemDef::Event(_) => {}
            sem::CompItemDef::On(_) => {}
        }
    }
    writeln!(out, "}}").unwrap();

    // `struct ComponentTypeBuilder`
    // -------------------------------------------------------------------
    buildergen::gen_builder(
        comp,
        meta_comp,
        comp_ident,
        &analysis,
        ctx,
        &item_meta2sem_map,
        diag,
        &mut out,
    );

    // TODO: setters/getters/subscriptions

    out
}

// Lower-level codegen utils
// -------------------------------------------------------------------

macro_rules! fn_fmt_write {
    (|$this:ident| ($($fmt:tt)*)) => {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let $this = self;
            write!(f, $($fmt)*)
        }
    };
}

struct Angle<T>(T);
impl<T: fmt::Display> fmt::Display for Angle<T> {
    fn_fmt_write! { |this| ("<{}>", this.0) }
}

struct CompTy<T>(T);
impl<T: fmt::Display> fmt::Display for CompTy<T> {
    fn_fmt_write! { |this| ("{}", this.0) }
}

struct CompSharedTy<T>(T);
impl<T: fmt::Display> fmt::Display for CompSharedTy<T> {
    fn_fmt_write! { |this| ("{}Shared", this.0) }
}

struct CompStateTy<T>(T);
impl<T: fmt::Display> fmt::Display for CompStateTy<T> {
    fn_fmt_write! { |this| ("{}State", this.0) }
}

struct CompBuilderTy<T>(T);
impl<T: fmt::Display> fmt::Display for CompBuilderTy<T> {
    fn_fmt_write! { |this| ("{}Builder", this.0) }
}

#[derive(Clone)]
struct FactoryGenParamNameForField<T>(T);
impl<T: fmt::Display> fmt::Display for FactoryGenParamNameForField<T> {
    fn_fmt_write! { |this| ("T_{}", this.0) }
}

struct FactorySetterForField<T>(T);
impl<T: fmt::Display> fmt::Display for FactorySetterForField<T> {
    fn_fmt_write! { |this| ("with_{}", this.0) }
}

struct InnerValueField<T>(T);
impl<T: fmt::Display> fmt::Display for InnerValueField<T> {
    fn_fmt_write! { |this| ("value_{}", this.0) }
}

struct EventInnerSubList<T>(T);
impl<T: fmt::Display> fmt::Display for EventInnerSubList<T> {
    fn_fmt_write! { |this| ("subscriptions_{}", this.0) }
}

struct SetterMethod<T>(T);
impl<T: fmt::Display> fmt::Display for SetterMethod<T> {
    fn_fmt_write! { |this| ("set_{}", this.0) }
}

struct EventDynHandlerTy<'a>(&'a [syn::FnArg]);
impl fmt::Display for EventDynHandlerTy<'_> {
    fn_fmt_write! { |this| (
        "dyn {fn}({params})",
        fn = paths::FN,
        params = CommaSeparated(this.0.iter()
            .map(|arg| match arg {
                syn::FnArg::Receiver(_) => unreachable!(),
                syn::FnArg::Typed(pat) => pat.ty.to_token_stream(),
            }))
    ) }
}

struct EventBoxHandlerTy<'a>(&'a [syn::FnArg]);
impl fmt::Display for EventBoxHandlerTy<'_> {
    fn_fmt_write! { |this| (
        "{bx}<{inner}>",
        bx = paths::BOX,
        inner = EventDynHandlerTy(this.0)
    ) }
}

struct TempVar<T>(T);
impl<T: fmt::Display> fmt::Display for TempVar<T> {
    fn_fmt_write! { |this| ("__tmp_{}", this.0) }
}

struct CommaSeparated<T>(T);
impl<T> fmt::Display for CommaSeparated<T>
where
    T: Clone + IntoIterator,
    T::Item: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut it = self.0.clone().into_iter();
        if let Some(e) = it.next() {
            write!(f, "{}", e)?;
            drop(e);
            for e in it {
                write!(f, ", {}", e)?;
            }
        }
        Ok(())
    }
}

/// Construct a mapping from names to values of type `T`. Reports an error if
/// duplicate names are detected.
fn make_name_map<T>(
    it: impl IntoIterator<Item = (T, sem::Ident)>,
    diag: &mut Diag,
) -> HashMap<String, T> {
    let mut multimap = HashMap::<String, Vec<_>>::new();

    for (val, ident) in it {
        multimap
            .entry(ident.sym)
            .or_default()
            .push((ident.span, val));
    }

    // Check duplicates
    for (ident_st, occurrences) in multimap.iter() {
        if occurrences.len() <= 1 {
            continue;
        }

        diag.emit(&[Diagnostic {
            level: Level::Error,
            message: format!("`{}` is defined for multiple times", ident_st),
            code: None,
            spans: occurrences
                .iter()
                .filter_map(|a| a.0)
                .map(|span| SpanLabel {
                    span,
                    label: None,
                    style: SpanStyle::Primary,
                })
                .into_iter()
                .collect(),
        }]);
    }

    // Convert to the desired hashmap type
    multimap
        .into_iter()
        .map(|(ident_st, mut occurrences)| (ident_st, occurrences.pop().unwrap().1))
        .collect()
}
