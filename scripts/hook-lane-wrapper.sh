#!/bin/sh
# Run a selected Lefthook lane without the invoking Git command's repository
# context. Keep this boundary after file selection: the parent hook still needs
# its Git environment for discovery, diff filtering and staging in other hooks.
set -eu

fail() {
  echo "hook-lane-wrapper: $1" >&2
  exit 1
}

[ "$#" -gt 0 ] || fail "missing lane command"
local_vars=$(git rev-parse --local-env-vars 2>/dev/null) || fail "cannot discover local Git environment"
# Validate the entire list before clearing anything. Never echo its contents.
found_git_dir=false
while IFS= read -r name; do
  case "$name" in
    GIT_*) ;;
    *) fail "invalid local Git environment list" ;;
  esac
  case "$name" in
    *[!A-Z0-9_]*) fail "invalid local Git environment name" ;;
  esac
  [ "$name" != GIT_DIR ] || found_git_dir=true
done <<EOF_VARS
$local_vars
EOF_VARS
[ "$found_git_dir" = true ] || fail "local Git environment list lacks GIT_DIR"
while IFS= read -r name; do
  unset "$name"
done <<EOF_VARS
$local_vars
EOF_VARS
# These invoking-command overrides can also redirect fixture Git operations.
unset GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT

cwd=$(pwd -P) || fail "cannot resolve lane directory"
root=$(git rev-parse --show-toplevel 2>/dev/null) || fail "lane directory is not a repository"
root=$(CDPATH= cd -- "$root" && pwd -P) || fail "cannot resolve repository root"
[ "$root" = "$cwd" ] || fail "lane must run from the repository root"
exec "$@"
