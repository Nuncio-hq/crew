//! Turn-scoped Cowork checkpoints riding the path-keyed lease.

use buzz_cowork::{CheckpointKind, CheckpointSpec, ShadowRepo};

use crate::thread_workspace::ThreadWorkspace;

/// RAII post-turn checkpoint. Pre-turn capture runs in [`begin`].
#[derive(Debug)]
pub(crate) struct CoworkTurnGuard {
    repo: ShadowRepo,
    thread_id: String,
    thread_title: String,
    agent_name: String,
}

impl CoworkTurnGuard {
    pub(crate) fn begin(workspace: &ThreadWorkspace, thread_title: &str) -> anyhow::Result<Self> {
        let repo = ShadowRepo::open_existing(&workspace.common_git, &workspace.worktree_path)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        if repo
            .is_dirty()
            .map_err(|error| anyhow::anyhow!("{error}"))?
        {
            repo.checkpoint(&CheckpointSpec {
                kind: CheckpointKind::External,
                agent_name: None,
                thread_title: None,
                thread_id: None,
                turn_seq: None,
            })
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        }
        Ok(Self {
            repo,
            thread_id: workspace.root_event_id.clone(),
            thread_title: thread_title.to_string(),
            agent_name: "agent".into(),
        })
    }

    pub(crate) fn arm_agent(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !name.trim().is_empty() {
            self.agent_name = name;
        }
    }
}

impl Drop for CoworkTurnGuard {
    fn drop(&mut self) {
        let seq = self.repo.next_turn_seq(&self.thread_id).unwrap_or(1);
        let _ = self.repo.checkpoint(&CheckpointSpec {
            kind: CheckpointKind::Turn,
            agent_name: Some(self.agent_name.clone()),
            thread_title: Some(self.thread_title.clone()),
            thread_id: Some(self.thread_id.clone()),
            turn_seq: Some(seq),
        });
    }
}
