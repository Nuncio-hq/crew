//! Where a bare sidecar command name is looked up, and in what order.

use std::path::{Path, PathBuf};

fn profile_build_dirs(root: &Path) -> [PathBuf; 2] {
    if cfg!(debug_assertions) {
        // `just dev` builds fresh debug sidecars; never prefer stale release output.
        [root.join("target/debug"), root.join("target/release")]
    } else {
        [root.join("target/release"), root.join("target/debug")]
    }
}

pub(crate) fn command_search_dirs(workspace_root: &Path) -> Vec<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    ordered_command_search_dirs(
        workspace_root,
        std::env::current_dir().ok().as_deref(),
        exe_dir.as_deref(),
        !cfg!(debug_assertions),
    )
}

/// The workspace root is baked in at compile time, so a shipped build keeps
/// pointing at the checkout it was compiled from. Where that checkout still
/// exists — the build host, or any developer laptop — its build output would
/// otherwise outrank the binaries bundled beside the executable, and an
/// installed app could spawn a stale debug sidecar. A build that ships its own
/// sidecars therefore looks next to the executable first; a dev build keeps
/// preferring the workspace it just rebuilt.
pub(crate) fn ordered_command_search_dirs(
    workspace_root: &Path,
    current_dir: Option<&Path>,
    exe_dir: Option<&Path>,
    prefer_bundled: bool,
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if prefer_bundled {
        dirs.extend(exe_dir.map(Path::to_path_buf));
    }
    dirs.extend(profile_build_dirs(workspace_root));
    if let Some(current_dir) = current_dir {
        dirs.extend(profile_build_dirs(current_dir));
    }
    if !prefer_bundled {
        dirs.extend(exe_dir.map(Path::to_path_buf));
    }
    dirs.into_iter().fold(Vec::new(), |mut unique, dir| {
        if !unique.contains(&dir) {
            unique.push(dir);
        }
        unique
    })
}

#[cfg(test)]
mod tests {
    use super::ordered_command_search_dirs;
    use std::path::Path;

    /// A shipped build must spawn the sidecar it bundled, not whatever build
    /// output happens to sit in the checkout it was compiled from.
    #[test]
    fn a_shipped_build_searches_its_bundled_sidecars_before_any_checkout() {
        let workspace = Path::new("/src/crew");
        let cwd = Path::new("/src/other");
        let bundled = Path::new("/Applications/Crew.app/Contents/MacOS");

        let shipped = ordered_command_search_dirs(workspace, Some(cwd), Some(bundled), true);
        assert_eq!(shipped.first().map(|dir| dir.as_path()), Some(bundled));

        let dev = ordered_command_search_dirs(workspace, Some(cwd), Some(bundled), false);
        assert_eq!(dev.last().map(|dir| dir.as_path()), Some(bundled));
        assert!(dev.first().is_some_and(|dir| dir.starts_with(workspace)));
    }

    /// Repeated roots collapse, and a build with no resolvable executable
    /// directory simply searches the checkout.
    #[test]
    fn search_dirs_are_deduplicated_and_tolerate_a_missing_exe_dir() {
        let workspace = Path::new("/src/crew");
        let dirs = ordered_command_search_dirs(workspace, Some(workspace), None, true);
        let mut unique = dirs.clone();
        unique.dedup();
        assert_eq!(dirs, unique);
        assert_eq!(dirs.len(), 2);
    }
}
