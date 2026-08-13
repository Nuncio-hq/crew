//! Local Project-thread worktree lifecycle: identity records and leases.
//!
//! This crate owns only local filesystem orchestration that both `buzz-acp`
//! and Desktop Tauri must share:
//! - versioned lifecycle records under the repository common Git directory;
//! - cross-process shared/exclusive advisory leases for active turns vs eviction.
//!
//! Git mutation stays in callers. Records never store credentials or prompt text.

#![deny(unsafe_code)]

mod error;
mod identity;
mod lease;
mod path_lease;
mod paths;
mod record;

pub use error::{LeaseError, RecordError, WorktreeError};
pub use identity::{normalize_root_event_id, validate_root_event_id, ROOT_EVENT_ID_LEN};
pub use lease::{try_acquire_exclusive, try_acquire_shared, ExclusiveLease, SharedLease};
pub use path_lease::{
    read_path_lease_holder, try_acquire_path_exclusive, PathExclusiveLease, PathLeaseHolder,
};
pub use paths::{
    lease_lock_path, lifecycle_record_path, lifecycle_records_dir, path_lease_lock_path,
    LEASE_DIRECTORY, LEASE_SCHEMA_VERSION, LIFECYCLE_RECORD_DIRECTORY, PATH_LEASE_DIRECTORY,
    RECORD_SCHEMA_VERSION,
};
pub use record::{
    adopt_or_create_record, advance_eviction_generation, list_lifecycle_records,
    max_last_used_at_for_path, read_lifecycle_record, touch_last_used_at, write_lifecycle_record,
    LifecycleIdentity, LifecycleRecord,
};
