//! Image assets for Stella2

pub type Stvg = (&'static [u8], [f32; 2]);

macro_rules! stvg {
    ($path:literal) => {
        stvg_macro::include_stvg!($path)
    };
}

pub static SEARCH: Stvg = stvg!("src/search.svg");
pub static CLOSE: Stvg = stvg!("src/close.svg");

pub static LIST_GROUP_OPEN: Stvg = stvg!("src/list_group_open.svg");

pub mod pref {
    use super::*;

    pub static TAB_ABOUT: Stvg = stvg!("src/pref/tab_about.svg");
    pub static TAB_ACCOUNTS: Stvg = stvg!("src/pref/tab_accounts.svg");
    pub static TAB_ADVANCED: Stvg = stvg!("src/pref/tab_advanced.svg");
    pub static TAB_CONNECTION: Stvg = stvg!("src/pref/tab_connection.svg");
    pub static TAB_GENERAL: Stvg = stvg!("src/pref/tab_general.svg");
}

pub mod toolbar {
    use super::*;

    pub static SIDEBAR_HIDE: Stvg = stvg!("src/toolbar/sidebar_hide.svg");
    pub static SIDEBAR_SHOW: Stvg = stvg!("src/toolbar/sidebar_show.svg");
    pub static USER_OUTLINE: Stvg = stvg!("src/toolbar/user_outline.svg");
    pub static MENU: Stvg = stvg!("src/toolbar/menu.svg");
}
