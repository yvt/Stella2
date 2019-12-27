designer_impl! { crate::objinit::shorthand::Comp }
designer_impl! { crate::objinit::shorthand::CompOther }

#[test]
fn check_inited_values() {
    let comp = CompBuilder::new().build();

    assert_eq!(comp.const1(), comp.other().const1());
}
