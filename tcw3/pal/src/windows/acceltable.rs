//! Accelerator tables
//!
//! Most of these definitions are implementation details and thus hidden. They
//! still need to be `pub` because they are instantiated by `accel_table!`.
use winapi::um::winuser;

use crate::iface;

#[derive(Debug)]
pub struct AccelTable {
    #[doc(hidden)]
    pub key: &'static [ActionKeyBinding],
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ActionKeyBinding {
    pub action: iface::ActionId,
    pub flags: u8,
    pub key: u16,
}

impl AccelTable {
    pub const MOD_SHIFT: u8 = 1 << 5;
    pub const MOD_CONTROL: u8 = 1 << 6;
    pub const MOD_MENU: u8 = 1 << 7;

    /// Query the current key modifier status of the user's desktop.
    pub(super) fn query_mod_flags() -> u8 {
        const DOWN: u16 = 0x8000;

        macro_rules! imp {
            ( $(($key:expr, $flag:expr)),*$(,)* ) => {
                $(
                    (unsafe { winuser::GetKeyState($key) } as u16 & DOWN)
                        / (DOWN / $flag as u16)
                )|*
            };
        }

        imp!(
            (winuser::VK_SHIFT, Self::MOD_SHIFT),
            (winuser::VK_CONTROL, Self::MOD_CONTROL),
            (winuser::VK_MENU, Self::MOD_MENU)
        ) as u8
    }

    pub(super) fn find_action_with_key(&self, key: u16, flags: u8) -> Option<iface::ActionId> {
        self.key
            .iter()
            .filter(move |binding| flags == binding.flags && key == binding.key)
            .map(|binding| binding.action)
            .nth(0)
    }
}
