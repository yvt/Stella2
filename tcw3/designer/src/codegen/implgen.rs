//! Implementation code generation
use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use quote::ToTokens;
use std::{collections::HashMap, fmt, fmt::Write};

use super::{diag::Diag, sem};
use crate::metadata;

/// Paths to standard library items.
mod paths {
    pub const BOX: &str = "::std::boxed::Box";
    pub const RC: &str = "::std::rc::Rc";
    pub const CELL: &str = "::std::cell::Cell";
    pub const REF_CELL: &str = "::std::cell::RefCell";
    pub const FN: &str = "::std::ops::Fn";
    pub const SUB_LIST: &str = "::subscriber_list::SubscriberList";
}

pub struct Ctx {
    /// The list of loaded crates. `[0]` is always the current crate.
    pub crates: Vec<metadata::Crate>,

    /// Mapping from external crate names to indices into `crates`.
    pub crate_map: HashMap<String, usize>,
}

pub fn gen_comp(comp: &sem::CompDef<'_>, _ctx: &Ctx, diag: &mut Diag) -> String {
    if comp.flags.contains(sem::CompFlags::PROTOTYPE_ONLY) {
        return r#"compile_error!(
            "`designer_impl!` can't generate code because the component is defined with #[prototype_only]"
        )"#.to_string();
    }

    let mut out = String::new();

    let comp_ident = &comp.path.segments.last().unwrap().ident;

    // String → index into `comp.items`
    // This also checks duplicate item names.
    let _item_name_map = make_name_map(
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

    // `struct ComponentType`
    // -------------------------------------------------------------------
    writeln!(out, "#![derive(Clone)]").unwrap();
    writeln!(out, "pub struct {} {{", comp_ident).unwrap();
    writeln!(
        out,
        "    inner: {rc}<{comp}Inner>,",
        rc = paths::RC,
        comp = comp_ident
    )
    .unwrap();
    writeln!(out, "}}").unwrap();

    // `struct ComponentTypeInner`
    // -------------------------------------------------------------------
    writeln!(out, "pub struct {}Inner {{", comp_ident).unwrap();

    for item in comp.items.iter() {
        match item {
            sem::CompItemDef::Field(item) => match item.field_ty {
                sem::FieldType::Const => {
                    writeln!(
                        out,
                        "    {ident}: {ty},",
                        ident = ConstInnerField(&item.ident.sym),
                        ty = item.ty.to_token_stream()
                    )
                    .unwrap();
                }
                sem::FieldType::Wire => {
                    // TODO
                }
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

    // TODO: builder and/or `new`
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

struct ConstInnerField<T>(T);
impl<T: fmt::Display> fmt::Display for ConstInnerField<T> {
    fn_fmt_write! { |this| ("value_{}", this.0) }
}

struct EventInnerSubList<T>(T);
impl<T: fmt::Display> fmt::Display for EventInnerSubList<T> {
    fn_fmt_write! { |this| ("subscriptions_{}", this.0) }
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
