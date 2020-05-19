use proc_macro_error::abort;
use std::convert::TryInto;

use super::{MacroInput, Trigger};
use crate::keycode::{Key, KeyPattern, ModFlags};

pub(super) fn gen_accel_table(input: &MacroInput) -> proc_macro2::TokenStream {
    let crate_path = &input.crate_path;

    let key_bindings = input
        .bindings
        .iter()
        .map(move |binding| {
            let action = &binding.action;
            binding.triggers.iter().filter_map(move |trigger| {
                if trigger.source == "windows" {
                    Some(gen_key_binding(crate_path, action, trigger))
                } else {
                    None
                }
            })
        })
        .flatten();

    quote::quote! {
        #crate_path::windows::AccelTable {
            key: &[#(#key_bindings),*],
        }
    }
}

fn gen_key_binding<'a>(
    crate_path: &'a syn::Path,
    action: &'a syn::Expr,
    trigger: &'a Trigger,
) -> proc_macro2::TokenStream {
    let pat = match trigger.pattern.value().parse::<KeyPattern>() {
        Ok(pat) => pat,
        Err(e) => abort!(trigger.pattern.span(), "{}", e),
    };

    // Pre-defined virtual key code
    let lookup_vk = |name: &str| {
        let name = syn::Ident::new(name, trigger.pattern.span());
        quote::quote! { #crate_path::windows::winuser::#name as u16 }
    };

    let vk = match pat.key {
        // `WinUser.h`
        Key::Char(c @ 'a'..='z') | Key::Char(c @ '0'..='9') | Key::Char(c @ ' ') => {
            let val = c.to_ascii_uppercase() as u32;
            let val: u16 = val.try_into().unwrap();
            quote::quote! { #val }
        }
        Key::Escape => lookup_vk("VK_ESCAPE"),
        Key::Backspace => lookup_vk("VK_BACK"),
        Key::Return => lookup_vk("VK_RETURN"),
        Key::Tab => lookup_vk("VK_TAB"),
        Key::Delete => lookup_vk("VK_DELETE"),
        Key::Left => lookup_vk("VK_LEFT"),
        Key::Up => lookup_vk("VK_UP"),
        Key::Right => lookup_vk("VK_RIGHT"),
        Key::Down => lookup_vk("VK_DOWN"),
        Key::PageUp => lookup_vk("VK_PRIOR"),
        Key::PageDown => lookup_vk("VK_NEXT"),
        Key::End => lookup_vk("VK_END"),
        Key::Home => lookup_vk("VK_HOME"),
        Key::Insert => lookup_vk("VK_INSERT"),
        Key::Numpad0 => lookup_vk("VK_NUMPAD0"),
        Key::Numpad1 => lookup_vk("VK_NUMPAD1"),
        Key::Numpad2 => lookup_vk("VK_NUMPAD2"),
        Key::Numpad3 => lookup_vk("VK_NUMPAD3"),
        Key::Numpad4 => lookup_vk("VK_NUMPAD4"),
        Key::Numpad5 => lookup_vk("VK_NUMPAD5"),
        Key::Numpad6 => lookup_vk("VK_NUMPAD6"),
        Key::Numpad7 => lookup_vk("VK_NUMPAD7"),
        Key::Numpad8 => lookup_vk("VK_NUMPAD8"),
        Key::Numpad9 => lookup_vk("VK_NUMPAD9"),
        Key::NumpadMultiply => lookup_vk("VK_MULTIPLY"),
        Key::NumpadAdd => lookup_vk("VK_ADD"),
        Key::NumpadSeparator => lookup_vk("VK_SEPARATOR"),
        Key::NumpadSubtract => lookup_vk("VK_SUBTRACT"),
        Key::NumpadDecimal => lookup_vk("VK_DECIMAL"),
        Key::NumpadDivide => lookup_vk("VK_DIVIDE"),
        Key::F1 => lookup_vk("VK_F1"),
        Key::F2 => lookup_vk("VK_F2"),
        Key::F3 => lookup_vk("VK_F3"),
        Key::F4 => lookup_vk("VK_F4"),
        Key::F5 => lookup_vk("VK_F5"),
        Key::F6 => lookup_vk("VK_F6"),
        Key::F7 => lookup_vk("VK_F7"),
        Key::F8 => lookup_vk("VK_F8"),
        Key::F9 => lookup_vk("VK_F9"),
        Key::F10 => lookup_vk("VK_F10"),
        Key::F11 => lookup_vk("VK_F11"),
        Key::F12 => lookup_vk("VK_F12"),
        Key::F13 => lookup_vk("VK_F13"),
        Key::F14 => lookup_vk("VK_F14"),
        Key::F15 => lookup_vk("VK_F15"),
        Key::F16 => lookup_vk("VK_F16"),
        Key::F17 => lookup_vk("VK_F17"),
        Key::F18 => lookup_vk("VK_F18"),
        Key::F19 => lookup_vk("VK_F19"),
        Key::F20 => lookup_vk("VK_F20"),
        Key::F21 => lookup_vk("VK_F21"),
        Key::F22 => lookup_vk("VK_F22"),
        Key::F23 => lookup_vk("VK_F23"),
        Key::F24 => lookup_vk("VK_F24"),

        unknown => {
            abort!(trigger.pattern.span(), "Unsupported key: `{:?}`", unknown);
        }
    };

    // `AccelTable::MOD_*`
    let mut mod_flags = Vec::new();
    if pat.mod_flags.contains(ModFlags::SHIFT) {
        mod_flags.push("MOD_SHIFT");
    }
    if pat.mod_flags.contains(ModFlags::CONTROL) {
        mod_flags.push("MOD_CONTROL");
    }
    if pat.mod_flags.contains(ModFlags::ALT) {
        mod_flags.push("MOD_MENU");
    }
    if pat.mod_flags.contains(ModFlags::SUPER) {
        abort!(
            trigger.pattern.span(),
            "The `Super` modifier is not supported on Windows"
        );
    }

    let mod_flags = if mod_flags.is_empty() {
        quote::quote! { 0 }
    } else {
        let mod_flags = mod_flags.into_iter().map(|x| {
            let x = syn::Ident::new(x, trigger.pattern.span());
            quote::quote! {
                #crate_path::windows::AccelTable::#x
            }
        });
        quote::quote! { #(#mod_flags)|* }
    };

    quote::quote! {
        #crate_path::windows::ActionKeyBinding {
            action: #action,
            flags: #mod_flags,
            key: #vk,
        }
    }
}
