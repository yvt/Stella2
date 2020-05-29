//! Defines a platform-neutral key notation.

#[derive(Debug, Clone, Copy, enum_utils::FromStr)]
pub enum Key {
    /// A character without any key modifiers applied. Does not include
    /// the inputs by a numeric keypad.
    #[enumeration(skip)]
    Char(char),
    Backspace,
    Tab,
    Return,
    Escape,
    PageUp,
    PageDown,
    End,
    Home,
    Left,
    Up,
    Right,
    Down,
    Insert,
    Delete,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadMultiply,
    NumpadAdd,
    NumpadSeparator,
    NumpadSubtract,
    NumpadDecimal,
    NumpadDivide,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
}

bitflags::bitflags! {
    pub struct ModFlags: u8 {
        const SHIFT = 1;
        const CONTROL = 1 << 1;
        const ALT = 1 << 2;
        const SUPER = 1 << 3;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct KeyPattern {
    pub key: Key,
    pub mod_flags: ModFlags,
}

impl std::str::FromStr for KeyPattern {
    type Err = String;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        let mut mod_flags = ModFlags::empty();

        loop {
            if s.starts_with("Shift+") {
                mod_flags |= ModFlags::SHIFT;
                s = &s[6..];
            } else if s.starts_with("Ctrl+") {
                mod_flags |= ModFlags::CONTROL;
                s = &s[5..];
            } else if s.starts_with("Alt+") {
                mod_flags |= ModFlags::ALT;
                s = &s[4..];
            } else if s.starts_with("Super+") {
                mod_flags |= ModFlags::SUPER;
                s = &s[6..];
            } else {
                break;
            }
        }

        let key = if let Ok(key) = s.parse::<Key>() {
            key
        } else {
            // If it's exactly a single character, treat it as a char code
            // pattern. Otherwise, it represents an unknown key.
            if s.chars().nth(1).is_some() {
                return Err(format!("Unknown key: {:?}", s));
            }

            if s.is_empty() {
                return Err("Key symbol is missing".to_owned());
            }

            // For convenience, convert the character to lower case
            Key::Char(s.chars().next().unwrap().to_ascii_lowercase())
        };

        Ok(Self { key, mod_flags })
    }
}
