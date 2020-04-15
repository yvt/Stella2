use proc_macro2::TokenStream;

use super::ActionBinding;

pub(super) fn gen_action_binding<'a>(
    crate_path: &'a syn::Path,
    actions: impl IntoIterator<Item = &'a ActionBinding> + 'a,
) -> impl Iterator<Item = TokenStream> + 'a {
    actions
        .into_iter()
        .map(move |binding| {
            let action = &binding.action;
            binding.triggers.iter().map(move |trigger| {
                let source = syn::LitStr::new(&trigger.source.to_string(), trigger.source.span());
                let pattern = &trigger.pattern;
                quote::quote! {
                    #crate_path::testing::wmapi::ActionBinding {
                        source: #source,
                        pattern: #pattern,
                        action: #action,
                    }
                }
            })
        })
        .flatten()
}
