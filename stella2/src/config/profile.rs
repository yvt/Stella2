use std::{
    io::Error,
    path::{Path, PathBuf},
};

/// Represents the location where a set of configuration and/or persistent data
/// is stored.
#[derive(Debug)]
pub struct Profile {
    data_dir: PathBuf,
}

/// The directory name used for application-specific directories.
const APP_DIR_NAME: &str = "Stella2";

impl Default for Profile {
    fn default() -> Self {
        let portable_profile_dir = portable_profile_dir();

        // Detect portable installation
        if let Some(portable_profile_dir) = &portable_profile_dir {
            if portable_profile_dir.is_dir() {
                log::info!(
                    "Found a portable profile directory at {:?}, using this \
                    as the default profile",
                    portable_profile_dir,
                );
                return Profile::from_portable_dir(portable_profile_dir);
            } else {
                log::debug!(
                    "The portable profile directory was not found at {:?}",
                    portable_profile_dir,
                );
            }
        }

        if let Some(sys_profile) = Profile::from_sys_dirs() {
            log::debug!("Setting up the default profile using standard directories");
            return sys_profile;
        } else {
            log::warn!("Could not detect standard directories");
        }

        // Fall back to a portable installation
        if let Some(portable_profile_dir) = &portable_profile_dir {
            log::info!("Creating a portable profile at {:?}", portable_profile_dir);
            Profile::from_portable_dir(portable_profile_dir)
        } else {
            // This is a very unlikely situation, so abort the application
            panic!("Could not determine the default profile directory");
        }
    }
}

fn portable_profile_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("Profile"))
}

impl Profile {
    /// Construct a `Profile` based on standard system directories.
    ///
    ///  - `Some(_)`: The operation is successful.
    ///  - `None`: Could not detect the standard system directories.
    fn from_sys_dirs() -> Option<Self> {
        Some(Self {
            data_dir: dirs::data_dir()?.join(APP_DIR_NAME),
        })
    }

    fn from_portable_dir(d: &Path) -> Self {
        Self {
            data_dir: d.join("Data"),
        }
    }

    /// Ensure the profile directory exists.
    pub fn prepare(&self) -> Result<(), Error> {
        log::debug!("Creating the directory {:?}", self.data_dir);
        std::fs::create_dir_all(&self.data_dir)?;

        Ok(())
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}
