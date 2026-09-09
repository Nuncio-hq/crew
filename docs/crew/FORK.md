# Crew as a fork of Buzz

This repository is [Nuncio-hq/crew](https://github.com/Nuncio-hq/crew), a
fork of [block/buzz](https://github.com/block/buzz). Product name:
**NuncioCrew** (Crew). This file is the single place for fork identity, the
upstream-sync runbook, and the fork-delta record. It is rewritten in place.

## Identity

| Name | Meaning |
| --- | --- |
| **NuncioCrew / Crew** | This fork's product, GitHub repo, CI (`NuncioCrew CI` / `NuncioCrew Gate`), releases, data dir `…/Application Support/NuncioCrew/` |
| **Buzz** | Upstream platform and protocol: relay, Nostr kinds, ACP harness, `buzz-*` crates, anything unchanged from `block/buzz` |
| `upstream` remote | `https://github.com/block/buzz.git`, fetch only; push URL disabled. **Never open PRs against block/buzz** (D-020) |
| `origin` remote | `https://github.com/Nuncio-hq/crew.git` |

Rules: say Crew for the fork, Buzz for upstream; do not mass-rename "Buzz"
in upstream docs; issues and PRs go to Nuncio-hq/crew. macOS data paths
`Buzz/` and `xyz.block.buzz.app/` may belong to a stock Buzz install — never
delete them from Crew automation.

## The fork in numbers (2026-09-07, pin `desktop-v0.5.23`)

- Upstream-owned files Crew modifies: **675** (`scripts/check-fork-delta.py`).
- Crew-only files under upstream directories: ~800.
- `crates/buzz-acp` is where Crew is *ahead* of upstream (~12k lines over
  30+ Crew commits: session ledger, worktree leases, receipts, recovery,
  Hermes, governor). Everywhere else Crew is a thin layer on upstream.

## How Crew stays mergeable

1. **Real ancestry.** Every upstream sync is a `git merge <tag>` with two
   parents, never a squash or a copy. `git merge-base HEAD upstream/main`
   must be the last synced tag. (Restored on 2026-09-07 after the 0.5.22
   "upgrade" had been a squash; see the merge commit for the resolution.)
2. **Upstream wins by default.** A conflict hunk is resolved to upstream
   unless a decision or issue names the Crew behavior. Then the smallest
   Crew hook is re-applied and the logic lives in a Crew-owned file (D-022).
3. **Areas, not files.** [`fork-delta.json`](fork-delta.json) records the
   fork by area glob with *why* and *how to resolve*. CI (`CI Policy` job)
   fails when an upstream-owned file changes and matches no area. Add the
   file to an area or add an area; do not silence the check.
4. **Numbering and identity are Crew's.** Migrations use Crew numbering
   (Crew inserted `0031_wiki_fts_allowlist`, so upstream `0031+` shift by
   one); product identity lives in `tauri.nuncio-crew*.conf.json`; the
   desktop version tracks upstream's.
5. **File-size ratchet.** Upstream growth in a shared file is recorded as
   the exact `wc -l` in `desktop/scripts/file-size-baselines.json` in the
   same sync PR; Crew growth is extracted (D-022). Never raise the limit.

## Sync runbook

```bash
git fetch --prune upstream 'refs/tags/desktop-v*:refs/tags/desktop-v*'
git switch main && git pull --ff-only origin main
git switch -c sync/upstream-YYYY-MM-DD
git merge --no-ff desktop-vX.Y.Z
```

Then, in order:

1. Resolve conflicts with rule 2. For a whole-file add/add (both sides
   added since the merge-base), 3-way merge against the previous tag:
   `git merge-file ours prev-tag theirs`.
2. Restore Crew-only intent that Git cannot see: files unchanged between
   the two upstream tags must stay at Crew's version; files Crew never
   touched must be upstream's. (The 2026-09-07 merge scripted both checks;
   reuse that approach.)
3. Rename incoming upstream migrations to Crew numbering; never keep both.
4. Update `upstream-buzz.json` (version, tag, exact commit) and the pin in
   `desktop/src/testing/nuncio-crew-release-contract.test.mjs`.
5. `scripts/check-fork-delta.py --list` — add areas for anything new.
6. Gates: `cd desktop && pnpm exec tsc --noEmit && pnpm test`;
   `cargo test -p buzz-acp --lib`; `cargo test --workspace --lib`;
   `pnpm --filter buzz check` (file sizes, channel-first IA).
7. For `crates/buzz-acp`: **cherry-pick upstream commits onto Crew**
   (`git cherry-pick -x <sha>`), do not take upstream's file. Upstream's
   thread-per-session model (#6732, `SessionPolicy` / `session_owners`) is
   not adopted; a change there needs a decision first.
8. Run `NuncioCrew Upstream Sync` on the branch, merge through a reviewed
   PR into `main`, disable any newly imported upstream workflows.

## CI

Normal PRs require one check, `NuncioCrew Gate` (`CI Policy` +
path-gated Desktop Fast / Desktop Rust / buzz-acp / macOS ARM Package /
Project Relay). Desktop Smoke E2E and Desktop E2E Integration are advisory
(D-032, D-047). `NuncioCrew Upstream Sync` is manual and runs the root Rust
gates. `NuncioCrew Release` is manual, signed, and checks the exact `main`
SHA. Inherited Buzz workflow files stay in the tree but are disabled at the
repository level.

## Channel-first IA guardrail (#278; successor tracked in #349)

D-078 / Stage 0 #344 accepts the demonstrated v0.9 Project/Wiki shell, with
Browse channels in the workspace menu. #349 owns the narrow Projects
guard update with production implementation. The present guard still runs
unchanged; the Workbench prohibition remains authoritative.

`pnpm --filter buzz check:channel-first-ia` currently rejects any sync hunk that
re-adds a Projects sidebar section, a Workbench picker, `onSelectProjects`,
`onSelectWorkbench`, or `selectedView: "workbench"`. Workbench routes stay
as redirects; the live-job desk lives in `LiveJobDesk`.

## Conflict policy for repeatedly-conflicting upstream files

Re-read upstream intent → reapply only the smallest Crew hook → move new
logic into a Crew file → run the upstream test and the Crew contract test →
record a decision if the maintenance boundary moved. Never resolve by
keeping "ours" wholesale.

## Recovery

If a sync branch becomes confused, keep it, create a fresh branch from
clean `main`, and compare. Destructive Git operations on a dirty checkout
need explicit founder approval.
