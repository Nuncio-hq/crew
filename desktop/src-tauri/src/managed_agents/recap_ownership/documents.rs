//! Serde documents for the private staging ownership manifest and the
//! runtime-ready grant stored beside it.
use super::super::recap_capability::RecapProfileIdentity;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize)]
pub(super) struct OwnershipDocument {
    pub(super) schema: String,
    pub(super) version: u8,
    pub(super) environment_id: String,
    pub(super) status: String,
    pub(super) mac: MacOwnership,
}

#[derive(Deserialize)]
pub(super) struct MacOwnership {
    pub(super) owner_uid: u32,
    pub(super) home: PathBuf,
    pub(super) build_demo_slug: String,
    pub(super) bundle_id: String,
    pub(super) keyring_service: String,
    pub(super) deep_link_scheme: String,
    pub(super) roots: Roots,
    pub(super) excluded_roots: Vec<PathBuf>,
    pub(super) auth_references: Vec<serde_json::Value>,
    pub(super) runtime_generation_allowed: bool,
}

#[derive(Deserialize)]
pub(super) struct Roots {
    pub(super) app_data: PathBuf,
    pub(super) config_home: PathBuf,
    pub(super) nest: PathBuf,
    pub(super) profiles: PathBuf,
    pub(super) workspaces: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeReadyDocument {
    pub(super) schema: String,
    pub(super) version: u8,
    pub(super) environment_id: String,
    pub(super) ownership_sha256: String,
    pub(super) status: String,
    pub(super) owner_uid: u32,
    pub(super) home: PathBuf,
    pub(super) app_data: PathBuf,
    pub(super) bundle_id: String,
    pub(super) runtime_id: String,
    pub(super) executable: RuntimeExecutable,
    pub(super) selection: RuntimeSelection,
    pub(super) auth_reference: String,
    pub(super) auth_service: String,
    pub(super) effective_model: String,
    pub(super) output_digest: String,
    pub(super) tool_probe_digest: String,
    pub(super) guarantees: RuntimeGuarantees,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeExecutable {
    pub(super) resolved_path: PathBuf,
    pub(super) version: String,
    pub(super) fingerprint: String,
    pub(super) platform: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeSelection {
    pub(super) model: String,
    pub(super) profile: Option<PathBuf>,
    pub(super) profile_digest: Option<String>,
    pub(super) profile_identity: Option<RecapProfileIdentity>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeGuarantees {
    pub(super) one_shot: bool,
    pub(super) tool_isolation: bool,
    pub(super) state_isolation: bool,
    pub(super) process_containment: bool,
}
