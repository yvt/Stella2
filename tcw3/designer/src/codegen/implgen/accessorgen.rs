//! Generates getters and setters.
use quote::ToTokens;
use std::fmt::Write;

use super::{
    fields, paths, sem, CompTy, Ctx, EventBoxHandlerTy, EventInnerSubList, GetterMethod,
    InnerValueField, SetterMethod, SubscribeMethod,
};

pub fn gen_accessors(ctx: &Ctx<'_>, out: &mut String) {
    let comp = ctx.cur_comp;

    writeln!(out, "impl {} {{", CompTy(&comp.ident.sym)).unwrap();

    for item in comp.items.iter() {
        match item {
            sem::CompItemDef::Field(field) => {
                use super::sem::{
                    FieldGetMode::{Borrow, Clone},
                    FieldType::{Const, Prop, Wire},
                };

                if let Some(get) = &field.accessors.get {
                    write!(
                        out,
                        "   {vis} fn {meth}(&self) -> ",
                        vis = get.vis,
                        meth = GetterMethod(&field.ident.sym),
                    )
                    .unwrap();

                    match (field.field_ty, get.mode) {
                        (Prop, Borrow) | (Wire, Borrow) => {
                            writeln!(
                                out,
                                "impl {deref}<Target = {ty}> + '_ {{",
                                deref = paths::DEREF,
                                ty = field.ty.as_ref().unwrap().to_token_stream(),
                            )
                            .unwrap();
                            writeln!(
                                out,
                                "        {or}::new(self.{shared}.{state}.borrow())",
                                or = paths::OWNING_REF,
                                shared = fields::SHARED,
                                state = fields::STATE,
                            )
                            .unwrap();
                            writeln!(
                                out,
                                "            .map(|state| &state.{field})",
                                field = InnerValueField(&field.ident.sym),
                            )
                            .unwrap();
                        }
                        (Prop, Clone) | (Wire, Clone) => {
                            writeln!(
                                out,
                                "{ty} {{",
                                ty = field.ty.as_ref().unwrap().to_token_stream(),
                            )
                            .unwrap();
                            writeln!(
                                out,
                                "        {clone}::clone(&self.{shared}.{state}.borrow().{field})",
                                clone = paths::CLONE,
                                shared = fields::SHARED,
                                state = fields::STATE,
                                field = InnerValueField(&field.ident.sym),
                            )
                            .unwrap();
                        }
                        (Const, Borrow) => {
                            writeln!(
                                out,
                                "&{ty} {{",
                                ty = field.ty.as_ref().unwrap().to_token_stream(),
                            )
                            .unwrap();
                            writeln!(
                                out,
                                "        &self.{shared}.{field}",
                                shared = fields::SHARED,
                                field = InnerValueField(&field.ident.sym),
                            )
                            .unwrap();
                        }
                        (Const, Clone) => {
                            writeln!(
                                out,
                                "{ty} {{",
                                ty = field.ty.as_ref().unwrap().to_token_stream(),
                            )
                            .unwrap();
                            writeln!(
                                out,
                                "        {clone}::Clone(&self.{shared}.{field})",
                                clone = paths::CLONE,
                                shared = fields::SHARED,
                                field = InnerValueField(&field.ident.sym),
                            )
                            .unwrap();
                        }
                    }

                    writeln!(out, "    }}").unwrap();
                } // let Some(get) = &field.accessors.get

                if let (Some(set), Prop) = (&field.accessors.set, field.field_ty) {
                    writeln!(
                        out,
                        "   {vis} fn {meth}(&self, new_value: {ty}) {{",
                        vis = set.vis,
                        meth = SetterMethod(&field.ident.sym),
                        ty = field.ty.as_ref().unwrap().to_token_stream(),
                    )
                    .unwrap();
                    writeln!(
                        out,
                        "        self.{shared}.{field}.set({some}(new_value));",
                        shared = fields::SHARED,
                        field = InnerValueField(&field.ident.sym),
                        some = paths::SOME,
                    )
                    .unwrap();
                    // TODO: Set a dirty flag
                    writeln!(out, "    }}").unwrap();
                } // let Some(set) = &field.accessors.set
            }
            sem::CompItemDef::Event(event) => {
                writeln!(
                    out,
                    "   {vis} fn {meth}(&self, handler: {ty}) -> {sub} {{",
                    vis = event.vis,
                    meth = SubscribeMethod(&event.ident.sym),
                    ty = EventBoxHandlerTy(&event.inputs),
                    sub = paths::SUB,
                )
                .unwrap();
                writeln!(
                    out,
                    "        self.{shared}.{field}",
                    shared = fields::SHARED,
                    field = EventInnerSubList(&event.ident.sym),
                )
                .unwrap();
                writeln!(out, "            .borrow_mut()").unwrap();
                writeln!(out, "            .insert(handler)").unwrap();
                writeln!(out, "            .untype()").unwrap();
                writeln!(out, "    }}").unwrap();
            }
            sem::CompItemDef::On(_) => {}
        }
    }

    writeln!(out, "}}").unwrap();
}
