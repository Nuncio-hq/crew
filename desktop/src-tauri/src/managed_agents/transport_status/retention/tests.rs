use super::*;
use buzz_core_pkg::transport_status::{
    TransportCode, TransportRecord, TransportState, TransportStatus, MAX_DIRECTORY_ENTRIES,
};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

struct Fixture {
    directory: PathBuf,
    runtime_id: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("crew-338-retention-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            directory,
            runtime_id: crate::managed_agents::ManagedAgentRuntimeKey::new(
                "a".repeat(64),
                "ws://fixture",
            )
            .unwrap()
            .runtime_id(),
        }
    }
    fn opaque(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.directory.join(name);
        std::fs::write(&path, bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path
    }
    fn record(&self, nonce: &str, terminal: bool, time: u64) -> PathBuf {
        let record = TransportRecord {
            version: 1,
            runtime_id: self.runtime_id.clone(),
            start_nonce: nonce.into(),
            sequence: 1,
            timestamp_ms: time * 1000,
            terminal,
            transport: TransportStatus {
                state: if terminal {
                    TransportState::Exhausted
                } else {
                    TransportState::Connected
                },
                code: TransportCode::None,
                attempts: 1,
                elapsed_ms: 0,
                next_retry_at_ms: None,
                last_error: None,
            },
        };
        let path = self.opaque(
            &format!("{nonce}.json"),
            &serde_json::to_vec(&record).unwrap(),
        );
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(time)),
            )
            .unwrap();
        path
    }
    fn run(&self, protected: &HashSet<String>) -> Result<(), String> {
        preflight(&self.directory, &self.runtime_id, protected, 100_000)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[test]
fn preserves_active_last_required_and_recent_history_while_reclaiming_old_owned_records() {
    let fixture = Fixture::new();
    let active = nonce();
    let required = nonce();
    let old = nonce();
    let recent = nonce();
    let active_path = fixture.record(&active, false, 100);
    let required_path = fixture.record(&required, true, 100);
    let old_path = fixture.record(&old, false, 100);
    let recent_path = fixture.record(&recent, false, 99_999);
    let old_stage = fixture.opaque(&format!(".{old}.tmp"), b"partial");
    let active_stage = fixture.opaque(&format!(".{active}.pending"), b"partial");
    fixture.run(&HashSet::from([active, required])).unwrap();
    assert!(active_path.exists());
    assert!(required_path.exists());
    assert!(recent_path.exists());
    assert!(!old_path.exists());
    assert!(!old_stage.exists());
    assert!(active_stage.exists());
}

#[test]
fn repeated_successful_replacements_have_no_restart_capacity_cliff() {
    let fixture = Fixture::new();
    let mut previous = nonce();
    fixture.record(&previous, false, 99_999);
    for _ in 0..MAX_DIRECTORY_ENTRIES + 5 {
        fixture.run(&HashSet::from([previous.clone()])).unwrap();
        let next = nonce();
        fixture.record(&next, false, 99_999);
        previous = next;
        assert!(std::fs::read_dir(&fixture.directory).unwrap().count() <= 5);
    }
}

#[test]
fn malformed_saturation_refuses_without_deleting_required_history() {
    let fixture = Fixture::new();
    let required = nonce();
    let path = fixture.record(&required, true, 100);
    let original = std::fs::read(&path).unwrap();
    for index in 1..MAX_DIRECTORY_ENTRIES {
        fixture.opaque(&format!("unknown-{index}"), b"preserve");
    }
    assert!(fixture.run(&HashSet::from([required])).is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
    assert_eq!(
        std::fs::read(fixture.directory.join("unknown-1")).unwrap(),
        b"preserve"
    );
}

#[test]
fn failed_replacement_keeps_previous_required_final_on_every_attempt() {
    let fixture = Fixture::new();
    let required = nonce();
    let path = fixture.record(&required, true, 100);
    let original = std::fs::read(&path).unwrap();
    for _ in 0..MAX_DIRECTORY_ENTRIES + 5 {
        fixture.run(&HashSet::from([required.clone()])).unwrap();
        fixture.record(&nonce(), false, 99_999); // spawned but never registered
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(std::fs::read_dir(&fixture.directory).unwrap().count() <= 5);
    }
}

#[test]
fn malformed_wrong_pair_and_noncanonical_names_are_never_cleanup_candidates() {
    let fixture = Fixture::new();
    let malformed = fixture.opaque(&format!("{}.json", nonce()), b"{");
    let unknown = fixture.opaque("user-note", b"preserve");
    let not_staging = fixture.opaque("not-a-nonce.tmp", b"preserve");
    let foreign = fixture.record(&nonce(), true, 100);
    let mut record: TransportRecord =
        serde_json::from_slice(&std::fs::read(&foreign).unwrap()).unwrap();
    record.runtime_id = "other-pair".into();
    std::fs::write(&foreign, serde_json::to_vec(&record).unwrap()).unwrap();
    fixture.run(&HashSet::new()).unwrap();
    for path in [malformed, unknown, not_staging, foreign] {
        assert!(path.exists());
    }
}

#[test]
fn unsafe_entries_refuse_before_any_owned_history_is_removed() {
    for kind in ["mode", "link", "symlink", "fifo", "directory", "oversized"] {
        let fixture = Fixture::new();
        let old = fixture.record(&nonce(), true, 100);
        let original = std::fs::read(&old).unwrap();
        let unsafe_path = fixture.directory.join("unsafe");
        match kind {
            "mode" => {
                fixture.opaque("unsafe", b"x");
                std::fs::set_permissions(&unsafe_path, std::fs::Permissions::from_mode(0o644))
                    .unwrap();
            }
            "link" => {
                std::fs::hard_link(&old, &unsafe_path).unwrap();
            }
            "symlink" => {
                std::os::unix::fs::symlink(&old, &unsafe_path).unwrap();
            }
            "fifo" => {
                nix::unistd::mkfifo(
                    &unsafe_path,
                    nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
                )
                .unwrap();
            }
            "directory" => {
                std::fs::create_dir(&unsafe_path).unwrap();
            }
            "oversized" => {
                fixture.opaque("unsafe", &vec![b'x'; 8193]);
            }
            _ => unreachable!(),
        }
        assert!(fixture.run(&HashSet::new()).is_err(), "{kind}");
        assert_eq!(std::fs::read(old).unwrap(), original, "{kind}");
    }
}

#[test]
fn shared_directory_lock_blocks_cleanup_until_the_writer_releases_it() {
    use fs4::fs_std::FileExt;
    let fixture = Fixture::new();
    let old = fixture.record(&nonce(), true, 100);
    let lock_path = fixture.opaque(".spool.lock", b"");
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    file.try_lock_exclusive().unwrap();
    assert!(fixture.run(&HashSet::new()).is_err());
    assert!(old.exists());
    drop(file);
    fixture.run(&HashSet::new()).unwrap();
    assert!(!old.exists());
}
