//! Image assets for Stella2

pub type Stvg = (&'static [u8], [f32; 2]);

macro_rules! stvg {
    ($path:literal) => {
        stvg_macro::include_stvg!($path)
    };
}

pub static SEARCH: Stvg = stvg!("src/search.svg");

pub static LIST_GROUP_OPEN: Stvg = stvg!("src/list_group_open.svg");

pub mod toolbar {
    use super::*;

    pub static SIDEBAR_HIDE: Stvg = stvg!("src/toolbar/sidebar_hide.svg");
    pub static SIDEBAR_SHOW: Stvg = stvg!("src/toolbar/sidebar_show.svg");
    pub static USER_OUTLINE: Stvg = stvg!("src/toolbar/user_outline.svg");
    pub static MENU: Stvg = stvg!("src/toolbar/menu.svg");
}
