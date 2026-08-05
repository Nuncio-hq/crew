# Audit — Crew surfaces against the "build on top of Buzz" rule

Date: 2026-08-05. Upstream compared: `upstream/main` = `8342dfcc5`
(`desktop-v0.5.5`). Crew compared: `origin/main` = `971dffdf1`.
Fork point / current pin: `3a96acea0` (`desktop-v0.5.3`,
`docs/crew/upstream-buzz.json`).

Question asked: for each Crew surface, does it sit **on top of** a Buzz model,
and what does upstream's multi-repo landing (#4671) do to it?

## The decisive structural finding

Upstream #4671 **split** the type Crew calls `Project` into two
(`desktop/src/features/projects/projectModels.ts`):

| Upstream type | Kind | Carries |
|---|---|---|
| `Repository` | `30617` | `cloneUrls`, `dtag`, `defaultBranch`, `repoAddress` |
| `Project` | `30621` | `repositoryAddresses[]`, `repositories[]`, `primaryRepositoryAddress`, `legacy` |

Crew's local `Project` type (`desktop/src/features/projects/hooks.ts:69`)
carries `cloneUrls` **and** `localWorkspacePath`. Those fields map onto
upstream's **`Repository`**, not upstream's new `Project`.

**So: Crew's "Project" ≡ upstream's `Repository`.** Every Crew per-repo
feature is semantically correct under NIP-MP — it just needs re-homing from
Crew's `Project` onto upstream's `Repository`.

### The migration ramp already exists

`repositoryToLegacyProject(repository: Repository): Project`
(`projectModels.ts:368`) wraps a bare `30617` repository into a synthetic
project with `legacy: true` and `repositories: [repository]`. Existing Crew
projects therefore keep working after the sync as single-repository legacy
projects. This is the on-ramp; Crew should ride it rather than build one.

## Per-surface audit

| Surface | Sits on a Buzz seam? | Multi-repo impact | Action |
|---|---|---|---|
| Local workspace path (`buzz-location` tag) | **Yes** — tag on existing kind `30617`, per D-010 | None semantically: the tag is per-repository, which is what NIP-MP wants | Re-home the type from `Project` to `Repository` |
| Worktree-per-thread registry | Partly — keys off filesystem path + `common_git`, not a Nostr coordinate | Needs a rule for *which* repository a thread binds to | Default to `primaryRepositoryAddress`; confirm with product |
| GitHub PR/CI chips | Partly — resolves the `origin` remote of a checkout (`thread_github_target.rs:5`) | Same ambiguity: N repos ⇒ N origins | Same rule as worktrees |
| ACP additions (elicitation, retry-turn, stop, edit-as-undo) | **Yes** — new modules wired into `buzz-acp/src/lib.rs` | None | Keep |
| Crew CI (`nuncio-crew-ci.yml`) | Separate by design (D-017) | None | Keep; see CI note below |

### Single-repo assumption to fix

`desktop/src/features/projects/lib/projectLocalRepos.ts:32` —
`project.cloneUrls[0]`. Under the new model the caller must first choose a
repository from `project.repositories[]`, then read that repository's
`cloneUrls`.

## Terminology collision (docs-level, not code-level)

- **Code is already correct.** `desktop/src/shared/constants/kinds.ts:62`
  defines `KIND_REPO_ANNOUNCEMENT = 30617`; `projectIssues.mjs:144` says
  "Issue repo address must reference a kind:30617 repo".
- **Docs are not.** `docs/crew/DECISIONS.md` D-003 and D-010 call kind `30617`
  a "Project". With `KIND_PROJECT = 30621` now real, that wording collides
  with upstream.

Recommend a docs-only clarification on D-003/D-010 stating that "Project"
there means kind `30617` = upstream's *repository*. Do **not** rewrite the
decisions themselves — the mechanism they describe is still correct.

## Thin-fork budget: measured drift

`docs/crew/UPSTREAM-SYNC.md` sets the normal UI integration budget at one
route-registration edit and one navigation-entry edit, and requires explicit
justification for other upstream-file edits.

Measured (`git diff --diff-filter=M upstream/main...origin/main`):
**113 upstream files modified.** Largest: `crates/buzz-acp/src/lib.rs`
(+897/−96, of which only ~10 added lines are test attributes),
`managed_node_paths.rs` (+619/−16), `buzz-acp/src/pool.rs` (+543/−62).

This is not automatically a violation — the rule permits justified edits, and
`DECISIONS.md` records D-001…D-018+ covering much of this work. But the gap
between "one route edit" and 113 files is large enough that it should either
be re-stated as a realistic budget or reconciled decision by decision. Flagged
for Oscar, not actioned.

## CI note

Upstream `ci.yml` gained three guards since the pin that protect exactly the
model Crew is adopting:

- workspace-profile `kind:9033` gate tests
- NIP-MP coordinate-deletion guard (never delete a head newer than the
  tombstone)
- `e2e_project`

Crew's inherited `CI` workflow is `disabled_manually` by deliberate decision
(D-017, `docs/crew/CI.md`), verified via `gh workflow list --all`. Adopting
upstream CI wholesale would reverse that decision and pay for platforms Crew
does not ship. Recommend instead picking up these three guards in the manual
`NuncioCrew Upstream Sync` lane.

Issue #41 (path-gated `Desktop Rust` job) is **already approved and delegated**
— it is not an open question. These three guards are additive to it and belong
in the Upstream Sync lane, not in the `Desktop Rust` gate.

## Not verified

- Have not read all 30 conflicting files individually; the
  "take upstream, re-apply Crew delta" rule remains a proposal, not a measured
  cost.
- Have not run any build or test at this commit. Every claim here is from
  reading source at the two revisions named at the top.
- Have not confirmed which repository a Crew thread *should* bind to when a
  project has several — that is a product decision, not a code fact.

## Open questions

1. When a project has N repositories, does a Crew thread worktree bind to
   `primaryRepositoryAddress`, or should the thread choose?
2. Should the thin-fork budget in `UPSTREAM-SYNC.md` be re-stated to match the
   measured 113-file reality, or should the drift be reconciled?
3. Should the three upstream `ci.yml` guards above go into the manual
   `NuncioCrew Upstream Sync` lane as part of this sync, or wait until after
   the approved `Desktop Rust` job (#41) lands?
