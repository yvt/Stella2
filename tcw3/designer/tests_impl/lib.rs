//! Tests for TCW3 Designer-generated code. Please see `tcw3_designer`'s
//! documentation for how all tests are organized.
#![cfg(test)]

include!(concat!(env!("OUT_DIR"), "/designer.rs"));

mod commit {
    mod remotetrigger;
}

mod field {
    mod accessors;
    mod lifetime_elision;
    mod prop;
}

mod func {
    mod inline;
}

mod interop {
    mod builder_simple;
}

mod objinit {
    mod shorthand;
}
