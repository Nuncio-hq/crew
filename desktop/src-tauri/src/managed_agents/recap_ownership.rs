//! Native staging ownership receipt. It is deliberately not a generation grant.

use super::recap_capability::{
    verify_executable, RecapExecutableIdentity, RecapGuarantees, RecapRuntimeReadyProof,
    RecapSelection,
};
use super::recap_state::{
    directory_identity, private_read_file, validate_owned_base, DirectoryIdentity,
    RecapStateFailure,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

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

#[derive(Deserialize)]
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
    guarantees: RuntimeGuarantees,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeExecutable {
    resolved_path: PathBuf,
    version: String,
    fingerprint: String,
    platform: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeSelection {
    model: String,
    profile: Option<PathBuf>,
}

#[derive(Deserialize)]
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
        RecapRuntimeReadyProof::from_grant(
            grant.runtime_id,
            executable,
            selection,
            grant.auth_reference,
            RecapGuarantees {
                one_shot: grant.guarantees.one_shot,
                tool_isolation: grant.guarantees.tool_isolation,
                state_isolation: grant.guarantees.state_isolation,
                process_containment: grant.guarantees.process_containment,
            },
        )
        .map_err(|_| RecapStateFailure::RuntimeNotReady)
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
