use either::{Left, Right};
use quote::ToTokens;
use std::{fmt::Write, iter::repeat};

use super::super::{diag::Diag, sem};
use super::{
    iterutils::Iterutils, paths, Angle, CommaSeparated, FactoryGenParamNameForField,
    FactorySetterForField, InnerValueField,
};
use crate::metadata;

pub fn gen_builder(
    comp: &sem::CompDef<'_>,
    meta_comp: &metadata::CompDef,
    comp_ident: &proc_macro2::Ident,
    _diag: &mut Diag,
    out: &mut String,
) {
    let builder_vis = meta_comp.builder_vis();

    let settable_consts = comp.items.iter().filter_map(|item| match item {
        sem::CompItemDef::Field(
            field @ sem::FieldDef {
                field_ty: sem::FieldType::Const,
                accessors: sem::FieldAccessors { set: Some(_), .. },
                ..
            },
        ) => Some(field),
        _ => None,
    });
    let optional_consts = settable_consts
        .clone()
        .filter(|field| field.value.is_some());
    let non_optional_consts = settable_consts
        .clone()
        .filter(|field| field.value.is_none());
    let num_non_optional_consts = non_optional_consts.clone().count();

    // `T_field1`, `T_field2`, ...
    let builder_ty_params = non_optional_consts
        .clone()
        .map(|field| FactoryGenParamNameForField(&field.ident.sym));

    // `u32`, `HView`, ...
    let builder_complete_ty_params = non_optional_consts
        .clone()
        .map(|field| field.ty.to_token_stream());

    writeln!(
        out,
        "{vis} struct {comp}Builder{gen} {{",
        vis = builder_vis,
        comp = comp_ident,
        gen = if num_non_optional_consts != 0 {
            Left(Angle(CommaSeparated(builder_ty_params.clone())))
        } else {
            Right("")
        }
    )
    .unwrap();
    for field in settable_consts.clone() {
        writeln!(
            out,
            "    {ident}: {ty},",
            ident = InnerValueField(&field.ident.sym),
            ty = if field.value.is_some() {
                // optional
                Left(format!("{}<{}>", paths::OPTION, field.ty.to_token_stream()))
            } else {
                // non-optional - use a generics-based trick to enforce
                //                initialization
                Right(FactoryGenParamNameForField(&field.ident.sym))
            },
        )
        .unwrap();
    }
    writeln!(out, "}}").unwrap();

    // `ComponentBuilder::<Unset, ...>::new`
    // -------------------------------------------------------------------
    writeln!(
        out,
        "impl {comp}Builder{gen} {{",
        comp = comp_ident,
        gen = if num_non_optional_consts != 0 {
            Left(Angle(CommaSeparated(
                repeat(paths::UNSET).take(num_non_optional_consts),
            )))
        } else {
            Right("")
        }
    )
    .unwrap();
    writeln!(out, "    {vis} fn new() -> Self {{", vis = builder_vis).unwrap();
    writeln!(out, "        Self {{").unwrap();
    for field in settable_consts.clone() {
        writeln!(
            out,
            "            {ident}: {ty},",
            ident = InnerValueField(&field.ident.sym),
            ty = if field.value.is_some() {
                "None"
            } else {
                paths::UNSET
            },
        )
        .unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();

    // `ComponentBuilder::{with_*}`
    // -------------------------------------------------------------------
    writeln!(
        out,
        "impl{gen} {comp}Builder{gen} {{",
        comp = comp_ident,
        gen = if non_optional_consts.clone().next().is_some() {
            Left(Angle(CommaSeparated(builder_ty_params.clone())))
        } else {
            Right("")
        }
    )
    .unwrap();

    for field in optional_consts.clone() {
        // They just assign a new value to `Option<T>`
        writeln!(
            out,
            "    {vis} fn {method}(self, {ident}: {ty}) -> Self {{",
            vis = field.vis.to_token_stream(),
            method = FactorySetterForField(&field.ident.sym),
            ident = field.ident.sym,
            ty = field.ty.to_token_stream(),
        )
        .unwrap();
        writeln!(
            out,
            "        Self {{ {ident}: {some}({ident}), ..self }}",
            some = paths::SOME,
            ident = field.ident.sym,
        )
        .unwrap();
        writeln!(out, "    }}",).unwrap();
    }

    for (i, field) in non_optional_consts.clone().enumerate() {
        // They each change one type parameter of `ComponentBuilder`
        let new_builder_ty = format!(
            "{comp}Builder<{gen}>",
            comp = comp_ident,
            gen = CommaSeparated(
                builder_ty_params
                    .clone()
                    .map(Left)
                    .replace_at(i, Right(field.ty.to_token_stream()))
            )
        );
        writeln!(
            out,
            "    {vis} fn {method}(self, {ident}: {ty}) -> {new_bldr_ty} {{",
            vis = field.vis.to_token_stream(),
            method = FactorySetterForField(&field.ident.sym),
            ident = field.ident.sym,
            ty = field.ty.to_token_stream(),
            new_bldr_ty = new_builder_ty,
        )
        .unwrap();
        writeln!(
            out,
            "        {comp}Builder {{ {fields} }}",
            comp = comp_ident,
            fields = CommaSeparated(settable_consts.clone().map(|field2| {
                if field2.ident.sym == field.ident.sym {
                    // Replace with the new value
                    format!(
                        "{}: {}",
                        InnerValueField(&field2.ident.sym),
                        field2.ident.sym
                    )
                } else {
                    // Use the old value
                    format!(
                        "{}: self.{}",
                        InnerValueField(&field2.ident.sym),
                        field2.ident.sym
                    )
                }
            }))
        )
        .unwrap();
        writeln!(out, "    }}",).unwrap();
    }
    writeln!(out, "}}").unwrap();

    // `ComponentBuilder::<u32, ...>::build`
    // -------------------------------------------------------------------
    writeln!(
        out,
        "impl {comp}Builder{gen} {{",
        comp = comp_ident,
        gen = if num_non_optional_consts != 0 {
            Left(Angle(CommaSeparated(builder_complete_ty_params)))
        } else {
            Right("")
        }
    )
    .unwrap();
    writeln!(out, "    fn build(self) -> {} {{", comp_ident).unwrap();
    // TODO
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
}
