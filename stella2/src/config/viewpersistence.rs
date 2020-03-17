use harmony::{set_field, Elem, ShallowEq};
use miniserde::{json, Deserialize, Serialize};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
use tcw3::pal::{prelude::*, HInvoke, MtLock, Wm};

use super::profile::Profile;
use crate::model;

/// State updates are persisted to disk after waiting for
/// at least this duration.
const DEBOUNCE_LATENCY_MIN: Duration = Duration::from_secs(5);
const DEBOUNCE_LATENCY_MAX: Duration = Duration::from_secs(20);

/// The projection of an app state to be persisted to disk.
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

/// Load `PersistedState` from the specified path.
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

/// Write a file atomically.
fn write_atomically(path: &Path, tmp_path: &Path, contents: &str) -> Result<(), std::io::Error> {
    // Use a temporary file `tmp_path` to atomically update `patH`.
    std::fs::write(tmp_path, contents)?;

    // Move `tmp_path` to `path`, overwriting it. It's okay to leave
    // `tmp_path` on failure.
    std::fs::rename(tmp_path, path)
}

/// Schedules the asynchronous persistence operations of an app state.
pub struct PersistenceScheduler {
    persisted: Cell<Option<Elem<PersistedState>>>,
    shared: Arc<PersistenceSchedulerShared>,
}

impl PersistenceScheduler {
    /// Construct a `PersistenceScheduler` with an initial app state, which it
    /// will *not* persist to disk.
    pub fn new(app_state: &model::AppState) -> Self {
        Self {
            persisted: Cell::new(Some(PersistedState::new(app_state))),
            shared: Arc::new(PersistenceSchedulerShared {
                timer: MtLock::new(Cell::new(None)),
                mt_gen: MtLock::new(Cell::new(0)),
                persistent_req_gen: MtLock::new(Cell::new(0)),
                persistent_gen: Mutex::new(0),
                persistent_gen_cv: Condvar::new(),
            }),
        }
    }

    /// Persist the current app state to disk immediately. Blocks the current
    /// thread until the operation is complete.
    pub fn flush(&self, wm: Wm, app_state: &model::AppState, profile: &'static Profile) {
        let mut persisted = self.persisted.take().unwrap();

        if let Some(new_persisted) = PersistedState::merge_from_app(&persisted, app_state) {
            persisted = new_persisted;
        } else {
            // There might be already an active timer waiting to persist
            // `self.persisted`, but we don't want to wait for it to fire, so
            // create a new generation anyway
        }

        PersistenceSchedulerShared::commit_new_generation_blocking(
            wm,
            Arc::clone(&self.shared),
            profile,
            persisted.clone(),
        );

        // Put it back
        self.persisted.set(Some(persisted));
    }

    /// Schedule the persistence operation of the current app state.
    pub fn handle_update(&self, wm: Wm, app_state: &model::AppState, profile: &'static Profile) {
        // Temporarily take `persisted`
        let mut persisted = self.persisted.take();

        if let Some(new_persisted) =
            PersistedState::merge_from_app(persisted.as_ref().unwrap(), app_state)
        {
            persisted = Some(new_persisted.clone());

            PersistenceSchedulerShared::commit_new_generation_lazily(
                wm,
                Arc::clone(&self.shared),
                profile,
                new_persisted,
            );
        }

        // Put it back
        self.persisted.set(persisted);
    }
}

/// Shared by (1) `PersistenceScheduler`, (2) the timer handlers, and (3) the
/// working thread where I/O takes place
struct PersistenceSchedulerShared {
    timer: MtLock<Cell<Option<HInvoke>>>,
    /// The generation of `PersistenceScheduler::persisted` (which is owned by
    /// a main thread, hence "MT")
    mt_gen: MtLock<Cell<Gen>>,
    /// The latest generation sent to a worker thread.
    persistent_req_gen: MtLock<Cell<Gen>>,
    /// The latest generation processed by a worker thread.
    persistent_gen: Mutex<Gen>,
    persistent_gen_cv: Condvar,
}

/// Generation
type Gen = u64;

impl PersistenceSchedulerShared {
    /// Commit a new generation and block the current thread until it's
    /// persisted.
    fn commit_new_generation_blocking(
        wm: Wm,
        this: Arc<Self>,
        profile: &'static Profile,
        ps: Elem<PersistedState>,
    ) {
        // The `PersistedState` owned by the timer handler is supposed to be of
        // the latest generation, so the timer handler must be dropped as we
        // commit a new generation.
        if let Some(timer) = this.timer.get_with_wm(wm).take() {
            wm.cancel_invoke(&timer);
        }

        // Increment generation
        let mt_gen = this.mt_gen.get_with_wm(wm);
        mt_gen.set(mt_gen.get() + 1);

        log::trace!("Commited the generation {:?}", mt_gen.get());

        // Wait until the previous operation is done
        let last_requested_gen = this.persistent_req_gen.get_with_wm(wm).get();
        log::debug!(
            "Waiting for the previous persistence operation (gen {:?}) to complete...",
            last_requested_gen
        );
        this.block_until_gen_persisted(last_requested_gen);

        Self::start_persist_latest_gen(wm, Arc::clone(&this), profile, &ps);

        // Wait until this generation (`latest_gen`) is persisted
        let latest_gen = this.mt_gen.get_with_wm(wm).get();
        log::debug!(
            "Waiting for the current presistence operation (gen {:?}) to complete...",
            latest_gen
        );
        this.block_until_gen_persisted(latest_gen);
    }

    /// Commit a new generation, persist later.
    fn commit_new_generation_lazily(
        wm: Wm,
        this: Arc<Self>,
        profile: &'static Profile,
        ps: Elem<PersistedState>,
    ) {
        // Cancel the previous invocation so that if the state was updated
        // for many times within a short time, the state is saved only once.
        // Also, the `PersistedState` owned by the timer handler must be of
        // the latest generation, so the timer handler must be dropped as we
        // commit a new generation.
        if let Some(timer) = this.timer.get_with_wm(wm).take() {
            wm.cancel_invoke(&timer);
        }

        // Increment generation
        let mt_gen = this.mt_gen.get_with_wm(wm);
        mt_gen.set(mt_gen.get() + 1);

        log::trace!("Commited the generation {:?}", mt_gen.get());

        Self::schedule_persist_timer(wm, this, profile, ps);
    }

    /// Schedule `Self::persist_timer_handler` to run later.
    fn schedule_persist_timer(
        wm: Wm,
        this: Arc<Self>,
        profile: &'static Profile,
        ps: Elem<PersistedState>,
    ) {
        // Schedule a persist task
        let this2 = Arc::clone(&this);
        let timer = wm.invoke_after(DEBOUNCE_LATENCY_MIN..DEBOUNCE_LATENCY_MAX, move |_| {
            PersistenceSchedulerShared::persist_timer_handler(wm, this2, profile, ps);
        });

        log::trace!(
            "Scheduled the state persistence timer ({:?}) to run in {:?}",
            timer,
            DEBOUNCE_LATENCY_MIN..DEBOUNCE_LATENCY_MAX
        );

        this.timer.get_with_wm(wm).set(Some(timer));
    }

    fn persist_timer_handler(
        wm: Wm,
        this: Arc<Self>,
        profile: &'static Profile,
        ps: Elem<PersistedState>,
    ) {
        log::trace!("The state persistence timer has fired");

        // Wait again if the previous operation is still in progress
        let last_requested_gen = this.persistent_req_gen.get_with_wm(wm).get();
        if !this.is_gen_persisted(last_requested_gen) {
            log::trace!(
                "A previous persistence operation (gen {:?}) is still in progress, waiting again",
                last_requested_gen
            );

            return Self::schedule_persist_timer(wm, this, profile, ps);
        }

        Self::start_persist_latest_gen(wm, this, profile, &ps);
    }

    /// Start a worker thread to persist the latest state (`mt_gen`).
    ///
    /// `ps` must pertain to `mt_gen`. There must not be an ongoing persistence
    /// operation.
    fn start_persist_latest_gen(
        wm: Wm,
        this: Arc<Self>,
        profile: &'static Profile,
        ps: &PersistedState,
    ) {
        // `PersistedState` might be `!Send`, so serialization must happen
        // on the main thread
        let json = json::to_string(ps);

        // There must not be an ongoing persistence operation.
        debug_assert!(this.is_gen_persisted(this.persistent_req_gen.get_with_wm(wm).get()));

        let gen = this.mt_gen.get_with_wm(wm).get();
        this.persistent_req_gen.get_with_wm(wm).set(gen);

        // Do the I/O in a worker thread
        nativedispatch::Queue::global_bg().invoke(move || {
            let path = state_path(profile);
            let tmp_path = state_tmp_path(profile);

            log::info!(
                "Writing the state (gen {:?}) to {:?} using a temporary file at {:?}",
                gen,
                path,
                tmp_path
            );

            if let Err(e) = write_atomically(&path, &tmp_path, &json) {
                // TODO: Report the error to the user
                log::error!(
                    "Could not write the state to {:?} using a temporary file at {:?}: {}",
                    path,
                    tmp_path,
                    e
                );
            }

            log::debug!("Wrote the state (gen {:?}) to {:?}", gen, path);

            // Log the latest generation persisted
            let mut persistent_gen = this.persistent_gen.lock().unwrap();
            debug_assert!(*persistent_gen < gen);
            *persistent_gen = gen;

            this.persistent_gen_cv.notify_all();
        });
    }

    /// Block the current thread until the specified generation is persisted.
    fn block_until_gen_persisted(&self, gen: Gen) {
        let mut persistent_gen = self.persistent_gen.lock().unwrap();
        while *persistent_gen < gen {
            persistent_gen = self.persistent_gen_cv.wait(persistent_gen).unwrap();
        }
        drop(persistent_gen);
        debug_assert!(self.is_gen_persisted(gen));
    }

    /// Return `true` if `gen` or a newer generation has already been persisted.
    fn is_gen_persisted(&self, gen: Gen) -> bool {
        *self.persistent_gen.lock().unwrap() >= gen
    }
}
