designer_impl! { crate::func::inline::Comp }

#[test]
fn check_evaluated_values() {
    let comp = CompBuilder::new().build();

    assert_eq!(*comp.const1(), 42);
    assert_eq!(*comp.const2(), 42 * 2);
    assert_eq!(*comp.const3(), 42 * 3);
}
