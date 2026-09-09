#[cfg(unix)]
use super::recap_ownership::NativeIdentity;
use super::recap_ownership::VerifiedStagingOwnership;
use crate::managed_agents::recap_state::RecapStateFailure;
use tauri::AppHandle;
#[cfg(unix)]
use tauri::Manager;

impl VerifiedStagingOwnership {
    /// Load the fixed native app-data manifest; accepts no frontend identity,
    /// path, hash, profile, credential reference or generation permission.
    pub(crate) fn load<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<Self, RecapStateFailure> {
        #[cfg(unix)]
        {
            let uid = rustix::process::getuid();
            if uid != rustix::process::geteuid() {
                return Err(RecapStateFailure::Ownership);
            }
            let slug = crate::build_identity::demo_slug().ok_or(RecapStateFailure::Ownership)?;
            let home = dirs::home_dir().ok_or(RecapStateFailure::Ownership)?;
            let config_base = dirs::config_dir().ok_or(RecapStateFailure::Ownership)?;
            let config_home = crate::build_identity::demo_config_home()
                .map_err(|_| RecapStateFailure::Ownership)?
                .ok_or(RecapStateFailure::Ownership)?;
            Self::from_native(NativeIdentity {
                home,
                app_data: app
                    .path()
                    .app_data_dir()
                    .map_err(|_| RecapStateFailure::Ownership)?,
                config_home,
                config_base,
                slug: slug.to_owned(),
                bundle_id: app.config().identifier.clone(),
                keyring_service: crate::build_identity::keyring_service().into_owned(),
                scheme: crate::build_identity::deep_link_scheme().into_owned(),
                uid: uid.as_raw(),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = app;
            Err(RecapStateFailure::UnsupportedPlatform)
        }
    }
}
