//! Shebang resolution and the hard-link fence.

#![cfg(unix)]

use super::*;
use std::os::unix::fs::PermissionsExt;

fn script(directory: &std::path::Path, name: &str, body: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, body).expect("script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).expect("mode");
    path
}

#[test]
fn a_native_binary_has_no_interpreter_directory() {
    assert_eq!(interpreter_directory(Path::new("/bin/echo")), None);
}

/// An npm-installed `claude` is `#!/usr/bin/env node`. The directory that has
/// to be reachable is `node`'s, not `env`'s — resolving only as far as
/// `/usr/bin/env` leaves the runtime unable to start, which surfaces as what
/// looks like a model failure. Removing the `env` branch in
/// `resolved_interpreter` makes this return `/usr/bin`.
#[test]
fn an_npm_style_shim_resolves_through_env_to_the_real_interpreter() {
    let directory = tempfile::tempdir().expect("temp");
    let interpreter_dir = directory.path().join("node-bin");
    std::fs::create_dir(&interpreter_dir).expect("bin");
    let interpreter = script(&interpreter_dir, "buzz-test-node", "#!/bin/sh\nexit 0\n");
    assert!(interpreter.exists());

    let shim = script(
        directory.path(),
        "claude",
        "#!/usr/bin/env buzz-test-node\nconsole.log('hi');\n",
    );
    // An explicit search path, not this process's own: mutating `PATH` here
    // would race every other test that reads it.
    let resolved =
        interpreter_directory_with_path(&shim, Some(interpreter_dir.clone().into_os_string()));
    assert_eq!(
        resolved,
        Some(interpreter_dir.canonicalize().expect("canonical")),
        "the interpreter's own directory must be resolved"
    );
}

#[test]
fn a_relative_or_absent_shebang_resolves_to_nothing() {
    assert_eq!(shebang_interpreter(b"#!node\n"), None);
    assert_eq!(shebang_interpreter(b"not a script\n"), None);
    assert_eq!(shebang_interpreter(b""), None);
    assert_eq!(
        shebang_interpreter(b"#!/usr/bin/perl\n"),
        Some("/usr/bin/perl".to_owned())
    );
    // A non-ASCII shebang is refused rather than decoded.
    assert_eq!(shebang_interpreter(b"#!/usr/bin/\xff\n"), None);
}

/// A hard link inside the run root aliases an inode outside it, so the policy's
/// run-root write allowance becomes a write allowance for that outside file.
/// Removing the `nlink() > 1` check lets the run start with one in place.
#[test]
fn a_hard_link_into_the_run_root_is_refused_before_spawn() {
    let directory = tempfile::tempdir().expect("temp");
    let root = directory.path().join("run-root");
    std::fs::create_dir(&root).expect("root");
    std::fs::create_dir(root.join("nested")).expect("nested");
    std::fs::write(root.join("nested/ordinary"), b"fine").expect("ordinary");
    assert_eq!(assert_no_hard_links(&root), Ok(()));

    let outside = directory.path().join("outside-secret");
    std::fs::write(&outside, b"employee-secret").expect("outside");
    std::fs::hard_link(&outside, root.join("nested/aliased")).expect("hard link");
    assert_eq!(
        assert_no_hard_links(&root),
        Err(PrivateAskFailure::InvalidState)
    );
}

/// A symlink is not a hard link: it cannot alias an inode past the policy,
/// which matches paths. Treating it as one would refuse ordinary run roots.
#[test]
fn a_symlink_is_not_a_hard_link() {
    let directory = tempfile::tempdir().expect("temp");
    let root = directory.path().join("run-root");
    std::fs::create_dir(&root).expect("root");
    let outside = directory.path().join("outside");
    std::fs::write(&outside, b"bytes").expect("outside");
    std::os::unix::fs::symlink(&outside, root.join("link")).expect("symlink");
    assert_eq!(assert_no_hard_links(&root), Ok(()));
}
