use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Duration,
};

use crate::app_state::owner_scope::OwnerScopeToken;

type Key = (String, String, String);
type Locks = Mutex<HashMap<Key, Weak<tokio::sync::Mutex<()>>>>;
static LOCKS: OnceLock<Locks> = OnceLock::new();

pub(super) async fn acquire(
    scope: &OwnerScopeToken,
    channel: &str,
) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
    let parsed = uuid::Uuid::parse_str(channel).map_err(|_| "invalid channel UUID")?;
    if parsed.to_string() != channel {
        return Err("channel UUID must be canonical".into());
    }
    let lock = {
        let mut locks = LOCKS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map_err(|_| "canvas save lock unavailable")?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        let key = (
            scope.scope.owner.clone(),
            scope.scope.community.clone(),
            channel.to_string(),
        );
        if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
            lock
        } else {
            if locks.len() >= 256 {
                return Err("too many active canvas saves; retry later".into());
            }
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            locks.insert(key, Arc::downgrade(&lock));
            lock
        }
    };
    tokio::time::timeout(Duration::from_secs(5), lock.lock_owned())
        .await
        .map_err(|_| "another canvas save is in progress; retry".into())
}
