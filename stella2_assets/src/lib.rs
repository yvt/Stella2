//! Image assets for Stella2
#![feature(proc_macro_hygiene)]

pub type Stvg = (&'static [u8], [f32; 2]);

macro_rules! stvg {
    ($path:literal) => {
        stvg_macro::include_stvg!($path)
    };
}

pub mod toolbar {
    use super::*;

    pub static SIDEBAR_HIDE: Stvg = stvg!("stella2_assets/src/toolbar/sidebar_hide.svg");
    pub static SIDEBAR_SHOW: Stvg = stvg!("stella2_assets/src/toolbar/sidebar_show.svg");
    pub static GO_BACK: Stvg = stvg!("stella2_assets/src/toolbar/go_back.svg");
    pub static GO_FORWARD: Stvg = stvg!("stella2_assets/src/toolbar/go_forward.svg");
}
