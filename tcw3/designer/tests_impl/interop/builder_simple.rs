use std::cell::Cell;

designer_impl! { crate::interop::builder_simple::Comp }

#[test]
fn check_inited_values() {
    let comp = CompBuilder::new().build();

    assert_eq!(*comp.c1().const1(), 1);
    assert_eq!(*comp.c1().const2(), 2);
    assert_eq!(comp.c1().prop1(), 3);
    assert_eq!(*comp.c2().const1(), 0);
    assert_eq!(*comp.c2().const2(), 2);
    assert_eq!(comp.c2().prop1(), 0);
}

struct ExtComp {
    const1: u32,
    const2: u32,
    prop1: Cell<u32>,
}

impl ExtComp {
    fn new(const1: u32, const2: u32) -> Self {
        Self {
            const1,
            const2,
            prop1: Cell::new(0),
        }
    }

    fn const1(&self) -> &u32 {
        &self.const1
    }

    fn const2(&self) -> &u32 {
        &self.const2
    }

    fn prop1(&self) -> u32 {
        self.prop1.get()
    }

    fn set_prop1(&self, x: u32) {
        self.prop1.set(x);
    }
}
