# Plan — #104 Hermes first-class operations

Spec: [#104](https://github.com/Nuncio-hq/crew/issues/104) — the issue body is the
authoritative spec (Phases 01–05, each with user flow, work, acceptance criteria,
and durable output). This file is the **execution plan only**.

Status: not started. No branch, no PR. Follow-up to #51 and #78 (both shipped).

## Phase ordering — the spec's order is right, with one caveat

```
01 contracts + trusted-autonomy boundary
02 Hermes Doctor / readiness
03 Needs You for decisions          ← overlaps #105, see below
04 Project Runner certification
05 Profile custody
```

**Phase 03 overlaps #105 / PR #108 directly.** Both define how a Hermes
clarification reaches Crew's `Needs You` surface and how answering resumes the same
turn. #105 Slice 2 ("Needs You survives and resumes") is already implemented in
PR #108, which touches `crates/buzz-acp/src/elicitation.rs`,
`crates/buzz-relay/src/handlers/ingest.rs`, `crates/buzz-sdk/src/builders.rs`, and
`desktop/src-tauri/src/commands/user_input.rs`.

**Consequence:** do not start Phase 03 until PR #108 lands. Starting it now means
two implementations of the same `46040 → 46041/46042` relationship, which is the
duplicate-model failure `CLAUDE.md` explicitly forbids. Phases 01, 02, 04, 05 are
independent of #108 and can proceed.

Recommended actual order: **01 → 02 → 04 → (wait for #108) → 03 → 05**.

## Phase 01 is the real gate

Phase 01 locks contracts and the trusted-autonomy boundary — a backend
authorization change (`respond-to anyone` rejection, remote/provider deployment
rejection for profile-bound agents). Everything after it assumes those rules hold.

Get this reviewed carefully:
- It changes **authorization**, which sits outside my standing merge authority
  (condition 4). Oscar approves this one explicitly.
- The spec is clear that **no permission approval UI** is added and that
  "questions are not permissions". Keep that boundary; it is a product decision,
  not an implementation detail.

## Crew-specific risks

- **Phase 02 (Doctor) must not leak secrets.** The spec's acceptance criterion —
  "diagnostics contain no credentials or raw secret-bearing environment values" —
  needs a test, not a review comment. `desktop/src-tauri/src/managed_agents/reserved_env_keys.rs`
  already exists as the seam; assert against it.
- **Phase 04 (Project Runner) touches the thread-worktree harness.** That harness
  has a documented history of hard failures: a foreign branch inside a harness
  worktree permanently blocks a thread. Any change to worktree provisioning must
  keep `git worktree add` on a **sibling** path and must never `checkout -b` inside
  a harness worktree.
- **A merged harness fix does not reach running agents.** Agents run the packaged
  binary from `/Applications/NuncioCrew.app`, not the source tree. Phase 04's
  acceptance criteria are only observable after a rebuild + app restart, which
  kills every running agent session. Plan that with Oscar rather than claiming
  "fixed on merge".
- **Phase 05 (export/restore) writes archives that may contain sensitive profile
  state.** The spec already requires a warning; also require that the archive path
  is never a repo directory.

## Verification bar

Per-phase acceptance criteria in the spec are the bar. Additions for this repo:

- **Mutation check** on every behavioural fix.
- Rust changes: `cargo test -p buzz-acp --lib` must be run with `BUZZ_ACP_*`
  stripped from the environment, or it manufactures false failures:
  ```bash
  ( for v in $(env | grep -o "^BUZZ_ACP_[A-Z_]*"); do unset $v; done; cargo test -p buzz-acp --lib )
  ```
- `desktop/src-tauri` is outside the root workspace:
  `cargo test --manifest-path desktop/src-tauri/Cargo.toml`.

## Open questions for Oscar

- Is #104 wanted before #102? Both are large and both land in the agent/thread
  surfaces. Running them concurrently will collide.
- Phase 02 lists an "upstream Hermes ask" — is there a channel to raise it, or does
  Crew work around whatever Hermes exposes today?
