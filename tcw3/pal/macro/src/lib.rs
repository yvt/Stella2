//! Provides the internal implementation of `tcw3_pal::new_accel`.
extern crate proc_macro;

mod accel;
mod keycode;

#[proc_macro_hack::proc_macro_hack]
#[proc_macro_error::proc_macro_error]
pub fn accel_table_inner(params: proc_macro::TokenStream) -> proc_macro::TokenStream {
    accel::accel_table_inner(params)
}
