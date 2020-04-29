use proc_macro_error::abort;
use std::convert::TryFrom;

use super::{MacroInput, Trigger};
use crate::keycode::{Key, KeyPattern, ModFlags};

// `NSEventModifierFlags`
const NS_SHIFT_KEY_MASK: u32 = 1 << 17;
const NS_CONTROL_KEY_MASK: u32 = 1 << 18;
const NS_ALTERNATE_KEY_MASK: u32 = 1 << 19;
const NS_COMMAND_KEY_MASK: u32 = 1 << 20;
const NS_NUMERIC_PAD_KEY_MASK: u32 = 1 << 21;

pub(super) fn gen_accel_table(input: &MacroInput) -> proc_macro2::TokenStream {
    let crate_path = &input.crate_path;

    let key_bindings = input
        .bindings
        .iter()
        .map(move |binding| {
            let action = &binding.action;
            binding.triggers.iter().filter_map(move |trigger| {
                if trigger.source == "macos" {
                    Some(gen_key_binding(crate_path, action, trigger))
                } else {
                    None
                }
            })
        })
        .flatten();

    let sel_bindings = input
        .bindings
        .iter()
        .map(move |binding| {
            let action = &binding.action;
            binding.triggers.iter().filter_map(move |trigger| {
                if trigger.source == "macos_sel" {
                    let sel = &trigger.pattern;
                    Some(quote::quote! {
                        #crate_path::macos::ActionSelBinding {
                            action: #action,
                            sel: #sel
                        }
                    })
                } else {
                    None
                }
            })
        })
        .flatten();

    quote::quote! {
        #crate_path::macos::AccelTable {
            key: &[#(#key_bindings),*],
            sel: &[#(#sel_bindings),*],
        }
    }
}

fn gen_key_binding(
    crate_path: &syn::Path,
    action: &syn::Expr,
    trigger: &Trigger,
) -> proc_macro2::TokenStream {
    let pat = match trigger.pattern.value().parse::<KeyPattern>() {
        Ok(pat) => pat,
        Err(e) => abort!(trigger.pattern.span(), "{}", e),
    };

    let charcode = match pat.key {
        Key::Char(c) => {
            let c = c as u32;
            if let Ok(x) = u16::try_from(c) {
                x
            } else {
                abort!(
                    trigger.pattern.span(),
                    "The character is out of Unicode BMP range"
                );
            }
        }
        Key::Escape => 0x001b,
        // <https://developer.apple.com/documentation/appkit/1540619-common_unicode_characters>
        Key::Backspace => 0x0008,
        Key::Return => 0x0003,
        Key::Tab => {
            if pat.mod_flags.contains(ModFlags::SHIFT) {
                0x0019
            } else {
                0x0009
            }
        }
        Key::Delete => 0x007f,
        Key::Left => 0xf702,
        Key::Up => 0xf700,
        Key::Right => 0xf703,
        Key::Down => 0xf701,
        // <https://developer.apple.com/documentation/appkit/1535851-function-key_unicodes>
        Key::PageUp => 0xf72c,
        Key::PageDown => 0xf72d,
        Key::End => 0xf72b,
        Key::Home => 0xf729,
        Key::Insert => 0xf727,
        Key::Numpad0 => b'0' as u16,
        Key::Numpad1 => b'1' as u16,
        Key::Numpad2 => b'2' as u16,
        Key::Numpad3 => b'3' as u16,
        Key::Numpad4 => b'4' as u16,
        Key::Numpad5 => b'5' as u16,
        Key::Numpad6 => b'6' as u16,
        Key::Numpad7 => b'7' as u16,
        Key::Numpad8 => b'8' as u16,
        Key::Numpad9 => b'9' as u16,
        Key::NumpadMultiply => b'*' as u16,
        Key::NumpadAdd => b'+' as u16,
        Key::NumpadSeparator => b',' as u16,
        Key::NumpadSubtract => b'-' as u16,
        Key::NumpadDecimal => b'.' as u16,
        Key::NumpadDivide => b'/' as u16,
        Key::F1 => 0xf704,
        Key::F2 => 0xf705,
        Key::F3 => 0xf706,
        Key::F4 => 0xf707,
        Key::F5 => 0xf708,
        Key::F6 => 0xf709,
        Key::F7 => 0xf70a,
        Key::F8 => 0xf70b,
        Key::F9 => 0xf70c,
        Key::F10 => 0xf70d,
        Key::F11 => 0xf70e,
        Key::F12 => 0xf70f,
        Key::F13 => 0xf710,
        Key::F14 => 0xf711,
        Key::F15 => 0xf712,
        Key::F16 => 0xf713,
        Key::F17 => 0xf714,
        Key::F18 => 0xf715,
        Key::F19 => 0xf716,
        Key::F20 => 0xf717,
        Key::F21 => 0xf718,
        Key::F22 => 0xf719,
        Key::F23 => 0xf71a,
        Key::F24 => 0xf71b,
    };

    // Some characters are included in both of a normal keyboard and a numerical
    // keypad. `Key` distinguishes between them.
    let needs_keypad_disambiguation = if charcode < 128 {
        "0123456789*+,-./".as_bytes().contains(&(charcode as u8))
    } else {
        false
    };

    let expects_keypad = matches!(
        pat.key,
        Key::Numpad0
            | Key::Numpad1
            | Key::Numpad2
            | Key::Numpad3
            | Key::Numpad4
            | Key::Numpad5
            | Key::Numpad6
            | Key::Numpad7
            | Key::Numpad8
            | Key::Numpad9
            | Key::NumpadMultiply
            | Key::NumpadAdd
            | Key::NumpadSeparator
            | Key::NumpadSubtract
            | Key::NumpadDecimal
            | Key::NumpadDivide
    );

    let mut mod_mask =
        NS_SHIFT_KEY_MASK | NS_CONTROL_KEY_MASK | NS_ALTERNATE_KEY_MASK | NS_COMMAND_KEY_MASK;
    if needs_keypad_disambiguation {
        mod_mask |= NS_NUMERIC_PAD_KEY_MASK;
    }

    let mut mod_flags = 0;
    if pat.mod_flags.contains(ModFlags::SHIFT) {
        mod_flags |= NS_SHIFT_KEY_MASK;
    }
    if pat.mod_flags.contains(ModFlags::CONTROL) {
        mod_flags |= NS_CONTROL_KEY_MASK;
    }
    if pat.mod_flags.contains(ModFlags::ALT) {
        mod_flags |= NS_ALTERNATE_KEY_MASK;
    }
    if pat.mod_flags.contains(ModFlags::SUPER) {
        mod_flags |= NS_COMMAND_KEY_MASK;
    }
    if expects_keypad {
        mod_flags |= NS_NUMERIC_PAD_KEY_MASK;
    }

    // Lower bits (presumably) only have device-dependent flags, so we simply
    // ignore them
    let mod_mask = (mod_mask >> 16) as u16;
    let mod_flags = (mod_flags >> 16) as u16;

    quote::quote! {
        #crate_path::macos::ActionKeyBinding {
            action: #action,
            mod_mask: #mod_mask,
            mod_flags: #mod_flags,
            charcode: #charcode,
        }
    }
}
