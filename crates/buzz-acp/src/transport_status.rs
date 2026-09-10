//! Owned local transport diagnostics for a Desktop-managed harness generation.
use crate::secure_spool;
use buzz_core::transport_status::{
    TransportRecord, TransportStatus, GENERATION_ENTRY_RESERVATION, MAX_DIRECTORY_ENTRIES,
    MAX_RECORD_BYTES, STORAGE_REVIEW_ERROR,
};
use std::ffi::OsString;
use std::path::{Component, PathBuf};

#[derive(Debug)]
pub(super) struct StatusConfig {
    pub path: PathBuf,
    pub nonce: String,
}

impl StatusConfig {
    pub fn parse(path: Option<OsString>, nonce: Option<OsString>) -> Result<Option<Self>, String> {
        let (path, nonce) = match (path, nonce) {
            (None, None) => return Ok(None),
            (Some(path), Some(nonce)) => (path, nonce),
            _ => return Err("managed transport requires both status path and start nonce".into()),
        };
        let path_text = path
            .to_str()
            .ok_or("managed transport status path must be UTF-8")?;
        let path_value = PathBuf::from(&path);
        if path_text.len() > 4096
            || path_text.ends_with('/')
            || !path_value.is_absolute()
            || path_value.file_name().is_none()
            || path_value
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
            || path_text.split('/').any(|part| part == ".")
        {
            return Err(
                "managed transport status path must be an absolute file path without traversal"
                    .into(),
            );
        }
        let nonce = nonce
            .into_string()
            .map_err(|_| "managed transport start nonce must be UTF-8")?;
        let parsed = uuid::Uuid::parse_str(&nonce)
            .map_err(|_| "managed transport start nonce must be a canonical nonzero UUID")?;
        if parsed.is_nil() || (parsed.to_string() != nonce && parsed.simple().to_string() != nonce)
        {
            return Err("managed transport start nonce must be a canonical nonzero UUID".into());
        }
        Ok(Some(Self {
            path: path_value,
            nonce,
        }))
    }
}

#[derive(Debug)]
pub(super) struct StatusWriter {
    config: StatusConfig,
    runtime_id: String,
    sequence: u64,
    admission_lock: Option<secure_spool::SecureSpoolDirectoryLock>,
}

impl StatusWriter {
    pub async fn new(config: StatusConfig, pubkey: &str, relay_url: &str) -> Result<Self, String> {
        if relay_url.len() > 4096 {
            return Err("managed transport relay identity exceeds the size limit".into());
        }
        let runtime_id = buzz_core::transport_status::runtime_id(pubkey, relay_url)?;
        let parent = config
            .path
            .parent()
            .ok_or("managed transport status path has no parent")?;
        secure_spool::ensure_secure_directory(parent)
            .await
            .map_err(|_| "cannot establish private managed transport status directory")?;
        let admission_lock = lock_directory(parent).await?;
        check_capacity(&config).await?;
        Ok(Self {
            config,
            runtime_id,
            sequence: 0,
            admission_lock: Some(admission_lock),
        })
    }

    pub async fn write(&mut self, status: TransportStatus, terminal: bool) -> Result<(), String> {
        if !status.has_safe_error() {
            return Err("managed transport status contains an untrusted error message".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("managed transport status sequence exhausted")?;
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "managed transport status clock unavailable")?
            .as_millis();
        let record = TransportRecord {
            version: 1,
            runtime_id: self.runtime_id.clone(),
            start_nonce: self.config.nonce.clone(),
            sequence: self.sequence,
            timestamp_ms: u64::try_from(timestamp_ms).unwrap_or(u64::MAX),
            terminal,
            transport: status,
        };
        let bytes =
            serde_json::to_vec(&record).map_err(|_| "cannot encode managed transport status")?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err("managed transport status exceeds the size limit".into());
        }
        let parent = self
            .config
            .path
            .parent()
            .ok_or("managed transport status path has no parent")?;
        let name = self
            .config
            .path
            .file_name()
            .ok_or("managed transport status path has no filename")?;
        // All writers share the native preflight lock. A retained initial
        // admission ends after first write; later writes recheck capacity too,
        // so an unlinked orphan writer cannot recreate beyond the hard cap.
        let _directory_lock = match self.admission_lock.take() {
            Some(lock) => lock,
            None => lock_directory(parent).await?,
        };
        check_capacity(&self.config).await?;
        let pending = OsString::from(format!(".{}.pending", self.config.nonce));
        let temporary = OsString::from(format!(".{}.tmp", self.config.nonce));
        // A single writer owns this generation. Bound crash residue to two names;
        // every operation is descriptor-relative and refuses symlink ancestors.
        secure_spool::remove_secure_entry(parent, &pending)
            .await
            .map_err(|_| "cannot clear pending managed transport status")?;
        secure_spool::remove_secure_entry(parent, &temporary)
            .await
            .map_err(|_| "cannot clear temporary managed transport status")?;
        if !secure_spool::write_secure_entry_if_absent(parent, &pending, &temporary, &bytes)
            .await
            .map_err(|_| "cannot persist managed transport status")?
        {
            return Err("managed transport pending status is unexpectedly occupied".into());
        }
        if !secure_spool::rename_secure_entry(parent, &pending, name)
            .await
            .map_err(|_| "cannot commit managed transport status")?
        {
            return Err("managed transport pending status disappeared before commit".into());
        }
        Ok(())
    }
}

async fn lock_directory(
    path: &std::path::Path,
) -> Result<secure_spool::SecureSpoolDirectoryLock, String> {
    for attempt in 0..20 {
        match secure_spool::lock_secure_directory(path).await {
            Ok(lock) => return Ok(lock),
            Err(error) if error == secure_spool::SECURE_SPOOL_LOCK_CONTENDED && attempt < 19 => {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            Err(_) => return Err(STORAGE_REVIEW_ERROR.into()),
        }
    }
    Err(STORAGE_REVIEW_ERROR.into())
}

async fn check_capacity(config: &StatusConfig) -> Result<(), String> {
    let parent = config.path.parent().ok_or(STORAGE_REVIEW_ERROR)?;
    let names =
        secure_spool::list_secure_flat_names(parent, MAX_RECORD_BYTES, MAX_DIRECTORY_ENTRIES)
            .await
            .map_err(|_| STORAGE_REVIEW_ERROR)?;
    let final_name = config.path.file_name().ok_or(STORAGE_REVIEW_ERROR)?;
    let pending = OsString::from(format!(".{}.pending", config.nonce));
    let temporary = OsString::from(format!(".{}.tmp", config.nonce));
    let own_entries = names
        .iter()
        .filter(|name| name.as_os_str() == final_name || **name == pending || **name == temporary)
        .count();
    if names.len().saturating_sub(own_entries) + GENERATION_ENTRY_RESERVATION
        > MAX_DIRECTORY_ENTRIES
    {
        return Err(STORAGE_REVIEW_ERROR.into());
    }
    Ok(())
}

#[derive(Debug)]
struct RenewingWriter {
    writer: StatusWriter,
    latest: Option<(TransportStatus, bool)>,
    last_attempt: tokio::time::Instant,
    dirty: bool,
}

impl RenewingWriter {
    fn new(writer: StatusWriter) -> Self {
        Self {
            writer,
            latest: None,
            last_attempt: tokio::time::Instant::now(),
            dirty: false,
        }
    }

    async fn update(&mut self, status: TransportStatus, terminal: bool) -> Result<(), String> {
        if !self.dirty && self.latest.as_ref() == Some(&(status.clone(), terminal)) {
            return Ok(());
        }
        self.latest = Some((status.clone(), terminal));
        self.last_attempt = tokio::time::Instant::now();
        let result = self.writer.write(status, terminal).await;
        self.dirty = result.is_err();
        result
    }

    async fn renew(&mut self) -> Result<(), String> {
        if self.last_attempt.elapsed()
            < std::time::Duration::from_secs(buzz_core::transport_status::RENEWAL_SECONDS)
        {
            return Ok(());
        }
        let Some((status, false)) = self.latest.clone() else {
            return Ok(());
        };
        self.last_attempt = tokio::time::Instant::now();
        let result = self.writer.write(status, false).await;
        self.dirty = result.is_err();
        result
    }
}

/// One serialized writer owns both state changes and bounded liveness renewal.
#[derive(Debug)]
pub(super) struct StatusReporter {
    writer: std::sync::Arc<tokio::sync::Mutex<RenewingWriter>>,
    renewal: tokio::task::JoinHandle<()>,
}

impl StatusReporter {
    pub fn new(writer: StatusWriter) -> Self {
        let writer = std::sync::Arc::new(tokio::sync::Mutex::new(RenewingWriter::new(writer)));
        let renewal_writer = writer.clone();
        let renewal = tokio::spawn(async move {
            let period =
                std::time::Duration::from_secs(buzz_core::transport_status::RENEWAL_SECONDS);
            let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticks.tick().await;
                if let Err(error) = renewal_writer.lock().await.renew().await {
                    // Fixed messages only; the retained latest state is retried
                    // on the next bounded tick. Readers expire the old lease.
                    tracing::warn!("{error}");
                }
            }
        });
        Self { writer, renewal }
    }

    pub async fn publish(&self, status: TransportStatus, terminal: bool) -> Result<(), String> {
        self.writer.lock().await.update(status, terminal).await
    }
}

impl Drop for StatusReporter {
    fn drop(&mut self) {
        self.renewal.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_pair_preserves_standalone_mode() {
        assert!(StatusConfig::parse(None, None).unwrap().is_none());
    }

    #[test]
    fn complete_pair_enables_owned_status() {
        let nonce = uuid::Uuid::new_v4().to_string();
        let config = StatusConfig::parse(
            Some("/private/tmp/crew/status.json".into()),
            Some(nonce.clone().into()),
        )
        .unwrap()
        .expect("complete pair must opt into managed transport");
        assert_eq!(config.path, PathBuf::from("/private/tmp/crew/status.json"));
        assert_eq!(config.nonce, nonce);
    }

    #[test]
    fn desktop_existing_simple_nonce_is_preserved_exactly() {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let config = StatusConfig::parse(
            Some("/private/tmp/crew/status.json".into()),
            Some(nonce.clone().into()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(config.nonce, nonce);
    }

    #[test]
    fn partial_pair_is_config_error() {
        assert!(StatusConfig::parse(Some("/private/tmp/crew/status.json".into()), None).is_err());
        assert!(StatusConfig::parse(None, Some(uuid::Uuid::new_v4().to_string().into())).is_err());
    }

    #[test]
    fn invalid_pair_never_falls_back_to_standalone() {
        for path in [
            "",
            "status.json",
            "/",
            "/private/tmp/../status.json",
            "/private/tmp/status.json/",
        ] {
            assert!(
                StatusConfig::parse(
                    Some(path.into()),
                    Some(uuid::Uuid::new_v4().to_string().into())
                )
                .is_err(),
                "{path}"
            );
        }
        for nonce in [
            "",
            "not-a-generation",
            "../escape",
            "00000000-0000-0000-0000-000000000000",
        ] {
            assert!(
                StatusConfig::parse(
                    Some("/private/tmp/crew/status.json".into()),
                    Some(nonce.into())
                )
                .is_err(),
                "{nonce}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_nonce_is_config_error() {
        use std::os::unix::ffi::OsStringExt;
        assert!(StatusConfig::parse(
            Some("/private/tmp/crew/status.json".into()),
            Some(OsString::from_vec(vec![255]))
        )
        .is_err());
    }
}

#[cfg(all(test, unix))]
mod writer_tests {
    use super::*;
    use buzz_core::transport_status::{
        TransportCode, TransportRecord, TransportState, TransportStatus,
    };
    use std::os::unix::fs::{symlink, PermissionsExt};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("crew-338-status-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(root)
        }
        fn config(&self) -> StatusConfig {
            StatusConfig {
                path: self.0.join("status.json"),
                nonce: uuid::Uuid::new_v4().to_string(),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn status(state: TransportState) -> TransportStatus {
        TransportStatus {
            state,
            code: TransportCode::None,
            attempts: 1,
            elapsed_ms: 0,
            next_retry_at_ms: None,
            last_error: None,
        }
    }

    #[tokio::test]
    async fn writer_replaces_latest_private_record_with_advancing_sequence() {
        let fixture = Fixture::new();
        let config = fixture.config();
        let nonce = config.nonce.clone();
        let path = config.path.clone();
        let mut writer = StatusWriter::new(config, &"A".repeat(64), "WS://localhost:80/")
            .await
            .unwrap();
        writer
            .write(status(TransportState::Connecting), false)
            .await
            .unwrap();
        let first: TransportRecord =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        writer
            .write(status(TransportState::Connected), false)
            .await
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let second: TransportRecord = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        assert_eq!(second.start_nonce, nonce);
        assert_eq!(
            second.runtime_id,
            buzz_core::transport_status::runtime_id(&"a".repeat(64), "ws://127.0.0.1").unwrap()
        );
        assert_eq!(second.transport.state, TransportState::Connected);
        assert!(bytes.len() <= 8192);
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let names: std::collections::HashSet<_> = std::fs::read_dir(&fixture.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            std::collections::HashSet::from([
                OsString::from("status.json"),
                OsString::from(".spool.lock")
            ])
        );
        assert_eq!(
            std::fs::metadata(fixture.0.join(".spool.lock"))
                .unwrap()
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn writer_rejects_symlink_ancestor() {
        let fixture = Fixture::new();
        let target = fixture.0.join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
        let link = fixture.0.join("link");
        symlink(&target, &link).unwrap();
        let mut config = fixture.config();
        config.path = link.join("status.json");
        assert!(StatusWriter::new(config, &"a".repeat(64), "ws://fixture")
            .await
            .is_err());
        assert!(!target.join("status.json").exists());
    }

    #[tokio::test]
    async fn writer_rejects_untrusted_error_text_and_oversized_identity() {
        let fixture = Fixture::new();
        let mut writer = StatusWriter::new(fixture.config(), &"a".repeat(64), "ws://fixture")
            .await
            .unwrap();
        let mut unsafe_status = status(TransportState::Degraded);
        unsafe_status.last_error = Some("secret-token-from-relay".into());
        assert!(writer.write(unsafe_status, false).await.is_err());
        assert!(!fixture.0.join("status.json").exists());
        assert!(StatusWriter::new(
            fixture.config(),
            &"a".repeat(64),
            &format!("ws://fixture/{}", "a".repeat(9000))
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn writer_failure_propagates_without_overwriting_committed_record() {
        let fixture = Fixture::new();
        let path = fixture.0.join("status.json");
        let mut writer = StatusWriter::new(fixture.config(), &"a".repeat(64), "ws://fixture")
            .await
            .unwrap();
        writer
            .write(status(TransportState::Connected), false)
            .await
            .unwrap();
        let original = std::fs::read(&path).unwrap();
        let moved = fixture.0.join("retired");
        std::fs::rename(&path, &moved).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(writer
            .write(status(TransportState::Exhausted), true)
            .await
            .is_err());
        assert_eq!(std::fs::read(moved).unwrap(), original);
    }
    #[tokio::test(start_paused = true)]
    async fn renewal_advances_only_at_five_seconds_and_stops_for_terminal_record() {
        let fixture = Fixture::new();
        let path = fixture.0.join("status.json");
        let writer = StatusWriter::new(fixture.config(), &"a".repeat(64), "ws://fixture")
            .await
            .unwrap();
        let mut renewal = RenewingWriter::new(writer);
        let connected = status(TransportState::Connected);
        renewal.update(connected.clone(), false).await.unwrap();
        let initial: TransportRecord =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        renewal.update(connected, false).await.unwrap();
        tokio::time::advance(std::time::Duration::from_secs(4)).await;
        renewal.renew().await.unwrap();
        let before: TransportRecord =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            before.sequence, initial.sequence,
            "identical polls and early ticks do not write"
        );
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        renewal.renew().await.unwrap();
        let renewed: TransportRecord =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            renewed.sequence,
            initial.sequence + 1,
            "renewal must advance the lease"
        );
        renewal
            .update(status(TransportState::Exhausted), true)
            .await
            .unwrap();
        let final_bytes = std::fs::read(&path).unwrap();
        tokio::time::advance(std::time::Duration::from_secs(30)).await;
        renewal.renew().await.unwrap();
        assert_eq!(
            std::fs::read(path).unwrap(),
            final_bytes,
            "final startup failure cannot renew forever"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn failed_update_expires_old_connected_lease_and_renewal_recovers() {
        let fixture = Fixture::new();
        let config = fixture.config();
        let pending = fixture.0.join(format!(".{}.pending", config.nonce));
        let path = config.path.clone();
        let writer = StatusWriter::new(config, &"a".repeat(64), "ws://fixture")
            .await
            .unwrap();
        let mut renewal = RenewingWriter::new(writer);
        renewal
            .update(status(TransportState::Connected), false)
            .await
            .unwrap();
        let original = std::fs::read(&path).unwrap();
        let record: TransportRecord = serde_json::from_slice(&original).unwrap();
        let mut lease = buzz_core::transport_status::TransportLease::default();
        let now = std::time::Instant::now();
        lease
            .observe(
                record.clone(),
                &record.runtime_id,
                &record.start_nonce,
                now,
                record.timestamp_ms,
                false,
            )
            .unwrap();
        // A real failing staging operation leaves the authoritative pathname
        // untouched. No read-only-permission assumption (which root bypasses).
        std::fs::create_dir(&pending).unwrap();
        assert!(renewal
            .update(status(TransportState::Degraded), false)
            .await
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        lease.expire(now + std::time::Duration::from_secs(16));
        assert_eq!(lease.status().state, TransportState::Unknown);
        std::fs::remove_dir(&pending).unwrap();
        tokio::time::advance(std::time::Duration::from_secs(5)).await;
        renewal.renew().await.unwrap();
        let recovered: TransportRecord =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(recovered.sequence > record.sequence);
        assert_eq!(recovered.transport.state, TransportState::Degraded);
        lease
            .observe(
                recovered,
                &record.runtime_id,
                &record.start_nonce,
                now + std::time::Duration::from_secs(17),
                record.timestamp_ms + 17_000,
                false,
            )
            .unwrap();
        assert_eq!(lease.status().state, TransportState::Degraded);
    }

    fn fill_capacity(fixture: &Fixture) {
        for index in 0..buzz_core::transport_status::MAX_DIRECTORY_ENTRIES {
            let path = fixture.0.join(format!("unknown-{index}"));
            std::fs::write(&path, b"preserve").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }

    #[tokio::test]
    async fn full_directory_refuses_new_generation_without_removing_history() {
        let fixture = Fixture::new();
        fill_capacity(&fixture);
        let original = std::fs::read(fixture.0.join("unknown-0")).unwrap();
        assert!(
            StatusWriter::new(fixture.config(), &"a".repeat(64), "ws://fixture")
                .await
                .is_err()
        );
        assert!(!fixture.config().path.exists());
        assert_eq!(
            std::fs::read(fixture.0.join("unknown-0")).unwrap(),
            original
        );
    }

    #[tokio::test]
    async fn unlinked_old_writer_must_readmit_before_recreating_a_status_file() {
        let fixture = Fixture::new();
        let config = fixture.config();
        let mut writer = StatusWriter::new(config, &"a".repeat(64), "ws://fixture")
            .await
            .unwrap();
        writer
            .write(status(TransportState::Connected), false)
            .await
            .unwrap();
        std::fs::remove_file(fixture.config().path).unwrap();
        fill_capacity(&fixture);
        assert!(writer
            .write(status(TransportState::Connected), false)
            .await
            .is_err());
        assert!(
            !fixture.config().path.exists(),
            "an old writer cannot exceed capacity after native cleanup"
        );
    }

    #[tokio::test]
    async fn writer_and_native_flat_directory_rules_reject_the_same_unsafe_entries() {
        for kind in ["mode", "link", "symlink", "fifo", "directory", "oversized"] {
            let fixture = Fixture::new();
            let path = fixture.0.join("unsafe");
            match kind {
                "mode" => {
                    std::fs::write(&path, b"x").unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
                        .unwrap();
                }
                "link" => {
                    std::fs::write(&path, b"x").unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                    std::fs::hard_link(&path, fixture.0.join("alias")).unwrap();
                }
                "symlink" => {
                    symlink("missing", &path).unwrap();
                }
                "fifo" => {
                    nix::unistd::mkfifo(
                        &path,
                        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
                    )
                    .unwrap();
                }
                "directory" => {
                    std::fs::create_dir(&path).unwrap();
                }
                "oversized" => {
                    std::fs::write(&path, vec![b'x'; 8193]).unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                StatusWriter::new(fixture.config(), &"a".repeat(64), "ws://fixture")
                    .await
                    .is_err(),
                "{kind}"
            );
            assert!(std::fs::symlink_metadata(path).is_ok());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn first_write_releases_admission_lock_while_writer_remains_alive() {
        let fixture = Fixture::new();
        let mut writer = StatusWriter::new(fixture.config(), &"a".repeat(64), "ws://fixture")
            .await
            .unwrap();
        assert!(secure_spool::lock_secure_directory(&fixture.0)
            .await
            .is_err());
        writer
            .write(status(TransportState::Connected), false)
            .await
            .unwrap();
        let native_lock = secure_spool::lock_secure_directory(&fixture.0)
            .await
            .unwrap();
        assert!(writer
            .write(status(TransportState::Degraded), false)
            .await
            .is_err());
        drop(native_lock);
        writer
            .write(status(TransportState::Degraded), false)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn startup_denial_flushes_terminal_owned_status_before_returning() {
        let fixture = Fixture::new();
        let path = fixture.0.join("status.json");
        let writer = StatusWriter::new(fixture.config(), &"a".repeat(64), "ws://fixture")
            .await
            .unwrap();
        let mut health = super::super::TransportHealth::with_reporter(StatusReporter::new(writer));
        let result: Result<(), super::super::RelayError> =
            super::super::transport_reconnect::retry_initial_connect_with_health(
                &mut health,
                || async {
                    Err(super::super::RelayError::AuthDenied(
                        "restricted: secret-token-from-relay".into(),
                    ))
                },
            )
            .await;
        assert!(result.is_err());
        let bytes = std::fs::read(path)
            .expect("startup must flush its final status before returning failure");
        let record: TransportRecord = serde_json::from_slice(&bytes).unwrap();
        assert!(record.terminal);
        assert_eq!(record.transport.state, TransportState::AuthRejected);
        assert_eq!(record.transport.attempts, 1);
        assert!(!String::from_utf8(bytes)
            .unwrap()
            .contains("secret-token-from-relay"));
    }
}
