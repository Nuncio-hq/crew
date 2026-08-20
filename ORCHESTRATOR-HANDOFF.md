# Orchestrator handoff — issue #242

Builder phase: implementation only. No push, no PR, no merge.

Worktree: `/Users/a1241968/Desktop/Oscar/crew/.worktrees/issue-242-office-channel-relink`
Branch: `feat/issue-242-office-channel-relink` (local, ahead of `origin/main` by the commits below)

## Files touched

| Path | Kind |
| --- | --- |
| `desktop/src/features/channels/lib/channelLocalWorkspace.ts` | New Crew file |
| `desktop/src/features/channels/lib/channelLocalWorkspace.test.mjs` | New Crew test |
| `desktop/src/features/channels/ui/ChannelLocalWorkspaceChip.tsx` | New Crew file |
| `desktop/src/features/channels/ui/ChannelLocalWorkspaceChip.test.mjs` | New Crew test |
| `desktop/src/features/channels/ui/ChannelPane.tsx` | **Upstream-owned / shared.** Thin hook only: import + one `toolbarExtraActions` sibling. Already listed in UPSTREAM-SYNC (#187). Now 992 / 1000 lines. |
| `docs/crew/STATE.md` | Crew doc |
| `docs/crew/UPSTREAM-SYNC.md` | Crew doc (one #187 row note) |
| `plans/20260820-office-channel-relink/plan.md` | Plan of record |
| `ORCHESTRATOR-HANDOFF.md` | This file |

Not touched: `CrewProjectWorkspacePanel` (still unmounted), `ProjectThreadWorkspacePanel` Pick folder (#217 recover stays), sidebar rail, wiki Pick button, DECISIONS.md.

## What shipped

Exclusive-repo office channel shows a truncated bound path next to the composer workspace selector. Owner gets `Relink folder`. Click runs `chooseProjectWorkspaceFolder` then `linkCurrentProjectWorkspace` (existing owner-signed 30617 `buzz-location` publisher). Cancel picker is inert. Success toast: `Project workspace linked. Send a new message to use it.`

Non-owner sees the path, no button. `#general` / no exclusive binding / duplicate bindings / unlinked / missing path: chip hidden. Folder-mode cowork bindings are included. A still-tagged gone path still returns a binding (status stays `linked`).

No new Tauri command, event kind, or CLI. Frozen thread roots were not changed (V1 / #217).

## TDD

1. Helper test first → RED `ERR_MODULE_NOT_FOUND` for `channelLocalWorkspace.ts` → implement → 7/7 GREEN → commit `301c80816`.
2. Chip test first → RED `ERR_MODULE_NOT_FOUND` for `ChannelLocalWorkspaceChip.tsx` → implement view + chip → 6/7 GREEN, ChannelPane mount RED → wire one JSX sibling → 7/7 GREEN.
3. Strengthened source contract to `/<ChannelLocalWorkspaceChip/` so an unused import cannot fake the mount.
4. Sabotage: comment out the JSX mount → that test RED (`expected: /<ChannelLocalWorkspaceChip/`) → restore → GREEN.

## Tests

From worktree after `. ./bin/activate-hermit`. Desktop `node_modules` was missing; `just desktop-install` was required first (honest: first test attempt failed on missing `typescript`, not on the helper).

Required slice (plan Task 5):

```bash
cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test \
  src/features/channels/lib/channelLocalWorkspace.test.mjs \
  src/features/channels/ui/ChannelLocalWorkspaceChip.test.mjs \
  src/features/sidebar/lib/channelsOnlySidebar.test.mjs \
  src/features/projects/project-add-local-workspace-ui-contract.test.mjs
```

Result: **21 pass, 0 fail** (exit 0).

Repo command `cd desktop && pnpm test`:

Result: **5486 pass, 0 fail, 1 skipped** (exit 0, ~95s). The skip is pre-existing, not from this change.

Fmt: `pnpm exec biome check --write` on the touched TS/TSX/MJS paths. Clean.

Not run: `just ci`, desktop typecheck, Tauri tests, Gate C live Relink on the official app.

## Commits (local only)

| SHA | Subject |
| --- | --- |
| `301c808169bf696f7343c1ab3e0d75dd7ea32e33` | `test(desktop): exclusive channel local-workspace helper (#242)` |
| `962e806b4badad5d4a95d9a7e3d00eeb9ce1af66` | `feat(desktop): office-channel Relink folder chip (#242)` |
| `010b08fb58b1e8302060f8a3470acec7b0a03b1a` | `docs(crew): #242 office-channel relink note + sync row` |
| (this commit) | plan + this handoff |

DCO signed (`git commit -s`). Not pushed.

## Leftover risks

1. **No honest live gone-folder probe.** `probe_project_git_workspace` `is_git: false` is not gone (cowork folders). No existing cheap snapshot/probe was reused. The chip therefore always shows `Relink folder` and does **not** render the #217 sentence in the live UI. The view helper still accepts `pathMissing` if a later honest signal appears.
2. **Click path is a source contract**, not a mounted React click. Publish reuse is proven by calling the existing functions, not by a second publisher.
3. **ChannelPane is 992 / 1000 lines.** Next sibling in this file may trip D-022.
4. **Gate C** (open NuncioCrew office channel → Relink → pick `~/Desktop/Oscar/crew` → new ping works) is post-merge / founder try script. Not run here.
5. Did **not** extract a shared `relinkProjectWorkspaceFolder` helper. `#217` drawer still has its own pickFolder. Duplication is small; extracting would have been a refactor tour.

## Blockers

None for this phase. Ready for orchestrator review, then push/PR if accepted.

Issue DoD items that are **not** this phase: PR to `Nuncio-hq/crew`, NuncioCrew Gate green, Gate C items in the PR/thread.
