designer_impl! { crate::func::inline::Comp }

#[test]
fn check_evaluated_values() {
    let comp = CompBuilder::new().build();

    assert_eq!(*comp.const1(), 42);
    assert_eq!(*comp.const2(), 42 * 2);
    assert_eq!(*comp.const3(), 42 * 3);
    assert_eq!(comp.const4()[0], 42 * 4);
    assert_eq!(comp.const4()[1], 5);
    assert_eq!(comp.const5()[0][0], 1);
    assert_eq!(comp.const5()[1][0], 42 * 4);
    assert_eq!(comp.const5()[1][1], 5);
    assert_eq!(comp.const5()[2][0], 3);
}
