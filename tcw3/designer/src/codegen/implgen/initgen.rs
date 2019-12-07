use std::fmt::Write;

/// Generates construction code for a component.
///
/// Assumes settable `const`s are already in the scope.
pub fn gen_builder(
    comp: &sem::CompDef<'_>,
    meta_comp: &metadata::CompDef,
    comp_ident: &proc_macro2::Ident,
    _diag: &mut Diag,
    out: &mut String,
) {
    // TODO
    writeln!(out, "unimplemented!();").unwrap();
}

enum DepNode {
    Field {
        item_i: usize,
    },
    ObjInitField {
        item_i: usize,
        field_i: usize,
    },
}
