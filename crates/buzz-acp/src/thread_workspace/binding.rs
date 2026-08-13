//! Workspace-binding query params on `buzz://project-workspace` markers.
//!
//! Absent `ws` / `base` is today's behavior: a new isolated worktree off the
//! repository default base. Unknown `ws` values fail closed to that default.
//! Empty or illegal branch names return a named error.

use anyhow::{bail, Result};

/// How a Project thread binds to a git checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBindingSpec {
    /// Fresh `buzz/<root-prefix>` branch from `base` (or the repo default).
    NewWorktree { base: Option<String> },
    /// Agent works directly on the canonical checkout (`ws=main`).
    Main,
    /// Attach to an existing named branch (`ws=branch:<name>`).
    ExistingBranch { name: String },
}

impl Default for WorkspaceBindingSpec {
    fn default() -> Self {
        Self::NewWorktree { base: None }
    }
}

impl WorkspaceBindingSpec {
    /// Parse `ws` / `base` query values. Missing both is [`Self::default`].
    pub fn from_params(ws: Option<&str>, base: Option<&str>) -> Result<Self> {
        let ws = ws.map(str::trim).filter(|value| !value.is_empty());
        let base = base.map(str::trim).filter(|value| !value.is_empty());
        match ws {
            None => {
                let base = match base {
                    Some(branch) => Some(validated_branch(branch, "base")?),
                    None => None,
                };
                Ok(Self::NewWorktree { base })
            }
            Some("main") => Ok(Self::Main),
            Some(value) => {
                let Some(name) = value.strip_prefix("branch:") else {
                    // Unknown `ws` fails closed to today's isolated worktree.
                    let base = match base {
                        Some(branch) => Some(validated_branch(branch, "base")?),
                        None => None,
                    };
                    return Ok(Self::NewWorktree { base });
                };
                if name.is_empty() {
                    bail!("workspace branch is missing");
                }
                Ok(Self::ExistingBranch {
                    name: validated_branch(name, "workspace")?,
                })
            }
        }
    }

    /// True when this spec matches today's absent-params default.
    #[cfg(test)]
    pub fn is_default_new_worktree(&self) -> bool {
        matches!(self, Self::NewWorktree { base: None })
    }
}

fn validated_branch(name: &str, label: &str) -> Result<String> {
    if !is_plausible_git_branch(name) {
        bail!("{label} branch is invalid");
    }
    Ok(name.to_string())
}

/// Conservative git-ref checks shared with the desktop branch picker.
pub(crate) fn is_plausible_git_branch(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    if name.starts_with('-') || name.ends_with('.') || name.ends_with(".lock") {
        return false;
    }
    if name.contains("..")
        || name.contains("//")
        || name.contains(' ')
        || name.contains('\t')
        || name.contains('\\')
        || name.contains('\0')
        || name.contains("@{")
    {
        return false;
    }
    if name.starts_with("refs/") && !name.starts_with("refs/heads/") {
        return false;
    }
    let name = name.strip_prefix("refs/heads/").unwrap_or(name);
    !name
        .split('/')
        .any(|component| component.is_empty() || component.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::{is_plausible_git_branch, WorkspaceBindingSpec};

    #[test]
    fn absent_params_are_todays_new_worktree_default() {
        assert_eq!(
            WorkspaceBindingSpec::from_params(None, None).unwrap(),
            WorkspaceBindingSpec::NewWorktree { base: None }
        );
        assert!(WorkspaceBindingSpec::from_params(None, None)
            .unwrap()
            .is_default_new_worktree());
    }

    #[test]
    fn ws_main_selects_canonical_checkout() {
        assert_eq!(
            WorkspaceBindingSpec::from_params(Some("main"), None).unwrap(),
            WorkspaceBindingSpec::Main
        );
    }

    #[test]
    fn ws_branch_selects_named_existing_branch() {
        assert_eq!(
            WorkspaceBindingSpec::from_params(Some("branch:feature/demo"), None).unwrap(),
            WorkspaceBindingSpec::ExistingBranch {
                name: "feature/demo".into()
            }
        );
    }

    #[test]
    fn base_without_ws_is_new_worktree_off_named_base() {
        assert_eq!(
            WorkspaceBindingSpec::from_params(None, Some("release")).unwrap(),
            WorkspaceBindingSpec::NewWorktree {
                base: Some("release".into())
            }
        );
    }

    #[test]
    fn unknown_ws_fails_closed_to_new_worktree() {
        assert_eq!(
            WorkspaceBindingSpec::from_params(Some("cowork"), None).unwrap(),
            WorkspaceBindingSpec::NewWorktree { base: None }
        );
    }

    #[test]
    fn empty_branch_name_is_a_named_error() {
        let error = WorkspaceBindingSpec::from_params(Some("branch:"), None).unwrap_err();
        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn illegal_branch_chars_are_a_named_error() {
        let error =
            WorkspaceBindingSpec::from_params(Some("branch:feature/../main"), None).unwrap_err();
        assert!(error.to_string().contains("invalid"));
        let error = WorkspaceBindingSpec::from_params(None, Some("-upload-pack")).unwrap_err();
        assert!(error.to_string().contains("invalid"));
    }

    #[test]
    fn plausible_branch_rejects_git_oddities() {
        assert!(is_plausible_git_branch("feature/demo"));
        assert!(is_plausible_git_branch("buzz/aaaaaaaaaaaa"));
        assert!(!is_plausible_git_branch(""));
        assert!(!is_plausible_git_branch("feature/.hidden"));
        assert!(!is_plausible_git_branch("refs/tags/v1"));
        assert!(!is_plausible_git_branch("feature lock"));
    }
}
