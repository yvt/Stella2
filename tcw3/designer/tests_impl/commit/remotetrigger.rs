use tcw3::testing::{prelude::*, use_testing_wm};

designer_impl! { crate::commit::remotetrigger::Comp }
designer_impl! { crate::commit::remotetrigger::CompOther }

#[use_testing_wm]
#[test]
fn watch_prop(twm: &dyn TestingWm) {
    let comp = CompBuilder::new().with_wm(twm.wm()).build();
    assert_eq!(0, comp.state().get());

    comp.other().raise_prop1_changed();

    assert_eq!(0, comp.state().get());

    twm.step_unsend();
    // TOOD: assert_eq!(1, comp.state().get());
}

#[use_testing_wm]
#[test]
fn watch_event(twm: &dyn TestingWm) {
    let comp = CompBuilder::new().with_wm(twm.wm()).build();
    assert_eq!(0, comp.state().get());

    comp.other().raise_event1();

    // `on (event_input)` should handle events synchronously
    assert_eq!(4, comp.state().get());
}
