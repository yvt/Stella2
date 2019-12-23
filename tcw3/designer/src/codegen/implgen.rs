//! Implementation code generation
use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use quote::ToTokens;
use std::{collections::HashMap, fmt, fmt::Write};

use super::{diag::Diag, sem, EmittedError};
use crate::metadata;

#[macro_use]
mod docgen;

mod accessorgen;
mod analysis;
mod bitsetgen;
mod buildergen;
mod dropgen;
mod evalgen;
mod initgen;
pub mod iterutils;

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
    pub const DEREF: &str = "::std::ops::Deref";
    pub const TRAIT_DROP: &str = "::std::ops::Drop";
    pub const FN_DROP: &str = "::std::mem::drop";
    pub const FORGET: &str = "::std::mem::forget";
    pub const DEBUG_ASSERT: &str = "::std::debug_assert";
    pub const MAYBE_UNINIT: &str = "::std::mem::MaybeUninit";
}

/// The fields of generated types.
mod fields {
    pub const SHARED: &str = "shared";
    pub const STATE: &str = "state";
    pub const DIRTY: &str = "dirty";
    /// `subs: [MaybeUninit<Sub>; num_subs()]`, used in a manner similar to
    /// `ManuallyDrop`
    pub const SUBS: &str = "subs";
}

mod methods {
    pub const SET_DIRTY_FLAGS: &str = "set_dirty_flags";
    pub const COMMIT: &str = "__commit";
}

/// Recognized field (not Rust field, but our field) names.
mod known_fields {
    /// The dirty flag system needs an access to `Wm` to use `invoke_on_update`.
    pub const WM: &str = "wm";
}

pub struct Ctx<'a> {
    /// Contains all loaded crates.
    pub repo: &'a metadata::Repo,

    /// Mapping from imported crate names to indices into `repo.crates`.
    pub imports_crate_i: &'a HashMap<&'a str, usize>,

    pub cur_comp: &'a sem::CompDef<'a>,

    pub cur_meta_comp_i: usize,

    pub tcw3_path: &'a str,
    pub designer_runtime_path: &'a str,
}

impl<'a> Ctx<'a> {
    fn cur_meta_comp_ref(&self) -> metadata::CompRef {
        metadata::CompRef {
            crate_i: self.repo.main_crate_i,
            comp_i: self.cur_meta_comp_i,
        }
    }

    fn cur_meta_comp(&self) -> &'a metadata::CompDef {
        self.repo.comp_by_ref(&self.cur_meta_comp_ref())
    }

    // `::tcw3::designer_runtime::SubscriberList`
    fn path_sub_list(&self) -> impl std::fmt::Display + Clone + '_ {
        DisplayFn(move |f| write!(f, "{}::SubscriberList", self.designer_runtime_path))
    }

    // `::tcw3::designer_runtime::Sub`
    fn path_sub(&self) -> impl std::fmt::Display + Clone + '_ {
        DisplayFn(move |f| write!(f, "{}::Sub", self.designer_runtime_path))
    }

    // `::tcw3::designer_runtime::OwningRef`
    fn path_owning_ref(&self) -> impl std::fmt::Display + Clone + '_ {
        DisplayFn(move |f| write!(f, "{}::OwningRef", self.designer_runtime_path))
    }

    // `::tcw3::designer_runtime::Unset`
    fn path_unset(&self) -> impl std::fmt::Display + Clone + '_ {
        DisplayFn(move |f| write!(f, "{}::Unset", self.designer_runtime_path))
    }

    // `::tcw3::uicore::WmExt::invoke_on_update`
    fn path_invoke_on_update(&self) -> impl std::fmt::Display + Clone + '_ {
        DisplayFn(move |f| write!(f, "{}::uicore::WmExt::invoke_on_update", self.tcw3_path))
    }

    // `::tcw3::designer_runtime::unwrap_unchecked`
    fn path_unwrap_unchecked(&self) -> impl std::fmt::Display + Clone + '_ {
        DisplayFn(move |f| write!(f, "{}::unwrap_unchecked", self.designer_runtime_path))
    }

    // `::tcw3::designer_runtime::ShallowEq`
    fn path_shallow_eq(&self) -> impl std::fmt::Display + Clone + '_ {
        DisplayFn(move |f| write!(f, "{}::ShallowEq", self.designer_runtime_path))
    }

    // `::tcw3::designer_runtime::unsubscribe_subs_unchecked`
    fn path_unsubscribe_subs_unchecked(&self) -> impl std::fmt::Display + Clone + '_ {
        DisplayFn(move |f| {
            write!(
                f,
                "{}::unsubscribe_subs_unchecked",
                self.designer_runtime_path
            )
        })
    }
}

pub fn gen_comp(ctx: &Ctx, diag: &mut Diag<'_>) -> Result<String, EmittedError> {
    let comp = ctx.cur_comp;

    if comp.flags.contains(sem::CompFlags::PROTOTYPE_ONLY) {
        return Ok(r#"compile_error!(
            "`designer_impl!` can't generate code because the component is defined with #[prototype_only]"
        )"#.to_string());
    }

    let mut out = String::new();

    let comp_ident = &comp.ident.sym;

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
        return Err(EmittedError);
    }

    // index into `meta_comp.items` → index into `comp.items`
    let item_meta2sem_map: Vec<_> = ctx
        .cur_meta_comp()
        .items
        .iter()
        .map(|i| *item_name_map.get(i.ident()).unwrap())
        .collect();

    // Analyze input references
    let analysis = analysis::Analysis::new(ctx, &item_meta2sem_map, diag);

    // Analyze field dependency
    let dep_analysis =
        initgen::DepAnalysis::new(&analysis, ctx, &item_meta2sem_map, &item_name_map, diag)?;

    use docgen::{CodegenInfoDoc, MdCode};

    // `struct ComponentType`
    // -------------------------------------------------------------------
    writeln!(out, "#[derive(Clone)]").unwrap();

    docgen::gen_doc_attrs(&comp.doc_attrs, "", &mut out);
    writeln!(out, "{}", doc_attr!("")).unwrap();
    writeln!(out, "{}", doc_attr!("")).unwrap();
    writeln!(
        out,
        "{}",
        CodegenInfoDoc(comp.path.span.map(|s| s.low()), diag)
    )
    .unwrap();

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

    writeln!(out, "impl {} {{", CompTy(comp_ident)).unwrap();
    // `ComponentType::__commit`
    initgen::gen_commit(&analysis, &dep_analysis, ctx, &item_meta2sem_map, &mut out);
    writeln!(out, "}}").unwrap();

    // `struct ComponentTypeShared`
    // -------------------------------------------------------------------
    writeln!(
        out,
        "{}",
        doc_attr!(
            "The immutable portion of {} component's instance-specific data.",
            MdCode(comp_ident)
        )
    )
    .unwrap();
    writeln!(out, "{}", doc_attr!("")).unwrap();
    writeln!(out, "{}", CodegenInfoDoc(None, diag)).unwrap();

    writeln!(out, "struct {} {{", CompSharedTy(comp_ident)).unwrap();
    writeln!(out, "    #[allow(dead_code)]").unwrap();
    writeln!(
        out,
        "    {field}: {cell}<{ty}>,",
        field = fields::STATE,
        cell = paths::REF_CELL,
        ty = CompStateTy(comp_ident)
    )
    .unwrap();
    writeln!(out, "    #[allow(dead_code)]").unwrap();
    writeln!(
        out,
        "    {field}: {cell}<{ty}>,",
        field = fields::DIRTY,
        cell = paths::CELL,
        ty = dep_analysis.cdf_ty.gen_ty(), // "compressed dirty flags"
    )
    .unwrap();
    if dep_analysis.num_subs() > 0 {
        writeln!(
            out,
            "    {field}: [{cell}<{mu}<{sub}>>; {len}],",
            field = fields::SUBS,
            cell = paths::CELL,
            mu = paths::MAYBE_UNINIT,
            sub = ctx.path_sub(),
            len = dep_analysis.num_subs(),
        )
        .unwrap();
    }

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
                    // Uncommited new value
                    writeln!(
                        out,
                        "    {ident}: {cell}<{opt}<{ty}>>,",
                        ident = InnerValueField(&item.ident.sym),
                        cell = paths::CELL,
                        opt = paths::OPTION,
                        ty = item.ty.to_token_stream()
                    )
                    .unwrap();
                }
            },
            sem::CompItemDef::Event(item) => {
                writeln!(
                    out,
                    "    {ident}: {cell}<{list}<{handler}>>,",
                    ident = EventInnerSubList(&item.ident.sym),
                    cell = paths::REF_CELL,
                    list = ctx.path_sub_list(),
                    handler = EventBoxHandlerTy(&item.inputs)
                )
                .unwrap();
            }
            sem::CompItemDef::On(_) => {}
        }
    }

    writeln!(out, "}}").unwrap();

    writeln!(out, "impl {} {{", CompSharedTy(comp_ident)).unwrap();
    // `ComponentTypeShared::set_dirty_flags`
    initgen::gen_set_dirty_flags(&dep_analysis, ctx, &mut out);
    writeln!(out, "}}").unwrap();

    // `<ComponentTypeShared as Drop>::drop`
    dropgen::gen_shared_drop(ctx, &dep_analysis, &mut out);

    // `struct ComponentTypeState`
    // -------------------------------------------------------------------
    writeln!(
        out,
        "{}",
        doc_attr!(
            "The mutable portion of {} component's instance-specific data.",
            MdCode(comp_ident)
        )
    )
    .unwrap();
    writeln!(out, "{}", doc_attr!("")).unwrap();
    writeln!(out, "{}", CodegenInfoDoc(None, diag)).unwrap();

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
        &analysis,
        &dep_analysis,
        ctx,
        &item_meta2sem_map,
        diag,
        &mut out,
    );

    // Setters and getters
    // -------------------------------------------------------------------
    accessorgen::gen_accessors(&dep_analysis, ctx, &mut out);

    Ok(out)
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

struct GetterMethod<T>(T);
impl<T: fmt::Display> fmt::Display for GetterMethod<T> {
    fn_fmt_write! { |this| ("{}", this.0) }
}

struct SetterMethod<T>(T);
impl<T: fmt::Display> fmt::Display for SetterMethod<T> {
    fn_fmt_write! { |this| ("set_{}", this.0) }
}

struct SubscribeMethod<T>(T);
impl<T: fmt::Display> fmt::Display for SubscribeMethod<T> {
    fn_fmt_write! { |this| ("subscribe_{}", this.0) }
}

struct RaiseMethod<T>(T);
impl<T: fmt::Display> fmt::Display for RaiseMethod<T> {
    fn_fmt_write! { |this| ("raise_{}", this.0) }
}

struct EventHandlerTrait<'a>(&'a [syn::FnArg]);
impl fmt::Display for EventHandlerTrait<'_> {
    fn_fmt_write! { |this| (
        "{fn}({params})",
        fn = paths::FN,
        params = CommaSeparated(this.0.iter()
            .map(|arg| match arg {
                syn::FnArg::Receiver(_) => unreachable!(),
                syn::FnArg::Typed(pat) => pat.ty.to_token_stream(),
            }))
    ) }
}

struct EventDynHandlerTy<'a>(&'a [syn::FnArg]);
impl fmt::Display for EventDynHandlerTy<'_> {
    fn_fmt_write! { |this| (
        "dyn {trait}",
        trait = EventHandlerTrait(this.0)
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

#[derive(Clone, Copy)]
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

struct CommaSeparatedWithTrailingComma<T>(T);
impl<T> fmt::Display for CommaSeparatedWithTrailingComma<T>
where
    T: Clone + IntoIterator,
    T::Item: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for e in self.0.clone() {
            write!(f, "{}, ", e)?;
        }
        Ok(())
    }
}

struct Concat<T>(T);
impl<T> fmt::Display for Concat<T>
where
    T: Clone + IntoIterator,
    T::Item: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for e in self.0.clone() {
            write!(f, "{}", e)?;
        }
        Ok(())
    }
}

/// Usage: `DisplayFn(move |f| { write!(f, "{} is best pony!", x) })`
#[derive(Clone, Copy)]
struct DisplayFn<T: Fn(&mut fmt::Formatter<'_>) -> fmt::Result>(T);
impl<T: Fn(&mut fmt::Formatter<'_>) -> fmt::Result> fmt::Display for DisplayFn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.0)(f)
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
