// This executable is used to test the generation of the Windows resource
// done by `build.rs`. It literally does nothing and only used to be
// manually inspected by developers.
fn main() {}

// Make sure `stella2_assets` is linked
#[used]
static X: &'static stella2_assets::Stvg = &stella2_assets::toolbar::GO_BACK;
