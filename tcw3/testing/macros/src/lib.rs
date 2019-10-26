extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_error::{abort, abort_call_site, proc_macro_error};
use std::mem::replace;
use syn::{
    parse, parse_macro_input, parse_str, spanned::Spanned, AttributeArgs, FnArg, Ident, Item, Lit,
    Meta, NestedMeta, Pat, PatIdent, PatType, Path, ReturnType,
};

#[proc_macro_attribute]
#[proc_macro_error]
pub fn use_testing_wm(args: TokenStream, input: TokenStream) -> TokenStream {
    let args: AttributeArgs = parse_macro_input!(args);

    let mut testing_path = "tcw3::testing".to_owned();
    let mut testing_path_span = None;
    for arg in args {
        match arg {
            NestedMeta::Meta(Meta::NameValue(nv)) => {
                if nv.path.is_ident("testing") {
                    if let Lit::Str(s) = nv.lit {
                        testing_path = s.value();
                        testing_path_span = Some(s.span());
                    } else {
                        abort!(nv.lit.span(), "expected a string literal");
                    };
                } else {
                }
            }
            _ => {
                abort!(arg.span(), "unrecognized parameter");
            }
        }
    }

    let testing_path: Path = match parse_str(&testing_path) {
        Ok(x) => x,
        Err(err) => abort!(testing_path_span.unwrap(), "{}", err),
    };

    let output = match parse(input.clone()) {
        Ok(Item::Fn(mut item_fn)) => {
            let inputs = &item_fn.sig.inputs;

            match inputs.first() {
                Some(FnArg::Typed(_)) => {
                    // We can't check if the parameter has the correct type
                    // due to lack of access to semantic information
                }
                Some(arg @ FnArg::Receiver(_)) => {
                    abort!(arg.span(), "methods are not supported (yet)")
                }
                None => abort_call_site!("must have an argument to receive `&dyn TestingWm`"),
            }

            if let ReturnType::Type(_, _) = item_fn.sig.output {
                abort!(item_fn.sig.output.span(), "must not have a return type");
            }

            // A wrapper function with the same name is created. `item_fn` is
            // defined inside the function, which is passed to `use_testing_wm`.
            // `outer_args` is the wrapper function's argument list. `call_args`
            // is the parameter list passed to the original function.
            let (outer_args, call_args): (Vec<_>, Vec<_>) = inputs
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, arg)| match arg {
                    FnArg::Typed(arg) => {
                        // `arg` = `(hoge, piyo): Ty`
                        let ident = ident_for_pat(&arg.pat).cloned().unwrap_or_else(|| {
                            Ident::new(&format!("__arg{}", i), Span::call_site())
                        });

                        // `__arg1: Ty`
                        let outer_arg = FnArg::Typed(PatType {
                            attrs: Vec::new(),
                            pat: Box::new(
                                PatIdent {
                                    attrs: Vec::new(),
                                    by_ref: None,
                                    mutability: None,
                                    ident: ident.clone(),
                                    subpat: None,
                                }
                                .into(),
                            ),
                            colon_token: arg.colon_token,
                            ty: arg.ty.clone(),
                        });

                        // `__arg1`
                        let call_arg = ident;

                        (outer_arg, call_arg)
                    }
                    FnArg::Receiver(_) => unreachable!(),
                })
                .unzip();

            let vis = item_fn.vis.clone();
            let attrs = replace(&mut item_fn.attrs, Vec::new());
            let ident = &item_fn.sig.ident;

            quote::quote! {
                #(#attrs)*
                #vis fn #ident(#(#outer_args),*) {
                    #testing_path::try_init_logger();
                    #item_fn
                    #testing_path::pal_testing::run_test(move |__testing_wm| {
                        #ident(__testing_wm, #(#call_args),*)
                    })
                }
            }
        }
        _ => {
            abort_call_site!("#[use_testing_wm] is only supported on functions");
        }
    };

    output.into()
}

/// Try to extract an unique identifier from a pattern.
fn ident_for_pat(pat: &Pat) -> Option<&Ident> {
    match pat {
        Pat::Box(pat_box) => ident_for_pat(&pat_box.pat),
        Pat::Ident(pat_ident) => Some(&pat_ident.ident),
        Pat::Reference(pat_ref) => ident_for_pat(&pat_ref.pat),
        Pat::Type(pat_ty) => ident_for_pat(&pat_ty.pat),
        Pat::Tuple(pat_tuple) => {
            if pat_tuple.elems.len() == 1 {
                ident_for_pat(&pat_tuple.elems[0])
            } else {
                None
            }
        }
        _ => None,
    }
}
