//! Generates function evaluation code.
use quote::ToTokens;
use std::fmt::Write;

use super::{analysis, paths, sem, CommaSeparatedWithTrailingComma, Ctx};
use crate::metadata;

pub trait FuncInputGen {
    /// Generate an expression that evaluates to the specified field's value
    /// or reference.
    ///
    /// `item_i` is an index into the current component's `sem::CompDef::items`.
    fn gen_field_ref(&mut self, item_i: usize, by_ref: bool, out: &mut String);

    /// Generate an expression that evaluates to `&ComponentType`.
    fn gen_this(&mut self, out: &mut String);

    /// Get the trigger position (in an `on` item) for which the handler is
    /// instantiated.
    fn trigger_i(&mut self) -> usize;

    /// Generate an expression that evaluates to an event parameter.
    fn gen_event_param(&mut self, param_i: usize, out: &mut String);
}

/// Generates an expression that evaluates the given `Func`.
pub fn gen_func_eval(
    func: &sem::Func,
    analysis: &analysis::Analysis,
    ctx: &Ctx,
    item_meta2sem_map: &[usize],
    input_gen: &mut dyn FuncInputGen,
    out: &mut String,
) {
    // TODO: Stop duplicating the body on every instance of the evaluation.
    //       This takes some effort to do because (top-level) functions require
    //       explicit types in their parameters, and our metadata do not have
    //       types for all fields.
    //
    //       Current implementation:
    //          on (event1, event2) || { body }
    //           ↓
    //          subscribe_event1(Box::new(|| {
    //              match () { () => { body } )
    //          }));
    //          subscribe_event2(Box::new(|| {
    //              match () { () => { body } )
    //          }));

    // `match` input
    write!(out, "match (").unwrap();
    for func_input in func.inputs.iter() {
        if !analysis.get_input(&func_input.input).has_value(ctx.repo) {
            continue;
        }

        match analysis.get_input(&func_input.input) {
            analysis::InputInfo::EventParam(param_input) => {
                if func_input.by_ref {
                    out.push_str("&");
                }
                let trigger_i = input_gen.trigger_i();
                let param_i = param_input.param_i[trigger_i];
                input_gen.gen_event_param(param_i, out);
            }
            analysis::InputInfo::Item(item_input) => {
                let ind0 = item_input.indirections.first().unwrap();
                if item_input.indirections.len() == 1 {
                    // Dereference the current component's field, and that's it
                    input_gen.gen_field_ref(item_meta2sem_map[ind0.item_i], func_input.by_ref, out);
                } else {
                    let ind_last = item_input.indirections.last().unwrap();
                    // | getter mode | by_ref | output               |
                    // | ----------- | ------ | -------------------- |
                    // | borrow      | false  | Clone::clone(&*expr) |
                    // | borrow      | true   | &*expr               |
                    // | clone       | false  | expr                 |
                    // | clone       | true   | &expr                |
                    let needs_closing_parenthesis = {
                        let field = ind_last.item(ctx.repo).field().unwrap();
                        let getter = field.accessors.get.as_ref().unwrap();
                        match (getter.mode, func_input.by_ref) {
                            (metadata::FieldGetMode::Borrow, false) => {
                                write!(out, "{}::clone(&*", paths::CLONE).unwrap();
                                true
                            }
                            (metadata::FieldGetMode::Borrow, true) => {
                                write!(out, "&*").unwrap();
                                false
                            }
                            (metadata::FieldGetMode::Clone, false) => false,
                            (metadata::FieldGetMode::Clone, true) => {
                                write!(out, "&").unwrap();
                                false
                            }
                        }
                    };

                    // Get the current component's field by reference
                    input_gen.gen_field_ref(item_meta2sem_map[ind0.item_i], true, out);

                    for ind in item_input.indirections[1..].iter() {
                        let item = ind.item(ctx.repo).field().unwrap();
                        write!(out, ".{}()", item.ident).unwrap();
                    }

                    if needs_closing_parenthesis {
                        write!(out, ")").unwrap();
                    }
                }
            }
            analysis::InputInfo::This => {
                if func_input.by_ref {
                    input_gen.gen_this(out);
                } else {
                    write!(out, "{}::Clone(", paths::CLONE).unwrap();
                    input_gen.gen_this(out);
                    write!(out, ")").unwrap();
                }
            }
            analysis::InputInfo::Invalid => {}
        }
        write!(out, ", ").unwrap();
    }
    write!(out, ")").unwrap();

    // `match` pattern and body
    write!(
        out,
        " {{ ({args}) => {{ {body} }} }}",
        args = CommaSeparatedWithTrailingComma(func.inputs.iter().filter_map(|func_input| {
            if analysis.get_input(&func_input.input).has_value(ctx.repo) {
                Some(&func_input.ident.sym)
            } else {
                None
            }
        })),
        body = func.body.to_token_stream(),
    )
    .unwrap();
}
