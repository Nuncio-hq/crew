//! Native staging ownership receipt. It is deliberately not a generation grant.

use super::recap_adapter::RecapAdapterObservation;
use super::recap_capability::{
    verify_executable, RecapCertificationParts, RecapExecutableIdentity, RecapGuarantees,
    RecapProbeTarget, RecapProcessObservation, RecapRuntimeCertification, RecapRuntimeReadyProof,
    RecapSelection, RecapStateObservation,
};
use super::recap_state::{
    directory_identity, private_read_file, validate_owned_base, DirectoryIdentity,
    RecapStateFailure,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::Manager;

pub(crate) const OWNERSHIP_FILENAME: &str = "crew-staging-ownership-v1.json";
const RUNTIME_READY_FILENAME: &str = "crew-staging-runtime-ready-v1.json";

/// Constructed only from native identity and a fixed private app-data file.
pub(crate) struct VerifiedStagingOwnership {
    app_data: PathBuf,
    document: OwnershipDocument,
    ownership_digest: String,
    native: NativeIdentity,
    generations: Vec<DirectoryIdentity>,
}

pub(super) struct NativeIdentity {
    pub home: PathBuf,
    pub app_data: PathBuf,
    pub config_home: PathBuf,
    pub config_base: PathBuf,
    pub slug: String,
    pub bundle_id: String,
    pub keyring_service: String,
    pub scheme: String,
    pub uid: u32,
}

#[derive(Deserialize)]
struct OwnershipDocument {
    schema: String,
    version: u8,
    environment_id: String,
    status: String,
    mac: MacOwnership,
}

#[derive(Deserialize)]
struct MacOwnership {
    owner_uid: u32,
    home: PathBuf,
    build_demo_slug: String,
    bundle_id: String,
    keyring_service: String,
    deep_link_scheme: String,
    roots: Roots,
    excluded_roots: Vec<PathBuf>,
    auth_references: Vec<serde_json::Value>,
    runtime_generation_allowed: bool,
}

#[derive(Deserialize)]
struct Roots {
    app_data: PathBuf,
    config_home: PathBuf,
    nest: PathBuf,
    profiles: PathBuf,
    workspaces: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeReadyDocument {
    schema: String,
    version: u8,
    environment_id: String,
    ownership_sha256: String,
    status: String,
    owner_uid: u32,
    home: PathBuf,
    app_data: PathBuf,
    bundle_id: String,
    runtime_id: String,
    executable: RuntimeExecutable,
    selection: RuntimeSelection,
    auth_reference: String,
    auth_service: String,
    effective_model: String,
    output_digest: String,
    tool_probe_digest: String,
    guarantees: RuntimeGuarantees,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeExecutable {
    resolved_path: PathBuf,
    version: String,
    fingerprint: String,
    platform: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeSelection {
    model: String,
    profile: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeGuarantees {
    one_shot: bool,
    tool_isolation: bool,
    state_isolation: bool,
    process_containment: bool,
}

impl VerifiedStagingOwnership {
    pub(super) fn from_native(native: NativeIdentity) -> Result<Self, RecapStateFailure> {
        validate_private_root(&native.app_data, native.uid)?;
        let mut bytes = Vec::new();
        let file = private_read_file(&native.app_data.join(OWNERSHIP_FILENAME))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if file
                .metadata()
                .map_err(|_| RecapStateFailure::Ownership)?
                .uid()
                != native.uid
            {
                return Err(RecapStateFailure::Ownership);
            }
        }
        file.take(16 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| RecapStateFailure::Io)?;
        if bytes.len() > 16 * 1024 {
            return Err(RecapStateFailure::Ownership);
        }
        let document = serde_json::from_slice(&bytes).map_err(|_| RecapStateFailure::Ownership)?;
        let mut receipt = Self {
            app_data: native.app_data.clone(),
            document,
            ownership_digest: hex::encode(Sha256::digest(&bytes)),
            native,
            generations: Vec::new(),
        };
        receipt.validate()?;
        receipt.generations = receipt
            .roots()
            .iter()
            .map(|root| directory_identity(root))
            .collect::<Result<_, _>>()?;
        Ok(receipt)
    }

    /// Resolve only an owned disposable-state parent. No runtime/profile launch.
    pub(crate) fn recap_base(&self) -> Result<PathBuf, RecapStateFailure> {
        self.validate()?;
        let base = self.app_data.join("agents");
        match std::fs::symlink_metadata(&base) {
            Ok(_) => {
                validate_owned_base(&base)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(RecapStateFailure::Ownership),
        }
        Ok(base)
    }

    /// Load a separately-issued native runtime grant. The ownership receipt
    /// alone can never mint generation authority; the grant is tied to the
    /// exact ownership bytes, native identity, executable and selection.
    pub(crate) fn runtime_ready_proof(&self) -> Result<RecapRuntimeReadyProof, RecapStateFailure> {
        self.runtime_ready_proof_inner(None)
    }

    /// Load a runtime grant and require its matching positive probe row in the
    /// existing scoped managed-agent retention database.
    pub(crate) fn runtime_ready_proof_with_store(
        &self,
        store: &Connection,
    ) -> Result<RecapRuntimeReadyProof, RecapStateFailure> {
        self.runtime_ready_proof_inner(Some(store))
    }

    fn runtime_ready_proof_inner(
        &self,
        store: Option<&Connection>,
    ) -> Result<RecapRuntimeReadyProof, RecapStateFailure> {
        self.validate()?;
        let ownership = read_private_document(&self.app_data.join(OWNERSHIP_FILENAME), 16 * 1024)?;
        if hex::encode(Sha256::digest(&ownership)) != self.ownership_digest {
            return Err(RecapStateFailure::Ownership);
        }
        let grant_bytes =
            read_private_document(&self.app_data.join(RUNTIME_READY_FILENAME), 16 * 1024)
                .map_err(|_| RecapStateFailure::RuntimeNotReady)?;
        let grant: RuntimeReadyDocument =
            serde_json::from_slice(&grant_bytes).map_err(|_| RecapStateFailure::RuntimeNotReady)?;
        if grant.schema != "crew-staging-runtime-ready"
            || grant.version != 1
            || grant.status != "RUNTIME_READY"
            || grant.environment_id != self.document.environment_id
            || grant.ownership_sha256 != self.ownership_digest
            || grant.owner_uid != self.native.uid
            || grant.home != self.native.home
            || grant.app_data != self.native.app_data
            || grant.bundle_id != self.native.bundle_id
            || grant.effective_model != grant.selection.model
            || !is_sha256(&grant.output_digest)
            || !is_sha256(&grant.tool_probe_digest)
            || !is_sha256(&grant.ownership_sha256)
        {
            return Err(RecapStateFailure::RuntimeNotReady);
        }
        let executable = RecapExecutableIdentity {
            resolved_path: grant.executable.resolved_path,
            version: grant.executable.version,
            fingerprint: grant.executable.fingerprint,
            platform: grant.executable.platform,
        };
        let executable =
            verify_executable(&executable).map_err(|_| RecapStateFailure::RuntimeNotReady)?;
        let selection = RecapSelection {
            model: grant.selection.model,
            profile: grant.selection.profile,
            auth_available: true,
        };
        let auth_service = grant.auth_service;
        let guarantees = RecapGuarantees {
            one_shot: grant.guarantees.one_shot,
            tool_isolation: grant.guarantees.tool_isolation,
            state_isolation: grant.guarantees.state_isolation,
            process_containment: grant.guarantees.process_containment,
        };
        if let Some(store) = store {
            let Some(certification) = super::retention::get_recap_certification(
                store,
                &grant.runtime_id,
                &executable.fingerprint,
                &executable.version,
                &executable.platform,
            )
            .map_err(|_| RecapStateFailure::RuntimeNotReady)?
            else {
                return Err(RecapStateFailure::RuntimeNotReady);
            };
            let parts = RecapCertificationParts {
                runtime_id: grant.runtime_id.clone(),
                executable: executable.clone(),
                selection: selection.clone(),
                auth: super::recap_capability::RecapAuthBinding {
                    service: auth_service.clone(),
                    reference: grant.auth_reference.clone(),
                },
                guarantees,
                effective_model: grant.effective_model.clone(),
                output_digest: grant.output_digest.clone(),
                tool_probe_digest: grant.tool_probe_digest.clone(),
            };
            if !certification.matches(&parts, &self.ownership_digest) {
                return Err(RecapStateFailure::RuntimeNotReady);
            }
        }
        RecapRuntimeReadyProof::from_grant(
            grant.runtime_id,
            executable,
            selection,
            auth_service,
            grant.auth_reference,
            guarantees,
        )
        .map_err(|_| RecapStateFailure::RuntimeNotReady)
    }

    /// Persist a positive certification and project the existing strict
    /// runtime-ready grant. This is a native-only producer: the certificate
    /// can only be created from a typed bounded probe, and the store must be
    /// the already-selected managed-agent retention scope.
    pub(crate) fn issue_runtime_ready_grant(
        &self,
        store: &Connection,
        certification: &RecapRuntimeCertification,
        certified_at: u64,
    ) -> Result<(), RecapStateFailure> {
        self.validate()?;
        let ownership = read_private_document(&self.app_data.join(OWNERSHIP_FILENAME), 16 * 1024)?;
        if hex::encode(Sha256::digest(&ownership)) != self.ownership_digest {
            return Err(RecapStateFailure::Ownership);
        }
        let parts = certification.parts();
        self.validate_certification(&parts)?;
        super::retention::persist_recap_certification(
            store,
            &parts,
            &self.ownership_digest,
            certified_at,
        )
        .map_err(|_| RecapStateFailure::Io)?;

        let grant = RuntimeReadyDocument {
            schema: "crew-staging-runtime-ready".to_string(),
            version: 1,
            environment_id: self.document.environment_id.clone(),
            ownership_sha256: self.ownership_digest.clone(),
            status: "RUNTIME_READY".to_string(),
            owner_uid: self.native.uid,
            home: self.native.home.clone(),
            app_data: self.native.app_data.clone(),
            bundle_id: self.native.bundle_id.clone(),
            runtime_id: parts.runtime_id,
            executable: RuntimeExecutable {
                resolved_path: parts.executable.resolved_path,
                version: parts.executable.version,
                fingerprint: parts.executable.fingerprint,
                platform: parts.executable.platform,
            },
            selection: RuntimeSelection {
                model: parts.selection.model,
                profile: parts.selection.profile,
            },
            auth_reference: parts.auth.reference,
            auth_service: parts.auth.service,
            effective_model: parts.effective_model,
            output_digest: parts.output_digest,
            tool_probe_digest: parts.tool_probe_digest,
            guarantees: RuntimeGuarantees {
                one_shot: parts.guarantees.one_shot,
                tool_isolation: parts.guarantees.tool_isolation,
                state_isolation: parts.guarantees.state_isolation,
                process_containment: parts.guarantees.process_containment,
            },
        };
        let bytes = serde_json::to_vec(&grant).map_err(|_| RecapStateFailure::Io)?;
        atomic_write_runtime_grant(&self.app_data, &bytes)
    }

    fn validate_certification(
        &self,
        parts: &RecapCertificationParts,
    ) -> Result<(), RecapStateFailure> {
        let Some(runtime) = super::known_acp_runtime_exact(&parts.runtime_id) else {
            return Err(RecapStateFailure::RuntimeNotReady);
        };
        let contract = runtime.recap_contract();
        if contract.command.is_none() {
            return Err(RecapStateFailure::RuntimeNotReady);
        }
        if !parts.guarantees.one_shot
            || !parts.guarantees.tool_isolation
            || !parts.guarantees.state_isolation
            || !parts.guarantees.process_containment
        {
            return Err(RecapStateFailure::RuntimeNotReady);
        }
        if !parts.selection.auth_available
            || !is_sha256(&parts.output_digest)
            || !is_sha256(&parts.tool_probe_digest)
        {
            return Err(RecapStateFailure::RuntimeNotReady);
        }
        let executable =
            verify_executable(&parts.executable).map_err(|_| RecapStateFailure::RuntimeNotReady)?;
        if executable != parts.executable {
            return Err(RecapStateFailure::RuntimeNotReady);
        }
        if parts.auth.service != self.native.keyring_service
            || parts.auth.reference.is_empty()
            || parts.auth.reference != parts.auth.reference.trim()
            || parts.auth.reference.len() > 256
            || parts.auth.reference.chars().any(char::is_control)
        {
            return Err(RecapStateFailure::RuntimeNotReady);
        }
        if parts.effective_model != parts.selection.model
            || parts.effective_model.is_empty()
            || parts.effective_model != parts.effective_model.trim()
            || parts.effective_model.chars().any(char::is_control)
        {
            return Err(RecapStateFailure::RuntimeNotReady);
        }
        match contract.selection {
            super::recap_capability::RecapSelectionContract::ExplicitModel => {
                if parts.selection.profile.is_some() {
                    return Err(RecapStateFailure::RuntimeNotReady);
                }
            }
            super::recap_capability::RecapSelectionContract::StagingProfile => {
                let Some(profile) = parts.selection.profile.as_deref() else {
                    return Err(RecapStateFailure::RuntimeNotReady);
                };
                let Some(name) = super::recap_adapter::hermes_profile_ref(profile) else {
                    return Err(RecapStateFailure::RuntimeNotReady);
                };
                if name == super::hermes_profile::HERMES_HOME_PROFILE_NAME
                    || !profile.is_absolute()
                    || profile.canonicalize().ok().as_deref() != Some(profile)
                    || !profile.starts_with(&self.document.mac.roots.profiles)
                {
                    return Err(RecapStateFailure::RuntimeNotReady);
                }
                if super::hermes_profile::validate_hermes_profile_name(&name).is_err() {
                    return Err(RecapStateFailure::RuntimeNotReady);
                }
                directory_identity(profile).map_err(|_| RecapStateFailure::RuntimeNotReady)?;
            }
        }
        Ok(())
    }

    fn roots(&self) -> [&PathBuf; 5] {
        let roots = &self.document.mac.roots;
        [
            &roots.app_data,
            &roots.config_home,
            &roots.nest,
            &roots.profiles,
            &roots.workspaces,
        ]
    }

    fn validate(&self) -> Result<(), RecapStateFailure> {
        let doc = &self.document;
        let mac = &doc.mac;
        let native = &self.native;
        if doc.schema != "crew-staging-ownership"
            || doc.version != 1
            || doc.status != "OWNERSHIP_ONLY_NOT_RUNTIME_READY"
            || doc.environment_id != format!("crew-{}", native.slug)
            || native.slug.is_empty()
            || !native
                .slug
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || mac.runtime_generation_allowed
            || !mac.auth_references.is_empty()
            || mac.owner_uid != native.uid
            || mac.home != native.home
            || mac.build_demo_slug != native.slug
            || mac.bundle_id != native.bundle_id
            || mac.keyring_service != native.keyring_service
            || mac.deep_link_scheme != native.scheme
            || native.bundle_id != format!("com.nuncio.crew.{}", native.slug)
        {
            return Err(RecapStateFailure::Ownership);
        }
        let roots = &mac.roots;
        for (root, generation) in self.roots().iter().zip(&self.generations) {
            if &directory_identity(root)? != generation {
                return Err(RecapStateFailure::Ownership);
            }
        }
        let expected = [
            native.app_data.clone(),
            native.config_home.clone(),
            native.home.join(format!(".buzz-demo-{}", native.slug)),
            native.home.join(&doc.environment_id).join("profiles"),
            native.home.join(&doc.environment_id).join("workspaces"),
        ];
        let observed = [
            &roots.app_data,
            &roots.config_home,
            &roots.nest,
            &roots.profiles,
            &roots.workspaces,
        ];
        for (root, expected) in observed.iter().zip(&expected) {
            if *root != expected {
                return Err(RecapStateFailure::Ownership);
            }
            validate_private_root(root, native.uid)?;
        }
        let required = [
            native.home.join(".buzz"),
            native.home.join(".buzz-dev"),
            native.home.join(".codex"),
            native.home.join(".claude"),
            native.home.join(".hermes"),
            native.config_base.join("com.nuncio.crew"),
        ];
        if required
            .iter()
            .any(|root| !mac.excluded_roots.contains(root))
        {
            return Err(RecapStateFailure::Ownership);
        }
        for excluded in &mac.excluded_roots {
            if !excluded.is_absolute()
                || excluded
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                || observed
                    .iter()
                    .any(|root| root.starts_with(excluded) || excluded.starts_with(root))
            {
                return Err(RecapStateFailure::Ownership);
            }
        }
        Ok(())
    }
}

/// Load the native grant and its matching row from the active scoped
/// managed-agent retention database. Production consumers use this helper so
/// a grant from another relay/owner scope cannot become an admission proof.
pub(crate) fn runtime_ready_proof_for_app<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<RecapRuntimeReadyProof, RecapStateFailure> {
    let ownership = VerifiedStagingOwnership::load(app)?;
    runtime_ready_proof_for_ownership(app, &ownership)
}

/// Load a runtime grant against the active owner/relay retention scope for an
/// already verified ownership receipt. Callers that also need the receipt's
/// disposable-state base can avoid reloading the private document by using
/// this helper after [`VerifiedStagingOwnership::load`].
pub(crate) fn runtime_ready_proof_for_ownership<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    ownership: &VerifiedStagingOwnership,
) -> Result<RecapRuntimeReadyProof, RecapStateFailure> {
    use tauri::Manager;

    let state = app.state::<crate::app_state::AppState>();
    let scope = super::retention::active_retention_scope(app, &state)
        .map_err(|_| RecapStateFailure::RuntimeNotReady)?;
    let store = super::retention::open_retention_db(&scope.db_path)
        .map_err(|_| RecapStateFailure::RuntimeNotReady)?;
    ownership.runtime_ready_proof_with_store(&store)
}

/// Consume a typed native adapter observation through the active catalog and
/// retention scope. The adapter creates the observation by parsing its bounded
/// probe envelope; this native entrypoint then creates the opaque certification
/// and projects the strict runtime-ready grant. Renderer settings and catalog
/// discovery cannot provide the individual probe facts.
pub(crate) fn certify_runtime_probe_for_app<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    target: RecapProbeTarget,
    adapter: RecapAdapterObservation,
    state: RecapStateObservation,
    process: RecapProcessObservation,
    certified_at: u64,
) -> Result<(), RecapStateFailure> {
    let certification =
        RecapRuntimeCertification::from_adapter_observation(target, adapter, state, process)
            .map_err(|_| RecapStateFailure::RuntimeNotReady)?;
    let ownership = VerifiedStagingOwnership::load(app)?;
    let state = app.state::<crate::app_state::AppState>();
    let scope = super::retention::active_retention_scope(app, &state)
        .map_err(|_| RecapStateFailure::RuntimeNotReady)?;
    let store = super::retention::open_retention_db(&scope.db_path)
        .map_err(|_| RecapStateFailure::RuntimeNotReady)?;
    ownership.issue_runtime_ready_grant(&store, &certification, certified_at)
}

fn atomic_write_runtime_grant(app_data: &Path, bytes: &[u8]) -> Result<(), RecapStateFailure> {
    #[cfg(not(unix))]
    {
        let _ = (app_data, bytes);
        return Err(RecapStateFailure::UnsupportedPlatform);
    }
    if bytes.len() > 16 * 1024 {
        return Err(RecapStateFailure::Io);
    }
    validate_private_root(app_data, rustix::process::geteuid().as_raw())?;
    let temporary = app_data.join(format!(".crew-runtime-ready-{}.tmp", uuid::Uuid::new_v4()));
    #[cfg(unix)]
    let mut file = {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| RecapStateFailure::Io)?
    };
    use std::io::Write;
    if file.write_all(bytes).is_err() || file.sync_all().is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(RecapStateFailure::Io);
    }
    drop(file);
    if validate_private_root(app_data, rustix::process::geteuid().as_raw()).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(RecapStateFailure::Ownership);
    }
    if std::fs::rename(&temporary, app_data.join(RUNTIME_READY_FILENAME)).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(RecapStateFailure::Io);
    }
    std::fs::File::open(app_data)
        .and_then(|file| file.sync_all())
        .map_err(|_| RecapStateFailure::Io)
}

fn read_private_document(path: &Path, limit: usize) -> Result<Vec<u8>, RecapStateFailure> {
    let mut bytes = Vec::new();
    private_read_file(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| RecapStateFailure::Ownership)?;
    if bytes.len() > limit {
        return Err(RecapStateFailure::Ownership);
    }
    Ok(bytes)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_private_root(path: &Path, uid: u32) -> Result<(), RecapStateFailure> {
    let identity = directory_identity(path)?;
    if identity.owner != uid
        || path
            .canonicalize()
            .map_err(|_| RecapStateFailure::Ownership)?
            != path
    {
        return Err(RecapStateFailure::Ownership);
    }
    Ok(())
}

#[cfg(test)]
#[path = "recap_ownership/tests.rs"]
mod tests;
