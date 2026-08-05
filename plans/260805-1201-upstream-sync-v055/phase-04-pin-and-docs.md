# Phase 4 — Move the pin and correct the docs

Status: **Ready** · Depends on: Phase 2, Phase 3 · Fixes acceptance criteria 1 and 2

## Problem

`docs/crew/upstream-buzz.json` still names `desktop-v0.5.3`. Separately,
`docs/crew/DECISIONS.md` D-003 and D-010 call kind `30617` a "Project" — wording
that predates NIP-MP and now collides with upstream's `KIND_PROJECT` = `30621`.

Crew's **code** is already correct (`desktop/src/shared/constants/kinds.ts:62`
defines `KIND_REPO_ANNOUNCEMENT = 30617`). This is a prose fix only.

## Approach

Move the pin. Add one clarifying sentence to D-003 and D-010 — **do not rewrite
the decisions.** The mechanism they describe (repository identity is
`(pubkey, identifier)`; local path is a `buzz-location` tag on the existing
`30617` announcement) is still correct under NIP-MP, and rewriting an accepted
decision to match new vocabulary would quietly restate a choice nobody asked to
revisit.

Record the `primaryRepositoryAddress` binding as a new decision, since it is a
product choice Phase 2 depends on.

## Files

Modify:

- `docs/crew/upstream-buzz.json` — `0.5.5` / `desktop-v0.5.5` / `8342dfcc5`.
- `docs/crew/DECISIONS.md` — clarifying sentence on D-003 and D-010; append the
  new repository-binding decision.
- `docs/crew/UPSTREAM-SYNC.md` — state that the sync target is a **published
  release tag**, verified with `gh release list --repo block/buzz`, never
  `upstream/main`.

## Steps

1. Update the pin JSON to the exact tag and commit merged in Phase 1.
2. On D-003 and D-010, add: *"'Project' here means kind `30617`, which upstream
   names `KIND_GIT_REPO_ANNOUNCEMENT` — a repository. NIP-MP's `KIND_PROJECT` is
   `30621`."*
3. Append the new decision: a Crew thread worktree binds to
   `primaryRepositoryAddress`; `legacy: true` projects fall back to
   `repositories[0]`.
4. In `UPSTREAM-SYNC.md`, record the release-tag rule and the mobile caveat:
   `mobile-v0.8.0-rc.*` are tags with no published release and arrive inside the
   desktop release commit. They cannot be filtered without forking upstream
   code. Crew CI excludes Flutter, so this is an accepted exposure.

## Validation

- `docs/crew/upstream-buzz.json` names `desktop-v0.5.5` / `8342dfcc5`.
- `gh release list --repo block/buzz` shows that tag as a published release.
- No occurrence of `30617` in `docs/crew/` describes it as a project without the
  clarifying note.

## Risk

Low — documentation and one JSON pin.

## Rollback

Revert the commit.
