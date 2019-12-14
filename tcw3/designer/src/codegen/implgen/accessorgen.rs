//! Generates getters and setters.
use quote::ToTokens;
use std::fmt::Write;

use super::{
    fields, initgen, paths, sem, CommaSeparated, CompTy, Ctx, EventBoxHandlerTy, EventInnerSubList,
    GetterMethod, InnerValueField, RaiseMethod, SetterMethod, SubscribeMethod, TempVar,
};

pub fn gen_accessors(dep_analysis: &initgen::DepAnalysis, ctx: &Ctx<'_>, out: &mut String) {
    let comp = ctx.cur_comp;

    writeln!(out, "impl {} {{", CompTy(&comp.ident.sym)).unwrap();

    for (item_i, item) in comp.items.iter().enumerate() {
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

                    // Set relevant dirty flags
                    let trigger = initgen::CommitTrigger::SetItem { item_i };
                    initgen::gen_activate_trigger(
                        dep_analysis,
                        ctx,
                        &trigger,
                        &format_args!("&self.{}", fields::SHARED),
                        out,
                    );

                    writeln!(out, "    }}").unwrap();
                } // let Some(set) = &field.accessors.set
            }
            sem::CompItemDef::Event(event) => {
                writeln!(
                    out,
                    "    {vis} fn {meth}(&self, handler: {ty}) -> {sub} {{",
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

                let handler = TempVar("handler");
                writeln!(
                    out,
                    "    fn {meth}(&self, {args}) {{",
                    meth = RaiseMethod(&event.ident.sym),
                    args =
                        CommaSeparated(event.inputs.iter().map(|fn_arg| fn_arg.to_token_stream())),
                )
                .unwrap();
                writeln!(
                    out,
                    "        for {i} in self.{shared}.{field}.borrow().iter() {{",
                    i = handler,
                    shared = fields::SHARED,
                    field = EventInnerSubList(&event.ident.sym),
                )
                .unwrap();
                writeln!(
                    out,
                    "            {i}({args});",
                    i = handler,
                    args = CommaSeparated(event.inputs.iter().map(|arg| match arg {
                        syn::FnArg::Receiver(_) => unreachable!(),
                        syn::FnArg::Typed(pat) => format!(
                            "{clone}::clone(&{val})",
                            clone = paths::CLONE,
                            val = pat.pat.to_token_stream()
                        ),
                    })),
                )
                .unwrap();
                writeln!(out, "        }}").unwrap();
                writeln!(out, "    }}").unwrap();
            }
            sem::CompItemDef::On(_) => {}
        }
    }

    writeln!(out, "}}").unwrap();
}
