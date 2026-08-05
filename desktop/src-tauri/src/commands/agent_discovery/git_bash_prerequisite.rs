/// Returns the Windows-only Git Bash prerequisite used by buzz-agent's shell MCP.
/// `None` on other platforms keeps the shared Doctor surfaces platform-neutral.
#[tauri::command]
pub async fn discover_git_bash_prerequisite(
) -> Result<Option<crate::managed_agents::GitBashPrerequisite>, String> {
    tokio::task::spawn_blocking(crate::managed_agents::discover_git_bash)
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))
}
