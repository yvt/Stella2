use tcw3::testing::{prelude::*, use_testing_wm};

designer_impl! { crate::field::accessors::Comp }

#[use_testing_wm]
#[test]
fn get(twm: &dyn TestingWm) {
    let comp = CompBuilder::new().with_wm(twm.wm()).build();
    assert_eq!(1, comp.prop1());
    assert_eq!(2, *comp.prop2());
    assert_eq!(3, comp.const1());
    assert_eq!(4, *comp.const2());
}
