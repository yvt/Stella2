use fslock::LockFile;
use std::path::PathBuf;

use super::profile::Profile;

pub struct LockGuard {
    _inner: LockFile,
}

fn lockfile_path(profile: &Profile) -> PathBuf {
    profile.data_dir().join("lock")
}

/// Create a file in the specified profile directory and attempt to lock it
/// to ensure only one instance of the application has access to the profile.
pub fn try_lock(profile: &Profile) -> Result<Option<LockGuard>, fslock::Error> {
    let path = lockfile_path(profile);
    log::info!("Locking {:?}", path);
    let mut file = LockFile::open(&path)?;

    if file.try_lock()? {
        Ok(Some(LockGuard { _inner: file }))
    } else {
        log::warn!(
            "Could not lock {:?} - another instance may be already running",
            path
        );
        Ok(None)
    }
}
