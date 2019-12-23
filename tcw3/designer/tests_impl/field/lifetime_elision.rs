designer_impl! { crate::field::lifetime_elision::Comp }

#[test]
fn have_correct_types() {
    let comp = CompBuilder::new()
        .with_field1a("hello")
        .with_field1b("hello")
        .with_field2a(|st: &str| st)
        .with_field2b(|st: &str| st)
        .with_field3a(&|st: &str| st)
        .with_field3b(&|st: &str| st)
        .with_field4a(&&42u32)
        .with_field4b(&&42u32)
        .build();

    let _x: &'static str = *comp.field1a();
    let _x: &'static str = *comp.field1b();
    let _x: fn(&'static str) -> &'static str = *comp.field2a(); // `'a` ← `'static`
    let _x: fn(&'static str) -> &'static str = *comp.field2b(); // `'a` ← `'static`
    let _x: &'static dyn Fn(&'static str) -> &'static str = *comp.field3a(); // `'a` ← `'static`
    let _x: &'static dyn Fn(&'static str) -> &'static str = *comp.field3b(); // `'a` ← `'static`
    let _x: &'static dyn std::cmp::PartialEq<&'static u32> = *comp.field4a(); // `'a` ← `'static`
    let _x: &'static dyn std::cmp::PartialEq<&'static u32> = *comp.field4b();
}
