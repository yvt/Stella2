#![allow(dead_code)]

designer_impl! { crate::misc::exprpath::Comp1 }
designer_impl! { crate::misc::exprpath::Comp2 }
designer_impl! { crate::misc::exprpath::Comp3 }

pub mod submod {
    // When the dynamic expressions in these components mention `doit`, it's
    // expanded to an absolute path by `use crate::…::submod`, so this function
    // will be used. If the expansion is not done correctly (e.g., in the
    // previous behavior), the above `designer_impl!` lines will refer to
    // `doit`, which is non-existent in their scope, causing a compilation
    // error.
    pub fn doit(_: u32) {
        dbg!();
    }
}
