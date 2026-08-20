# Office-channel Relink folder — implementation plan

> **Issue:** https://github.com/Nuncio-hq/crew/issues/242
> **Branch / worktree:** `feat/issue-242-office-channel-relink` at `.worktrees/issue-242-office-channel-relink`
> **Where brief and plan differ, this plan wins.**

**Goal:** On an exclusive-repo office channel, the Project owner can Relink / Pick the local folder without pinging an agent and without restoring the Projects rail.

**Architecture:** Reuse the shipped 30617 `buzz-location` publisher (`linkCurrentProjectWorkspace`). Add a Crew-owned chip next to the existing composer workspace selector. Thin hook only in already-touched `ChannelPane` `toolbarExtraActions`.

**Tech stack:** Desktop React 19 + existing Tauri folder picker. No new kinds, no new HTTP, no new CLI.

---

## Locked decisions

- D-066 / #223: no Projects rail, no top-nav Projects.
- D-014: `CrewProjectWorkspacePanel` stays unmounted.
- #217 V1: frozen thread roots stay frozen. New messages after relink use the new path.
- D-003 / D-010: React is not authoritative; relink = owner-signed 30617.
- D-020: PR to `Nuncio-hq/crew` only.
- D-022: new files stay small; do not grow `ChannelPane.tsx` except one JSX sibling.
- No spike: publish contract already exists; founder chose the office-channel door.

## Seam

`desktop/src/features/channels/ui/ChannelPane.tsx` already mounts:

```tsx
toolbarExtraActions={
  <>
    <ComposerWorkspaceSelector ... />
  </>
}
```

Add `<ChannelLocalWorkspaceChip channelId={...} />` as a sibling. UPSTREAM-SYNC already allows this slot (#187).

Do **not** extend `gitProjectWorkspaceForChannel` to do relink. That helper hides `mode=folder` and requires `localWorkspaceStatus === "linked"`. Relink must also work for cowork folder-mode and for a still-tagged path whose directory is gone (status stays `linked`).

## Tasks

### Task 1: RED — exclusive-channel binding helper

**Files:**
- Create: `desktop/src/features/channels/lib/channelLocalWorkspace.ts`
- Create: `desktop/src/features/channels/lib/channelLocalWorkspace.test.mjs`

**Contract:**

```ts
export type ChannelLocalWorkspace = {
  repoAddress: string; // 30617:<owner>:<dtag>
  owner: string;
  dtag: string;
  localPath: string;
  workspaceMode: "git" | "folder";
};

export function exclusiveChannelLocalWorkspace(
  channelId: string | null | undefined,
  projects: readonly Project[] | undefined,
): ChannelLocalWorkspace | null;
```

Rules:
- One exclusive binding for the channel (`project.projectChannelId === channelId` or exactly one repository `channelId` match).
- Return binding when `localWorkspacePath` is a non-empty absolute path, even if the folder is gone.
- Return null for `#general` / no binding / duplicate bindings / missing path / `unlinked`.
- Include `workspaceMode === "folder"`.

Write the test first. Run:

```bash
cd desktop && node --test src/features/channels/lib/channelLocalWorkspace.test.mjs
```

Expected RED: module missing.

Then implement the helper. Expected GREEN.

Commit: `test(desktop): exclusive channel local-workspace helper (#242)`

### Task 2: RED — chip visibility + owner Relink

**Files:**
- Create: `desktop/src/features/channels/ui/ChannelLocalWorkspaceChip.tsx`
- Create: `desktop/src/features/channels/ui/ChannelLocalWorkspaceChip.test.mjs` (source-contract / render-contract in the style of `channelsOnlySidebar.test.mjs` if a full React mount is heavy; prefer a small pure-view helper + source contract)

Minimum assertions:
1. Hidden when `exclusiveChannelLocalWorkspace` is null.
2. Visible when bound; shows truncated `localPath`.
3. Owner sees a button labeled `Relink folder` (path exists) or `Pick folder` (path gone, if known) / default `Relink folder`.
4. Non-owner: path visible, no button.
5. Click path: `chooseProjectWorkspaceFolder` then `linkCurrentProjectWorkspace({ owner, currentPubkey, dtag, channelId, localPath })`.
6. Success toast: `Project workspace linked. Send a new message to use it.`
7. Cancel picker: no publish.
8. Source contract: `ChannelPane.tsx` mounts `ChannelLocalWorkspaceChip`; still no `onSelectProjects`.
9. Source contract: `crew-projects-screen.tsx` still does not include `CrewProjectWorkspacePanel`.
10. `channelsOnlySidebar.test.mjs` still passes.

Reuse:
- `chooseProjectWorkspaceFolder` from `desktop/src/shared/api/tauri-project-folder-dialog.ts`
- `linkCurrentProjectWorkspace` from `desktop/src/features/projects/lib/project-local-workspace-runtime.ts`
- `parseCrewRepoAddress` if useful (`desktop/src/features/messages/lib/projectThreadWorkspace.ts`)
- `useIdentityQuery`, `useProjectsQuery`, `useQueryClient`
- Invalidate `["crew-project-announcement"]` and whatever `useProjectsQuery` uses
- #217 copy `The Project folder is gone. Pick a workspace again.` only when a **cheap existing** snapshot/probe proves the path is missing. Do **not** treat `probe_project_git_workspace` `is_git: false` as gone (cowork folders are not git). If no honest gone signal, skip the sentence and still show Relink.

Do not add a new Tauri command.

Commit: `feat(desktop): office-channel Relink folder chip (#242)`

### Task 3: Wire ChannelPane (thin)

**Modify:** `desktop/src/features/channels/ui/ChannelPane.tsx` — import + one sibling in `toolbarExtraActions` only.

**Modify if needed:** `docs/crew/UPSTREAM-SYNC.md` — one line under the existing ChannelPane #187 row: chip sibling (#242).

**Modify:** `docs/crew/STATE.md` — short issue #242 note at the top (English).

Commit: `docs(crew): #242 office-channel relink note + sync row`

### Task 4: Keep #217 recover

Do not remove `ProjectThreadWorkspacePanel` Pick folder. Optional: extract a shared `relinkProjectWorkspaceFolder(...)` helper used by both chip and drawer so there is one publish path. Only extract if it shrinks duplication without a refactor tour.

### Task 5: Verify

From worktree, after `. ./bin/activate-hermit`:

```bash
cd desktop && node --test \
  src/features/channels/lib/channelLocalWorkspace.test.mjs \
  src/features/channels/ui/ChannelLocalWorkspaceChip.test.mjs \
  src/features/sidebar/lib/channelsOnlySidebar.test.mjs \
  src/features/projects/project-add-local-workspace-ui-contract.test.mjs
```

If desktop test runner is `pnpm exec node --test` / `just desktop-unit`, use the repo's existing command, not a new one.

Sabotage: comment out the ChannelPane mount; source-contract test must fail; restore.

Fmt/lint the files you touched (`just desktop-fmt` or biome on those paths). Do not run full `just ci` unless cheap.

## Non-goals

- Push, PR, merge (orchestrator).
- Restoring Projects rail / remounting Local workspace strip.
- Wiki Pick button.
- Unfreezing old thread roots.
- New event kinds / HTTP / CLI.
- Computer-use / screenshots unless already free from unit tests.

## Handoff

Write `ORCHESTRATOR-HANDOFF.md` at worktree root (committed): files touched (flag upstream), test commands + results, commits, leftover risks, honest blockers.
