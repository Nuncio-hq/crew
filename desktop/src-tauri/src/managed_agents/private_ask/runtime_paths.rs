//! Path facts the launch recipe must establish before a child exists.
//!
//! Two of them, both discovered the same way: by looking at the bytes on disk
//! rather than trusting the shape the recipe assumes.
//!
//! * An npm-installed `claude` is not a native binary — it is a `#!` script.
//!   Allowing only the script's own directory to be read and put on `PATH`
//!   leaves the interpreter unreachable, and the runtime dies before `main`
//!   with an error that reads like a model failure.
//! * The run root is supposed to be a disposable private tree. A hard link
//!   inside it to an inode outside it makes the `file-write*` allowance for the
//!   run root a write allowance for that outside file, because Seatbelt matches
//!   the path used, not the inode.

use super::PrivateAskFailure;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Bytes read looking for a shebang. A `#!` line longer than this is not one.
const SHEBANG_LIMIT: usize = 512;

/// Canonical directory of the interpreter named by `executable`'s shebang.
///
/// `None` when the file is not a `#!` script, when the interpreter cannot be
/// resolved, or when it already lives in the executable's own directory — in
/// each case there is nothing extra to allow.
pub(super) fn interpreter_directory(executable: &Path) -> Option<PathBuf> {
    interpreter_directory_with_path(executable, std::env::var_os("PATH"))
}

/// The same resolution against an explicit `PATH`.
///
/// Separated so a test can name its own search path instead of mutating this
/// process's environment — a global mutation would race every other test that
/// reads `PATH`.
fn interpreter_directory_with_path(
    executable: &Path,
    search_path: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let mut file = std::fs::File::open(executable).ok()?;
    let mut head = vec![0u8; SHEBANG_LIMIT];
    let read = file.read(&mut head).ok()?;
    head.truncate(read);
    let interpreter = resolved_interpreter(&head, search_path)?;
    let directory = interpreter.parent()?.canonicalize().ok()?;
    let own = executable
        .parent()
        .and_then(|parent| parent.canonicalize().ok());
    if own.as_deref() == Some(directory.as_path()) {
        return None;
    }
    Some(directory)
}

/// The program a `#!` line actually runs.
///
/// `#!/usr/bin/env node` is the npm case and it is the one that matters: `env`
/// itself lives in an already-allowed directory, but the program it goes on to
/// exec does not. Resolving through `env` here is what puts the real `node`
/// directory on the child's `PATH` and in the read allow-list; stopping at
/// `/usr/bin/env` would leave the runtime unable to start.
///
/// The `env` argument is resolved against this process's own `PATH`, which is
/// the desktop's, not anything the request controls.
fn resolved_interpreter(head: &[u8], search_path: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let interpreter = shebang_interpreter(head)?;
    let path = PathBuf::from(&interpreter);
    if path.file_name().and_then(|name| name.to_str()) != Some("env") {
        return Some(path);
    }
    let argument = shebang_interpreter_argument(head)?;
    resolve_in_search_path(&argument, search_path)
}

/// First `PATH` entry holding an executable file named `program`.
fn resolve_in_search_path(
    program: &str,
    search_path: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if program.is_empty() || program.contains('/') {
        return None;
    }
    let path = search_path?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| is_executable_file(candidate))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// The word after the interpreter on a `#!` line, if any.
fn shebang_interpreter_argument(head: &[u8]) -> Option<String> {
    let line = shebang_line(head)?;
    line.split_whitespace().nth(1).map(str::to_owned)
}

/// The absolute interpreter path from a `#!` line, or `None`.
///
/// A relative interpreter is refused rather than resolved against an
/// unspecified working directory.
pub(super) fn shebang_interpreter(head: &[u8]) -> Option<String> {
    let interpreter = shebang_line(head)?.split_whitespace().next()?.to_owned();
    if !Path::new(&interpreter).is_absolute() {
        return None;
    }
    Some(interpreter)
}

fn shebang_line(head: &[u8]) -> Option<&str> {
    let line = head
        .split(|byte| *byte == b'\n')
        .next()?
        .split(|byte| *byte == b'\r')
        .next()?;
    let line = line.strip_prefix(b"#!")?;
    if !line.is_ascii() {
        return None;
    }
    std::str::from_utf8(line).ok()
}

/// Refuse a run root that contains a hard link to an inode outside it.
///
/// Only regular files are checked: a directory's link count is at least two by
/// construction, so `nlink > 1` says nothing about it. The walk is bounded by
/// the entry budget because the root is this process's own freshly created
/// tree, and a root that is somehow larger than that is not one we understand.
#[cfg(unix)]
pub(super) fn assert_no_hard_links(root: &Path) -> Result<(), PrivateAskFailure> {
    use std::os::unix::fs::MetadataExt;

    const ENTRY_BUDGET: usize = 20_000;
    let mut pending = vec![root.to_path_buf()];
    let mut seen = 0usize;
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|_| PrivateAskFailure::InvalidState)?;
        for entry in entries {
            let entry = entry.map_err(|_| PrivateAskFailure::InvalidState)?;
            seen += 1;
            if seen > ENTRY_BUDGET {
                return Err(PrivateAskFailure::InvalidState);
            }
            let metadata = entry
                .metadata()
                .map_err(|_| PrivateAskFailure::InvalidState)?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                // A symlink is not a hard link and cannot alias an inode past
                // the policy; the run root's own creation path already refuses
                // a symlinked state directory.
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if file_type.is_file() && metadata.nlink() > 1 {
                return Err(PrivateAskFailure::InvalidState);
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn assert_no_hard_links(root: &Path) -> Result<(), PrivateAskFailure> {
    let _ = root;
    Ok(())
}

#[cfg(test)]
#[path = "runtime_paths_tests.rs"]
mod tests;
