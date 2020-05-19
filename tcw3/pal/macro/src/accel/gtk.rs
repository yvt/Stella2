use proc_macro_error::abort;

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
                if trigger.source == "gtk" {
                    Some(gen_key_binding(crate_path, action, trigger))
                } else {
                    None
                }
            })
        })
        .flatten()
        .flatten();

    quote::quote! {
        #crate_path::gtk::AccelTable {
            key: &[#(#key_bindings),*],
        }
    }
}

fn gen_key_binding<'a>(
    crate_path: &'a syn::Path,
    action: &'a syn::Expr,
    trigger: &'a Trigger,
) -> impl Iterator<Item = proc_macro2::TokenStream> + 'a {
    let pat = match trigger.pattern.value().parse::<KeyPattern>() {
        Ok(pat) => pat,
        Err(e) => abort!(trigger.pattern.span(), "{}", e),
    };

    let keyval: &'static [&'static str] = match pat.key {
        // `gdkkeysyms.h`
        // Letters are upper/lower cased depending on the state of the
        // CapsLock and Shift keys, so we should be prepared for both cases.
        Key::Char('a') => &["a", "A"],
        Key::Char('b') => &["b", "B"],
        Key::Char('c') => &["c", "C"],
        Key::Char('d') => &["d", "D"],
        Key::Char('e') => &["e", "E"],
        Key::Char('f') => &["f", "F"],
        Key::Char('g') => &["g", "G"],
        Key::Char('h') => &["h", "H"],
        Key::Char('i') => &["i", "I"],
        Key::Char('j') => &["j", "J"],
        Key::Char('k') => &["k", "K"],
        Key::Char('l') => &["l", "L"],
        Key::Char('m') => &["m", "M"],
        Key::Char('n') => &["n", "N"],
        Key::Char('o') => &["o", "O"],
        Key::Char('p') => &["p", "P"],
        Key::Char('q') => &["q", "Q"],
        Key::Char('r') => &["r", "R"],
        Key::Char('s') => &["s", "S"],
        Key::Char('t') => &["t", "T"],
        Key::Char('u') => &["u", "U"],
        Key::Char('v') => &["v", "V"],
        Key::Char('w') => &["w", "W"],
        Key::Char('x') => &["x", "X"],
        Key::Char('y') => &["y", "Y"],
        Key::Char('z') => &["z", "Z"],
        Key::Char(' ') => &["space"],
        Key::Escape => &["Escape"],
        Key::Backspace => &["BackSpace"],
        Key::Return => &["Return"],
        Key::Tab => &["Tab"],
        Key::Delete => &["Delete"],
        Key::Left => &["Left"],
        Key::Up => &["Up"],
        Key::Right => &["Right"],
        Key::Down => &["Down"],
        Key::PageUp => &["Page_Up"],
        Key::PageDown => &["Page_Down"],
        Key::End => &["End"],
        Key::Home => &["Home"],
        Key::Insert => &["Insert"],
        Key::Numpad0 => &["KP_0"],
        Key::Numpad1 => &["KP_1"],
        Key::Numpad2 => &["KP_2"],
        Key::Numpad3 => &["KP_3"],
        Key::Numpad4 => &["KP_4"],
        Key::Numpad5 => &["KP_5"],
        Key::Numpad6 => &["KP_6"],
        Key::Numpad7 => &["KP_7"],
        Key::Numpad8 => &["KP_8"],
        Key::Numpad9 => &["KP_9"],
        Key::NumpadMultiply => &["KP_Multiply"],
        Key::NumpadAdd => &["KP_Add"],
        Key::NumpadSeparator => &["KP_Separator"],
        Key::NumpadSubtract => &["KP_Subtract"],
        Key::NumpadDecimal => &["KP_Decimal"],
        Key::NumpadDivide => &["KP_Divide"],
        Key::F1 => &["F1"],
        Key::F2 => &["F2"],
        Key::F3 => &["F3"],
        Key::F4 => &["F4"],
        Key::F5 => &["F5"],
        Key::F6 => &["F6"],
        Key::F7 => &["F7"],
        Key::F8 => &["F8"],
        Key::F9 => &["F9"],
        Key::F10 => &["F10"],
        Key::F11 => &["F11"],
        Key::F12 => &["F12"],
        Key::F13 => &["F13"],
        Key::F14 => &["F14"],
        Key::F15 => &["F15"],
        Key::F16 => &["F16"],
        Key::F17 => &["F17"],
        Key::F18 => &["F18"],
        Key::F19 => &["F19"],
        Key::F20 => &["F20"],
        Key::F21 => &["F21"],
        Key::F22 => &["F22"],
        Key::F23 => &["F23"],
        Key::F24 => &["F24"],

        unknown => {
            abort!(trigger.pattern.span(), "Unsupported key: `{:?}`", unknown);
        }
    };

    let kv_span = trigger.pattern.span();
    let keyval = keyval.iter().map(move |&kv| syn::Ident::new(kv, kv_span));

    // `AccelTable::MOD_*`
    let mut mod_flags = Vec::new();
    if pat.mod_flags.contains(ModFlags::SHIFT) {
        mod_flags.push("MOD_SHIFT");
    }
    if pat.mod_flags.contains(ModFlags::CONTROL) {
        mod_flags.push("MOD_CONTROL");
    }
    if pat.mod_flags.contains(ModFlags::ALT) {
        mod_flags.push("MOD_META");
    }
    if pat.mod_flags.contains(ModFlags::SUPER) {
        mod_flags.push("MOD_SUPER");
    }

    let mod_flags = if mod_flags.is_empty() {
        quote::quote! { 0 }
    } else {
        let mod_flags = mod_flags.into_iter().map(|x| {
            let x = syn::Ident::new(x, trigger.pattern.span());
            quote::quote! {
                #crate_path::gtk::AccelTable::#x
            }
        });
        quote::quote! { #(#mod_flags)|* }
    };

    keyval.map(move |keyval| {
        quote::quote! {
            #crate_path::gtk::ActionKeyBinding {
                action: #action,
                mod_flags: #mod_flags,
                keyval: #crate_path::gtk::gdk_keys::#keyval,
            }
        }
    })
}
