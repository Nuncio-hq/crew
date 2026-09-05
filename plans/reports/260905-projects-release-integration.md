# Projects release integration — 2026-09-05

Status: source integration complete; independent root review passed. Focused Projects browser verification passed.
Work context: `crew-wt/upstream-0522`, Crew base `871eecb18d7a243d87ec56a2eb154fbf2099d7ce`, release target `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`.

## Released behavior integrated

- Resolved all 24 project conflict files. All 330 target/current project source and test paths present; missing-path audit found no omitted target source, and no deleted project file required restoration.
- Released exhaustive assignment-operation pagination and abort handling; lowercase issue-root matching, causal assignment ordering, richer task categories, managed-owner/self-service assignment UI. Retained Crew write authorization guard and owner/self assignment boundary.
- Released repository/task/review share links, origin discussion references, review activity details, retained selected-review/diff behavior, unavailable/retry states, contributor signed activity counts and signed Git identity matching. Retained Crew repository-participant precedence for heuristic contributor matches.
- Released file navigation/readme and inline files-changed sections. Retained Crew Wiki tab and pending Wiki file opens/line highlights. Retained diff-only badge helper; pending diffs cannot show repository snapshot file counts.
- Gate forwards repeated entity-navigation request and file-path inputs. ProjectChannelHome forwards thread root and resolving state to the existing ChannelScreenView; parent owns capability-gated route wiring.

## Explicit Crew divergence retained

Parent approved D-066 retention: ProjectsView keeps Crew outcome landing/list presentation and folder-first add callback. Released overview/context/agent rails do not replace that surface. ProjectDetailScreen retains outcome wrapper, all-member-repository ship log, repository selector, profile panel, and safe managed terminal action. Released repository/detail components render inside the existing plumbing disclosure. The restored repository actions panel is a sibling with profile-panel priority; legacy project chat/controller surfaces remain unmounted. Released ProjectChannelHome is an existing channel renderer plus optional repository drawers, not another authoritative conversation model, and parent gates its route adoption.

Linked or invalid local workspaces stay forced to local read-only source: no branch/tag mutation, clone, push, pull, fetch, merge recovery terminal, or PR create/update actions. Released retained Git helper now forwards selectedTag to Crew local snapshot guard. Lazy file content now uses verified exact linked workspace parent/basename with no clone fallback, instead of looking up a managed checkout by repository d-tag. This closes an existing mismatch exposed by lazy file loading; tests prove another resolved folder is never read.

## Restored Projects surfaces

- `desktop/src/features/projects/ui/ProjectsView.tsx:914` renders the existing `ProjectsChannelsList` for Channels, preserving linked-channel grouping and labels.
- `desktop/src/features/projects/ui/ProjectDetailScreen.tsx:778` restores the existing selection provider and repository actions panel. Reuses current branch/source controls, repository snapshot, issue/PR selections, create-dialog request counters, persisted collapse state, and context width. Profile panel retains priority. File remains 999 lines.
- `ProjectRightPanelControls.tsx` accepts `repositoryOnly`; the detail screen fixes mode to repository, so previously saved chat mode cannot remount a legacy chat surface.
- `useProjectDiscussInChannel.ts:53` routes repository discussion and Ask for access through the existing channel composer. Draft merging preserves existing text, creation timestamp, mentions, attachment metadata, and spoiler metadata. No auto-send or auto-join.
- `projectShareLinks.ts` accepts an optional repository tab; fallback discussion context retains the current tab, including Commits, in its share link.
- `projectSelection.ts:188` intersects linked candidates with `useChannelsQuery` member channels in repository → project → selected-item order. An inaccessible repository channel can fall back to an accessible project or selection channel; unrelated channels are never chosen.
- The discussion hook checks channel-query availability before writing drafts. Pending/error data produces a retry toast. No accessible linked candidate produces “No accessible channel is linked to this repository.” and stays on the repository without writing a draft.

## Validation

- Final source TypeScript: PASS (`/tmp/crew-project-discussion-tsc.log`).
- Focused Biome and `git diff --check`: PASS.
- Existing detail-selection, selection, related-channel, and share-link tests: 34/34 PASS before the final membership guard (`/tmp/crew-project-surfaces-unit.log`).
- Final selection suite: 13/13 PASS, including two new tests covering all accessible fallback levels and rejecting unrelated/inaccessible destinations (`/tmp/crew-project-discussion-selection.log`).
- Earlier protocol/contributor/discussion/folder safety integration suite: 69/69 PASS (`/tmp/crew-project-focused-final.log`).
- Independent root source review: PASS for restored panel/profile priority/selection/discussion wiring, optional share-link tab, and final membership guard.
- Final full Projects suite: **8/8 PASS in 23.5s**. Covers Channels, repository actions, issue/PR dialogs, narrow layout, reachable fallback and unknown-channel toast/no-navigation/no-draft behavior. The restricted fixture separates repository access from project home membership; the link check asserts the rendered chip `data-href`. See [CI reconciliation report](tester-260905-release-ci-reconciliation.md).
- Broad Projects/huddle pair passed 2/2 in 12.9s; full huddle suite passed 24/24 in 37.6s.
- Final independent membership-guard review and four mounted regressions passed. Frozen desktop suite passed 7012 with one existing skip and zero failures (7013 total, 98.6s), `/tmp/crew-final-js-gate-pool.log`.

## Coverage ledger references

Rows 10 (#6458), 24 (#6512), 28 (#6460), 44 (#6447), 85 (#6590), 100 (#6533), 115 (#6939), 121 (#6980), 136 (#7106), 170 (#7122) include project paths. Project scopes integrated; each also contains parent/other-owned paths, so no whole-row verified claim made here. This report also covers older-than-tag-baseline project behavior incorporated from the actual merge ancestor.

Docs impact: minor — this integration record documents release adoption and retained Crew behavior; parent owns final roadmap/changelog release update.

Final source-freeze checks (all guards), TypeScript and diff checks passed.

Unresolved: new-head remote acceptance with the coordinator. No unresolved source-review finding.
