use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct AppState {
    pub main_wnd: Rc<WndState>,
}

#[derive(Debug, Clone)]
pub struct WndState {
    // UI state - It could be a local state of widget controllers, but we store
    // it here instead so that it can be intercepted by a persistence middleware
    pub sidebar_width: f32,
    pub editor_height: f32,
}

impl AppState {
    // TODO: Restore session state
    pub fn new() -> Self {
        Self {
            main_wnd: Rc::new(WndState {
                sidebar_width: 200.0,
                editor_height: 50.0,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AppAction {
    Wnd(WndAction),
}

#[derive(Debug, Clone)]
pub enum WndAction {
    SetSidebarWidth(f32),
    SetEditorHeight(f32),
}

impl AppState {
    pub fn reduce(this: Rc<Self>, action: AppAction) -> Rc<Self> {
        match action {
            AppAction::Wnd(wnd_action) => Rc::new(AppState {
                main_wnd: WndState::reduce(Rc::clone(&this.main_wnd), wnd_action),
            }),
        }
    }
}

impl WndState {
    fn reduce(this: Rc<Self>, action: WndAction) -> Rc<Self> {
        match action {
            WndAction::SetSidebarWidth(x) => {
                if x == this.sidebar_width {
                    this
                } else {
                    Rc::new(Self {
                        sidebar_width: x,
                        ..Self::clone(&this)
                    })
                }
            }
            WndAction::SetEditorHeight(x) => {
                if x == this.editor_height {
                    this
                } else {
                    Rc::new(Self {
                        editor_height: x,
                        ..Self::clone(&this)
                    })
                }
            }
        }
    }
}
