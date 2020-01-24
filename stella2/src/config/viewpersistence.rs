use harmony::{set_field, Elem, ShallowEq};
use miniserde::{json, Deserialize, Serialize};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    time::Duration,
};
use tcw3::pal::{iface::Wm as _, HInvoke, Wm};

use super::profile::Profile;
use crate::model;

/// State updates are persisted to disk after waiting for
/// at least this duration.
const DEBOUNCE_LATENCY_MIN: Duration = Duration::from_secs(5);
const DEBOUNCE_LATENCY_MAX: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    main_wnd: Elem<model::WndState>,
}

impl PersistedState {
    fn new(app_state: &model::AppState) -> Elem<Self> {
        Elem::new(Self {
            main_wnd: app_state.main_wnd.clone(),
        })
    }

    fn merge_into_app(self, app_state: Elem<model::AppState>) -> Elem<model::AppState> {
        set_field! {
            main_wnd: self.main_wnd,
            ..app_state
        }
    }

    fn merge_from_app(this: &Elem<Self>, app_state: &model::AppState) -> Option<Elem<Self>> {
        if this.main_wnd.shallow_ne(&app_state.main_wnd) {
            Some(Self::new(app_state))
        } else {
            None
        }
    }
}

/// The file path to store the application state.
fn state_path(profile: &Profile) -> PathBuf {
    profile.data_dir().join("view.json")
}

/// The temporary file path used during saving the application state.
fn state_tmp_path(profile: &Profile) -> PathBuf {
    profile.data_dir().join(".view.json.tmp")
}

/// Restore the application state from a given profile. `state` will be
/// updated with the restored state.
pub fn restore_state(profile: &Profile, app_state: Elem<model::AppState>) -> Elem<model::AppState> {
    let state_path = state_path(profile);

    if state_path.is_file() {
        log::info!("Loading a persisted state from {:?}.", state_path);

        // Load `PersistedState` from the file
        match load_persisted_state(&state_path) {
            Ok(st) => {
                return st.merge_into_app(app_state);
            }
            Err(e) => {
                // TODO: Report the error to the user
                log::error!("Could not read the persisted state: {}", e);
            }
        }
    } else {
        log::info!(
            "The persisted state file was not found at {:?}.",
            state_path
        );
    }

    app_state
}

fn load_persisted_state(path: &Path) -> Result<PersistedState, std::io::Error> {
    let json = std::fs::read_to_string(path)?;

    #[derive(Debug, displaydoc::Display)]
    enum Error {
        /// Deserialization failed.
        DeserializationFailure,
    }

    impl std::error::Error for Error {}

    json::from_str(&json).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            Error::DeserializationFailure,
        )
    })
}

fn write_state_file(path: &Path, tmp_path: &Path, json: &str) -> Result<(), std::io::Error> {
    // Use a temporary file `tmp_path` to atomically update `patH`.
    std::fs::write(tmp_path, json)?;

    // Move `tmp_path` to `path`, overwriting it. It's okay to leave
    // `tmp_path` on failure.
    std::fs::rename(tmp_path, path)
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
    pub fn flush(&self, _wm: Wm, app_state: &model::AppState, profile: &'static Profile) {
        let mut persisted = self.persisted.take();

        if let Some(new_persisted) =
            PersistedState::merge_from_app(persisted.as_ref().unwrap(), app_state)
        {
            persisted = Some(new_persisted.clone());

            Self::save_persisted_state_with_profile_and_report_error_on_background(
                profile,
                &new_persisted,
            );

            // TODO: Wait until it is done
        }

        // Put it back
        self.persisted.set(persisted);
    }

    /// Schedule the persistence of the current app state.
    pub fn handle_update(&self, wm: Wm, app_state: &model::AppState, profile: &'static Profile) {
        // Temporarily take `persisted`
        let mut persisted = self.persisted.take();

        if let Some(new_persisted) =
            PersistedState::merge_from_app(persisted.as_ref().unwrap(), app_state)
        {
            persisted = Some(new_persisted.clone());

            // Cancel the previous invocation so that if the state was updated
            // for many times within a short time, the state is saved only once.
            if let Some(timer) = self.timer.take() {
                wm.cancel_invoke(&timer);
            }

            // Schedule a persist task
            let timer = wm.invoke_after(DEBOUNCE_LATENCY_MIN..DEBOUNCE_LATENCY_MAX, move |_| {
                log::trace!("The state persistence timer has fired");

                Self::save_persisted_state_with_profile_and_report_error_on_background(
                    profile,
                    &new_persisted,
                );
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

    fn save_persisted_state_with_profile_and_report_error_on_background(
        profile: &'static Profile,
        ps: &PersistedState,
    ) {
        // `PersistedState` might be `!Send`, so serialization must happen
        // on the main thread
        let json = json::to_string(ps);

        // TODO: Mutual exclusion
        nativedispatch::Queue::global_bg().invoke(move || {
            let path = state_path(profile);
            let tmp_path = state_tmp_path(profile);

            log::info!(
                "Writing the state to {:?} using a temporary file at {:?}",
                path,
                tmp_path
            );

            if let Err(e) = write_state_file(&path, &tmp_path, &json) {
                // TODO: Report the error to the user
                log::error!(
                    "Could not write the state to {:?} using a temporary file at {:?}: {}",
                    path,
                    tmp_path,
                    e
                );
            }
        });
    }
}
