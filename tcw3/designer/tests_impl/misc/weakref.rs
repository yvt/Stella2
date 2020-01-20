designer_impl! { crate::misc::weakref::Comp }

#[test]
fn upgrade_alive() {
    let comp = CompBuilder::new().build();
    comp.downgrade().upgrade().unwrap();
}

#[test]
fn upgrade_dead() {
    let comp = CompBuilder::new().build();
    let weak = comp.downgrade();
    drop(comp);
    assert!(weak.upgrade().is_none());
}
