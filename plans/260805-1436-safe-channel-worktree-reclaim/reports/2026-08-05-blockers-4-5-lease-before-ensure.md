# BLOCKERS 4+5 — lease before ensure + reattach self-heal

Status: DONE

## Changes

- `thread_workspace.rs`: `plan_thread_worktree`, `EnsureKind`, `ensure_planned_thread_worktree`; `ensure_thread_worktree` = plan then ensure
- `pool.rs`: plan → shared lease → ensure under lease; `checkout_mutated` forces session invalidate
- Tests in `pool.rs` + `thread_workspace_tests.rs`

## Flow

1. Discover plan (no create/reattach)
2. `try_acquire_shared` — Busy refuses before mutation
3. `ensure_planned_thread_worktree` under lease
4. If `EnsureKind != AlreadyPresent`, invalidate cached session even when gen/path/branch match

## Tests (all PASS)

- `resolve_and_bind_exclusive_lease_before_prompt_prevents_ensure_mutation`
- `resolve_and_bind_start_remove_race_refuses_or_reattaches_never_missing_cwd`
- `resolve_and_bind_invalidates_session_after_reattach_even_when_generation_matches`
- `test_workspace_binding_mismatch_true_when_checkout_was_mutated`
- `plan_thread_worktree_resolves_identity_without_creating_checkout`
- prior `resolve_and_bind_*` suite still green (8/8)

## Unresolved

- none
