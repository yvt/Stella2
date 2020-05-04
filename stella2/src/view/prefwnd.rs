use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use tcw3::{
    pal,
    ui::{layouts::FillLayout, theming},
    uicore::{ActionId, HWnd, HWndRef, WndListener, WndStyleFlags},
};

use crate::{model, stylesheet, view::global};

// TODO: Most of these are copypasta of `WndView`, which hopefully we should
//       refactor.
pub(super) struct PrefWndView {
    hwnd: HWnd,
    dispatch: RefCell<Box<dyn Fn(model::AppAction)>>,
    pref_view: PrefView,
}

impl PrefWndView {
    pub(super) fn new(wm: pal::Wm) -> Rc<Self> {
        let hwnd = HWnd::new(wm);
        let style_manager = theming::Manager::global(wm);

        let pref_view = PrefViewBuilder::new()
            .with_wm(wm)
            .with_style_manager(style_manager)
            .build();

        hwnd.content_view()
            .set_layout(FillLayout::new(pref_view.view().clone()));

        hwnd.set_caption("Preferences");
        Self::update_wnd_style_flags(hwnd.as_ref(), false);
        hwnd.set_visibility(true);

        let this = Rc::new(Self {
            hwnd,
            dispatch: RefCell::new(Box::new(|_| {})),
            pref_view,
        });

        // Event handlers
        this.hwnd.set_listener(PrefWndViewWndListener {
            owner: Rc::downgrade(&this),
        });

        let this_weak = Rc::downgrade(&this);
        this.pref_view.subscribe_dispatch(Box::new(move |action| {
            if let Some(this) = this_weak.upgrade() {
                this.dispatch.borrow()(action);
            }
        }));

        let this_weak = Rc::downgrade(&this);
        this.pref_view.subscribe_close(Box::new(move || {
            if let Some(this) = this_weak.upgrade() {
                this.dispatch.borrow()(model::AppAction::HidePref);
            }
        }));

        let this_weak = Rc::downgrade(&this);
        this.hwnd.subscribe_focus(Box::new(move |_, _| {
            if let Some(this) = this_weak.upgrade() {
                this.update_focus();
            }
        }));

        this.update_focus();

        this
    }

    pub(super) fn set_dispatch(&self, cb: impl Fn(model::AppAction) + 'static) {
        *self.dispatch.borrow_mut() = Box::new(cb);
    }

    fn update_wnd_style_flags(hwnd: HWndRef, is_focused: bool) {
        hwnd.set_style_flags(
            if stylesheet::ENABLE_BACKDROP_BLUR && is_focused {
                WndStyleFlags::empty() | WndStyleFlags::TRANSPARENT_BACKDROP_BLUR
            } else {
                WndStyleFlags::empty()
            } | WndStyleFlags::FULL_SIZE_CONTENT,
        );
    }

    fn update_focus(&self) {
        let is_focused = self.hwnd.is_focused();
        if stylesheet::ENABLE_BACKDROP_BLUR {
            Self::update_wnd_style_flags(self.hwnd.as_ref(), is_focused);
        }
        self.pref_view.set_wnd_focused(is_focused);
    }
}

struct PrefWndViewWndListener {
    owner: Weak<PrefWndView>,
}

impl WndListener for PrefWndViewWndListener {
    fn close(&self, _: pal::Wm, _: HWndRef<'_>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.dispatch.borrow()(model::AppAction::HidePref);
        }
    }

    fn interpret_event(
        &self,
        _: pal::Wm,
        _: HWndRef<'_>,
        ctx: &mut tcw3::uicore::InterpretEventCtx<'_>,
    ) {
        global::interpret_event(ctx);
    }

    fn perform_action(&self, _: pal::Wm, _: HWndRef<'_>, _: ActionId) {
        // TODO: `global::QUIT` should be handled as an application-global action
    }
}

stella2_meta::designer_impl! {
    crate::view::prefwnd::PrefView
}
