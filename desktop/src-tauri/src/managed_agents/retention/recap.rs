//! Durable native recap-certification rows in the existing retention store.
//!
//! The table is deliberately part of the managed-agent retention database;
//! it is not a second runtime registry. Rows are addressed by the exact
//! runtime executable identity (including version, fingerprint and platform)
//! and contain only redacted probe evidence.

use rusqlite::{params, Connection, OptionalExtension};

use super::super::recap_capability::RecapCertificationParts;

/// A positive probe row loaded from the scoped managed-agent retention DB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredRecapCertification {
    pub(crate) runtime_id: String,
    pub(crate) executable_path: String,
    pub(crate) executable_version: String,
    pub(crate) executable_fingerprint: String,
    pub(crate) platform: String,
    pub(crate) model: String,
    pub(crate) profile: Option<String>,
    pub(crate) profile_digest: Option<String>,
    pub(crate) profile_identity: Option<String>,
    pub(crate) auth_service: String,
    pub(crate) auth_reference: String,
    pub(crate) effective_model: String,
    pub(crate) output_digest: String,
    pub(crate) tool_probe_digest: String,
    pub(crate) ownership_sha256: String,
    pub(crate) one_shot: bool,
    pub(crate) tool_isolation: bool,
    pub(crate) state_isolation: bool,
    pub(crate) process_containment: bool,
    pub(crate) certified_at: u64,
}

/// SQL for the one native recap-certification table. `open_retention_db`
/// installs it alongside `persona_events` for every existing scoped DB.
pub(crate) const RECAP_CERTIFICATION_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS recap_runtime_certifications (
    runtime_id TEXT NOT NULL,
    executable_path TEXT NOT NULL,
    executable_version TEXT NOT NULL,
    executable_fingerprint TEXT NOT NULL,
    platform TEXT NOT NULL,
    model TEXT NOT NULL,
    profile TEXT,
    profile_digest TEXT,
    profile_identity TEXT,
    auth_service TEXT NOT NULL,
    auth_reference TEXT NOT NULL,
    effective_model TEXT NOT NULL,
    output_digest TEXT NOT NULL,
    tool_probe_digest TEXT NOT NULL,
    ownership_sha256 TEXT NOT NULL,
    one_shot INTEGER NOT NULL,
    tool_isolation INTEGER NOT NULL,
    state_isolation INTEGER NOT NULL,
    process_containment INTEGER NOT NULL,
    certified_at INTEGER NOT NULL,
    PRIMARY KEY (runtime_id, executable_fingerprint, executable_version, platform)
);
"#;

/// Persist a completed bounded probe. Raw output is never written; only its
/// digest and the typed evidence carried by the opaque certification survive.
pub(crate) fn persist_recap_certification(
    conn: &Connection,
    certification: &RecapCertificationParts,
    ownership_sha256: &str,
    certified_at: u64,
) -> Result<(), String> {
    if ownership_sha256.len() != 64
        || !ownership_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("invalid recap ownership digest".to_string());
    }
    let profile = certification
        .selection
        .profile
        .as_deref()
        .map(|path| path.to_string_lossy().into_owned());
    let profile_identity = certification
        .selection
        .profile_identity
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| "failed to encode recap profile identity".to_string())?;
    conn.execute(
        "INSERT INTO recap_runtime_certifications (
            runtime_id, executable_path, executable_version, executable_fingerprint,
            platform, model, profile, profile_digest, profile_identity, auth_service, auth_reference, effective_model,
            output_digest, tool_probe_digest, ownership_sha256, one_shot, tool_isolation, state_isolation,
            process_containment, certified_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
         ON CONFLICT(runtime_id, executable_fingerprint, executable_version, platform)
         DO UPDATE SET
            executable_path = excluded.executable_path,
            model = excluded.model,
            profile = excluded.profile,
            profile_digest = excluded.profile_digest,
            profile_identity = excluded.profile_identity,
            auth_service = excluded.auth_service,
            auth_reference = excluded.auth_reference,
            effective_model = excluded.effective_model,
            output_digest = excluded.output_digest,
            tool_probe_digest = excluded.tool_probe_digest,
            ownership_sha256 = excluded.ownership_sha256,
            one_shot = excluded.one_shot,
            tool_isolation = excluded.tool_isolation,
            state_isolation = excluded.state_isolation,
            process_containment = excluded.process_containment,
            certified_at = excluded.certified_at",
        params![
            certification.runtime_id,
            certification.executable.resolved_path.to_string_lossy(),
            certification.executable.version,
            certification.executable.fingerprint,
            certification.executable.platform,
            certification.selection.model,
            profile,
            certification.selection.profile_digest,
            profile_identity,
            certification.auth.service,
            certification.auth.reference,
            certification.effective_model,
            certification.output_digest,
            certification.tool_probe_digest,
            ownership_sha256,
            certification.guarantees.one_shot as i32,
            certification.guarantees.tool_isolation as i32,
            certification.guarantees.state_isolation as i32,
            certification.guarantees.process_containment as i32,
            certified_at,
        ],
    )
    .map_err(|error| format!("failed to persist recap certification: {error}"))?;
    Ok(())
}

/// Read the exact positive row used to validate a native grant.
pub(crate) fn get_recap_certification(
    conn: &Connection,
    runtime_id: &str,
    executable_fingerprint: &str,
    executable_version: &str,
    platform: &str,
) -> Result<Option<StoredRecapCertification>, String> {
    conn.query_row(
        "SELECT runtime_id, executable_path, executable_version,
                executable_fingerprint, platform, model, profile, profile_digest, profile_identity, auth_service,
                auth_reference, effective_model, output_digest, tool_probe_digest, ownership_sha256,
                one_shot, tool_isolation, state_isolation, process_containment,
                certified_at
         FROM recap_runtime_certifications
         WHERE runtime_id = ?1 AND executable_fingerprint = ?2
           AND executable_version = ?3 AND platform = ?4",
        params![
            runtime_id,
            executable_fingerprint,
            executable_version,
            platform
        ],
        |row| {
            Ok(StoredRecapCertification {
                runtime_id: row.get(0)?,
                executable_path: row.get(1)?,
                executable_version: row.get(2)?,
                executable_fingerprint: row.get(3)?,
                platform: row.get(4)?,
                model: row.get(5)?,
                profile: row.get(6)?,
                profile_digest: row.get(7)?,
                profile_identity: row.get(8)?,
                auth_service: row.get(9)?,
                auth_reference: row.get(10)?,
                effective_model: row.get(11)?,
                output_digest: row.get(12)?,
                tool_probe_digest: row.get(13)?,
                ownership_sha256: row.get(14)?,
                one_shot: row.get::<_, i32>(15)? != 0,
                tool_isolation: row.get::<_, i32>(16)? != 0,
                state_isolation: row.get::<_, i32>(17)? != 0,
                process_containment: row.get::<_, i32>(18)? != 0,
                certified_at: row.get(19)?,
            })
        },
    )
    .optional()
    .map_err(|error| format!("failed to read recap certification: {error}"))
}

impl StoredRecapCertification {
    /// Compare every persisted field that can influence native admission.
    pub(crate) fn matches(
        &self,
        certification: &RecapCertificationParts,
        ownership_sha256: &str,
    ) -> bool {
        self.runtime_id == certification.runtime_id
            && self.executable_path == certification.executable.resolved_path.to_string_lossy()
            && self.executable_version == certification.executable.version
            && self.executable_fingerprint == certification.executable.fingerprint
            && self.platform == certification.executable.platform
            && self.model == certification.selection.model
            && self.profile
                == certification
                    .selection
                    .profile
                    .as_deref()
                    .map(|path| path.to_string_lossy().into_owned())
            && self.profile_digest == certification.selection.profile_digest
            && self.profile_identity
                == certification
                    .selection
                    .profile_identity
                    .as_ref()
                    .and_then(|identity| serde_json::to_string(identity).ok())
            && self.auth_service == certification.auth.service
            && self.auth_reference == certification.auth.reference
            && self.effective_model == certification.effective_model
            && self.output_digest == certification.output_digest
            && self.tool_probe_digest == certification.tool_probe_digest
            && self.ownership_sha256 == ownership_sha256
            && self.one_shot == certification.guarantees.one_shot
            && self.tool_isolation == certification.guarantees.tool_isolation
            && self.state_isolation == certification.guarantees.state_isolation
            && self.process_containment == certification.guarantees.process_containment
    }
}

#[cfg(test)]
#[path = "recap_tests.rs"]
mod tests;
