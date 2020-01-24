use harmony::{Elem, ShallowEq};
use std::{cell::Cell, time::Duration};
use tcw3::pal::{iface::Wm as _, HInvoke, Wm};

use super::profile::Profile;
use crate::model;

/// State updates are persisted to disk after waiting for
/// at least this duration.
const DEBOUNCE_LATENCY_MIN: Duration = Duration::from_secs(5);
const DEBOUNCE_LATENCY_MAX: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
struct PersistedState {
    main_wnd: Elem<model::WndState>,
}

impl PersistedState {
    fn new(app_state: &model::AppState) -> Elem<Self> {
        Elem::new(Self {
            main_wnd: app_state.main_wnd.clone(),
        })
    }

    fn reduce(this: &Elem<Self>, app_state: &model::AppState) -> Option<Elem<Self>> {
        if this.main_wnd.shallow_ne(&app_state.main_wnd) {
            Some(Self::new(app_state))
        } else {
            None
        }
    }
}

/// Restore the application state from a given profile. `state` will be
/// updated with the restored state.
pub fn restore_state(
    _profile: &'static Profile,
    app_state: Elem<model::AppState>,
) -> Elem<model::AppState> {
    // TODO
    log::warn!("restore_state: todo!");
    app_state
}

pub struct PersistenceScheduler {
    persisted: Cell<Option<Elem<PersistedState>>>,
    timer: Cell<Option<HInvoke>>,
}

impl PersistenceScheduler {
    pub fn new(app_state: &model::AppState) -> Self {
        Self {
            persisted: Cell::new(Some(PersistedState::new(app_state))),
            timer: Cell::new(None),
        }
    }

    // TODO: Force synchronization on quit

    /// Persist the current app state to disk immediately.
    pub fn flush(&self, _wm: Wm, _app_state: &model::AppState) {
        // TODO
        log::warn!("flush: todo!");
    }

    /// Schedule the persistence of the current app state.
    pub fn handle_update(&self, wm: Wm, app_state: &model::AppState) {
        // Temporarily take `persisted`
        let mut persisted = self.persisted.take();

        if let Some(new_persisted) = PersistedState::reduce(persisted.as_ref().unwrap(), app_state)
        {
            let new_persisted_2 = new_persisted.clone();
            persisted = Some(new_persisted);

            // Cancel the previous invocation so that if the state was updated
            // for many times within a short time, the state is saved only once.
            if let Some(timer) = self.timer.take() {
                wm.cancel_invoke(&timer);
            }

            // Schedule a persist task
            let timer = wm.invoke_after(DEBOUNCE_LATENCY_MIN..DEBOUNCE_LATENCY_MAX, move |_| {
                let _persisted = new_persisted_2;
                // TODO
                log::warn!("handle_update: todo! state persistence task");
            });

            log::trace!(
                "Scheduled the state persistence task ({:?}) to run in {:?}",
                timer,
                DEBOUNCE_LATENCY_MIN..DEBOUNCE_LATENCY_MAX
            );

            self.timer.set(Some(timer));
        }

        // Put it back
        self.persisted.set(persisted);
    }
}
