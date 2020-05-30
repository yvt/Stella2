//! Accelerator tables
//!
//! Most of these definitions are implementation details and thus hidden. They
//! still need to be `pub` because they are instantiated by `accel_table!`.
use winapi::um::winuser;

use crate::{actions, iface};

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

pub(super) static TEXT_INPUT_ACCEL: AccelTable = tcw3_pal_macro::accel_table_inner!(
    crate,
    "windows",
    [
        (actions::DELETE_BACKWARD, windows("Backspace")),
        (actions::DELETE_BACKWARD_WORD, windows("Ctrl+Backspace")),
        (actions::DELETE_FORWARD, windows("Delete")),
        (actions::DELETE_FORWARD_WORD, windows("Ctrl+Delete")),
        (actions::INSERT_LINE_BREAK, windows("Shift+Return")),
        (actions::INSERT_PARAGRAPH_BREAK, windows("Return")),
        (actions::INSERT_TAB, windows("Tab")),
        (actions::INSERT_BACKTAB, windows("Shift+Tab")),
        (actions::MOVE_LEFT, windows("Left")),
        (actions::MOVE_RIGHT, windows("Right")),
        (actions::MOVE_LEFT_WORD, windows("Ctrl+Left")),
        (actions::MOVE_RIGHT_WORD, windows("Ctrl+Right")),
        (actions::MOVE_START_OF_LINE, windows("Home")),
        (actions::MOVE_END_OF_LINE, windows("End")),
        (actions::MOVE_START_OF_PARAGRAPH, windows("Ctrl+Up")),
        (actions::MOVE_END_OF_PARAGRAPH, windows("Ctrl+Down")),
        (actions::MOVE_START_OF_DOCUMENT, windows("Ctrl+Home")),
        (actions::MOVE_END_OF_DOCUMENT, windows("Ctrl+End")),
        (actions::MOVE_UP, windows("Up")),
        (actions::MOVE_DOWN, windows("Down")),
        (actions::MOVE_UP_PAGE, windows("PageUp")),
        (actions::MOVE_DOWN_PAGE, windows("PageDown")),
        (actions::MOVE_LEFT_SELECTING, windows("Shift+Left")),
        (actions::MOVE_RIGHT_SELECTING, windows("Shift+Right")),
        (
            actions::MOVE_LEFT_WORD_SELECTING,
            windows("Shift+Ctrl+Left")
        ),
        (
            actions::MOVE_RIGHT_WORD_SELECTING,
            windows("Shift+Ctrl+Right")
        ),
        (actions::MOVE_START_OF_LINE_SELECTING, windows("Shift+Home")),
        (actions::MOVE_END_OF_LINE_SELECTING, windows("Shift+End")),
        (
            actions::MOVE_START_OF_PARAGRAPH_SELECTING,
            windows("Shift+Ctrl+Up")
        ),
        (
            actions::MOVE_END_OF_PARAGRAPH_SELECTING,
            windows("Shift+Ctrl+Down")
        ),
        (
            actions::MOVE_START_OF_DOCUMENT_SELECTING,
            windows("Shift+Ctrl+Home")
        ),
        (
            actions::MOVE_END_OF_DOCUMENT_SELECTING,
            windows("Shift+Ctrl+End")
        ),
        (actions::MOVE_UP_SELECTING, windows("Shift+Up")),
        (actions::MOVE_DOWN_SELECTING, windows("Shift+Down")),
        (actions::MOVE_UP_PAGE_SELECTING, windows("Shift+PageUp")),
        (actions::MOVE_DOWN_PAGE_SELECTING, windows("Shift+PageDown")),
    ]
);
