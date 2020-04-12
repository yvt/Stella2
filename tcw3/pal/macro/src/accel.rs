use proc_macro_error::abort;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Result, Token,
};

#[cfg(feature = "gtk")]
mod gtk;
#[cfg(feature = "macos")]
mod macos;
mod testing;

struct MacroInput {
    crate_path: syn::Path,
    backend: syn::LitStr,
    bindings: Punctuated<ActionBinding, Token![,]>,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let crate_path = input.parse()?;
        input.parse::<Token![,]>()?;

        let backend = input.parse()?;
        input.parse::<Token![,]>()?;

        let content;
        syn::bracketed!(content in input);

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }

        Ok(Self {
            crate_path,
            backend,
            bindings: content.call(Punctuated::parse_terminated)?,
        })
    }
}

struct ActionBinding {
    action: syn::Expr,
    triggers: Punctuated<Trigger, Token![,]>,
}

impl Parse for ActionBinding {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        syn::parenthesized!(content in input);

        let action = content.parse()?;
        content.parse::<Token![,]>()?;
        let triggers = content.call(Punctuated::parse_terminated)?;

        Ok(Self { action, triggers })
    }
}

struct Trigger {
    source: syn::Ident,
    pattern: syn::LitStr,
}

impl Parse for Trigger {
    fn parse(input: ParseStream) -> Result<Self> {
        let source = input.parse()?;

        let content;
        syn::parenthesized!(content in input);

        let pattern = content.parse()?;

        Ok(Self { source, pattern })
    }
}

pub fn accel_table_inner(params: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: MacroInput = syn::parse_macro_input!(params);

    let backend = input.backend.value();

    match &*backend {
        "testing" => {
            // Only in this case this macro produces an expression of type
            // `&'static [_]`.
            // In other cases, it's `tcw3_pal::native::AccelTable`.

            // A sequence of expressions of type `ActionBinding`
            let bindings = testing::gen_action_binding(&input.crate_path, input.bindings.iter());

            (quote::quote! { &[#(#bindings),*] }).into()
        }

        #[cfg(feature = "macos")]
        "macos" => macos::gen_accel_table(&input).into(),

        #[cfg(feature = "gtk")]
        "gtk" => gtk::gen_accel_table(&input).into(),

        // TODO:
        #[cfg(feature = "windows")]
        "windows" => quote::quote! { () }.into(),

        unknown_backend => abort!(
            input.backend.span(),
            "unknown backend: {:?}",
            unknown_backend
        ),
    }
}
