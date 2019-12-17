use tcw3::testing::{prelude::*, use_testing_wm};

designer_impl! { crate::field::prop::Comp }

#[use_testing_wm]
#[test]
fn prop_init_default(twm: &dyn TestingWm) {
    let comp = CompBuilder::new().with_wm(twm.wm()).build();
    assert_eq!(1, comp.prop1());
}

#[use_testing_wm]
#[test]
fn prop_init(twm: &dyn TestingWm) {
    let comp = CompBuilder::new().with_wm(twm.wm()).with_prop1(2).build();
    assert_eq!(2, comp.prop1());
}

#[use_testing_wm]
#[test]
fn prop_set(twm: &dyn TestingWm) {
    let comp = CompBuilder::new().with_wm(twm.wm()).build();
    comp.set_prop1(3);
    assert_eq!(1, comp.prop1());
    twm.step_unsend();
    assert_eq!(3, comp.prop1());
}
