use cggeom::box2;
use try_match::try_match;

use tcw3::{
    pal,
    testing::{prelude::*, use_testing_wm},
    ui::{layouts::AbsLayout, AlignFlags},
    uicore::{HView, HViewRef, HWnd, SizeTraits, ViewFlags, ViewListener},
};

struct VL;

impl ViewListener for VL {
    fn position(&self, _: pal::Wm, view: HViewRef<'_>) {
        assert_eq!(
            view.frame(),
            box2! { min: [-20.0, 30.0], max: [80.0, 50.0] }
        );
        assert_eq!(
            view.global_frame(),
            box2! { min: [-20.0, 30.0], max: [80.0, 50.0] }
        );
        assert_eq!(
            view.global_visible_frame(),
            box2! { min: [0.0, 30.0], max: [80.0, 50.0] }
        );
    }
}

#[use_testing_wm]
#[test]
fn position_event(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let view = HView::new(ViewFlags::empty());
    view.set_listener(VL);

    wnd.content_view().set_layout(AbsLayout::new(
        SizeTraits {
            min: [100.0, 100.0].into(),
            max: [100.0, 100.0].into(),
            preferred: [100.0, 100.0].into(),
        },
        vec![(
            view.clone(),
            box2! { min: [-20.0, 30.0], max: [80.0, 50.0] },
            AlignFlags::JUSTIFY,
        )],
    ));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();
}
