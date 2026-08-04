# Phase 2 — Say when GitHub state is unavailable instead of rendering nothing

Status: **Ready** · Depends on: Phase 1 · Fixes acceptance criterion 3

## Problem

`availability` is computed in Rust and carried all the way into the frontend
types (`thread-workspace-types.ts:51` for the thread, `:85` for the registry),
and then **nothing reads it** — verified by grep across `desktop/src`.

Both the chips (`ProjectThreadWorkspacePanel.tsx:287`) and the GitHub row
(`:385`) are gated on `pullRequest` being non-null. So these two very different
situations render identically as empty space:

- `gh` could not run at all → user should fix their machine.
- `gh` ran fine, this branch has no PR → nothing is wrong.

That is what made the Phase 1 bug look like an unshipped feature for a whole
release cycle. Fixing the resolver without fixing this leaves the next
`gh` breakage — an expired auth token, a repo with no remote — equally silent.

## Approach

Widen the availability enum so the cause survives to the UI, then render one
muted chip when the state is degraded. Do not render PR/CI chips when `gh`
worked and there simply is no PR — that is a correct empty state and adding
noise to it would be worse than today.

## Decision — three states, not two

```rust
pub enum ThreadGitHubAvailability {
    Available,   // gh ran; pull_request is authoritative (Some or None)
    CliMissing,  // gh binary could not be resolved
    CliFailed,   // gh ran and failed — auth, no remote, network, rate limit
}
```

serde is already `rename_all = "kebab-case"` on this enum
(`thread_github.rs:15`), so the wire values are `available`, `cli-missing`,
`cli-failed`.

`CliMissing` comes from Phase 1's `GhUnavailable`. `CliFailed` is every other
`command_output` error. The distinction matters because the user action differs:
install `gh` versus `gh auth login` / check the remote.

## Files

Rust:

- `desktop/src-tauri/src/commands/thread_github.rs` — widen the enum; replace
  the single `unavailable()` helper with the two cases. `find_pull_request_number`
  and `read_pull_request` must return which failure occurred rather than a bare
  `Option`.
- `desktop/src-tauri/src/commands/project_worktree_registry_github.rs` and
  `project_worktree_registry.rs:70` — same widening for `GithubAvailability`.

Frontend types:

- `desktop/src/shared/api/thread-workspace-types.ts:51` and `:85` — widen both
  unions.

Frontend logic:

- `desktop/src/features/messages/lib/projectThreadGitHubStore.ts:73` — the
  `.catch` fallback currently invents `availability: "unavailable"`. An invoke
  that throws is a frontend/IPC failure, not a CLI failure; map it to
  `cli-failed` and leave a comment saying why.
- `desktop/src/features/messages/ui/useProjectThreadWorkspaceModel.ts` — the
  model drops `availability` today (`:114-115` keeps only `pullRequest`). Add it
  to `ProjectThreadWorkspaceModel` (`:19-34`) and to the memo dependency list
  at `:134` — a value added to the object but not the deps array goes stale and
  is exactly the `React.memo` trap called out in `CLAUDE.md` gotcha #7.

Frontend UI:

- `desktop/src/features/messages/ui/ProjectThreadWorkspacePanel.tsx` — the chip
  row at `:287`.
- `desktop/src/features/channels/ui/ChannelWorktreesPill.tsx:23-27` — the label
  drops `· M PRs open` when the count is 0, which is indistinguishable from a
  failed fetch.
- `desktop/src/features/channels/ui/ChannelWorktreesDrawer.tsx` — it renders
  `pending` (`:152`) and `error` (`:155`) states already; a degraded-GitHub note
  belongs next to those.

## Steps

1. Widen the Rust enums and thread the failure cause through. Keep
   `Available` + `pull_request: None` meaning exactly what it means today.
2. Widen the two TS unions. Compile — `tsc` will point at every site that needs
   a new branch.
3. Add `githubAvailability` to the workspace model, including the deps array.
4. Chip row: when availability is not `available`, render a single muted
   `GitHub` chip with `tone="idle"` whose `title` names the cause:
   - `cli-missing` → `GitHub CLI (gh) not found`
   - `cli-failed` → `GitHub CLI could not read this repo`
   Keep the existing PR and CI chips exactly as they are for the `available`
   case. Reuse `ChipButton` (`:56`) rather than adding a new control.
5. Channel pill: when the registry reports degraded GitHub, render
   `N worktrees · PRs unavailable` instead of silently dropping the segment.
6. Worktrees drawer: one line near the existing pending/error notices naming the
   same cause, so the drawer is not the one surface still lying by omission.

Keep copy to naming the cause. No install instructions, no retry button, no
link-out — those are scope the user has not asked for and each adds a support
surface. Revisit if it turns out naming the cause is not enough.

## Tests

- Rust unit: a `gh` failure produces `CliFailed`, an unresolvable binary
  produces `CliMissing`, and success with an empty PR list stays `Available`
  with `pull_request: None`. Extend the existing `mod tests` in
  `thread_github.rs:177`.
- Node unit (`*.test.mjs`, matching the repo's existing style): the pill label
  for each of the three availability states.
- E2E: extend `desktop/tests/e2e/project-thread-worktree.spec.ts` with a
  degraded-availability case driven through the mock bridge
  (`desktop/src/testing/e2eBridge.ts`). Build with `pnpm test:e2e:smoke`, never
  a plain `pnpm run build` — that strips the mock bridge and every mock-mode
  spec fails with a misleading `Cannot read properties of undefined`.

## Validation

```bash
just ci
cargo test --manifest-path desktop/src-tauri/Cargo.toml
cd desktop && pnpm test:e2e:smoke
```

Manual: with `gh` reachable, a thread whose branch has no PR must show **no**
GitHub chip — confirming the degraded affordance did not leak into the healthy
empty state.

Crew CI runs the desktop smoke e2e as an advisory signal only, and the suite is
known-flaky under load. Attribute any failure individually before treating it as
a regression; do not gate on a scoped run.

## Risk and rollback

Low, and mostly typing. The enum widening is source-compatible in Rust
(`ThreadGitHubAvailability` is `Serialize`-only, so no stored data decodes it)
and the TS side is caught at compile time. Blast radius verified by grep for
`"unavailable"` in `desktop/src`: 3 sites, all listed above.

Rollback is per-surface — each of the three UI touches is independent, so a
copy change that reads badly can be reverted without touching the enum.
