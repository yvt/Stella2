//! Windows resources for Stella2
//!
//! [Resources] are read-only data embedded in an executable file. The purposes
//! of resources include (but not limited to) providing an application icon to
//! be displayed in Windows Explorer.
//!
//! Using [`embed-resource`], this crate produces an object file including
//! resources. The resources are added to the executable file when the object
//! file is linked.
//!
//! [Resources]: https://en.wikipedia.org/wiki/Resource_%28Windows%29
//! [`embed-resource`]: https://crates.io/crates/embed-resource

#[doc(hidden)]
pub static DUMMY: u8 = 0;

/// Ensures the object file containing Windows resources is linked.
#[macro_export]
macro_rules! attach_windres {
    () => {
        #[used]
        #[allow(dead_code)]
        static WINDRES_DUMMY: &'static u8= &$crate::DUMMY;
    };
}
