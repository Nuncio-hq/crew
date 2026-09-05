# Projects release integration — 2026-09-05

Status: source integration complete; shared renderer integration gates still pending.
Work context: `crew-wt/upstream-0522`, Crew base `871eecb18d7a243d87ec56a2eb154fbf2099d7ce`, release target `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`.

## Released behavior integrated

- Resolved all 24 project conflict files. All 330 target/current project source and test paths present; missing-path audit found no omitted target source, and no deleted project file required restoration.
- Released exhaustive assignment-operation pagination and abort handling; lowercase issue-root matching, causal assignment ordering, richer task categories, managed-owner/self-service assignment UI. Retained Crew write authorization guard and owner/self assignment boundary.
- Released repository/task/review share links, origin discussion references, review activity details, retained selected-review/diff behavior, unavailable/retry states, contributor signed activity counts and signed Git identity matching. Retained Crew repository-participant precedence for heuristic contributor matches.
- Released file navigation/readme and inline files-changed sections. Retained Crew Wiki tab and pending Wiki file opens/line highlights. Retained diff-only badge helper; pending diffs cannot show repository snapshot file counts.
- Gate forwards repeated entity-navigation request and file-path inputs. ProjectChannelHome forwards thread root and resolving state to the existing ChannelScreenView; parent owns capability-gated route wiring.

## Explicit Crew divergence retained

Parent approved D-066 retention: ProjectsView keeps Crew outcome landing/list presentation and folder-first add callback. Released overview/context/agent rails do not replace that surface. ProjectDetailScreen retains outcome wrapper, all-member-repository ship log, repository selector, profile panel, and safe managed terminal action. Released repository/detail components render inside the existing plumbing disclosure; no new project conversation/context rail wired here. Released ProjectChannelHome is an existing channel renderer plus optional repository drawers, not another authoritative conversation model, and parent gates its route adoption.

Linked or invalid local workspaces stay forced to local read-only source: no branch/tag mutation, clone, push, pull, fetch, merge recovery terminal, or PR create/update actions. Released retained Git helper now forwards selectedTag to Crew local snapshot guard. Lazy file content now uses verified exact linked workspace parent/basename with no clone fallback, instead of looking up a managed checkout by repository d-tag. This closes an existing mismatch exposed by lazy file loading; tests prove another resolved folder is never read.

## Validation

- Biome: `../node_modules/.bin/biome check --write --config-path . src/features/projects` from desktop; 330 files checked, final clean check pending only routine formatting confirmation.
- TypeScript compiler API syntax+semantic scan scoped to project source: no project-owned errors; three shared-dependency diagnostics remain while parent resolves entityLink and Markdown (isLinkableCoordinate, blockCode, hardLineBreaks). Full tsc blocked by other pending renderer merge markers at run time.
- Focused protocol/contributor/discussion/folder safety suite: **69/69 pass**. Log `/tmp/crew-project-focused-final.log`.
- Broader project suite latest snapshot: **488 pass, 9 fail, 1 skip** (498 total). Remaining failures depend on in-progress shared entity-link/profile/Markdown modules and parent-owned message send guard source test. Log `/tmp/crew-project-all-tests.log`; rerun when parent shared merge completes. No failing tests ignored for final release gate.
- No browser screenshot/E2E claim. Parent owns final renderer build and UI verification.

## Coverage ledger references

Rows 10 (#6458), 24 (#6512), 28 (#6460), 44 (#6447), 85 (#6590), 100 (#6533), 115 (#6939), 121 (#6980), 136 (#7106), 170 (#7122) include project paths. Project scopes integrated; each also contains parent/other-owned paths, so no whole-row verified claim made here. This report also covers older-than-tag-baseline project behavior incorporated from the actual merge ancestor.

Docs impact: minor — this integration record documents release adoption and retained Crew behavior; parent owns final roadmap/changelog release update.

Unresolved: parent shared renderer integration, full typecheck/tests/build and visual verification; independent final reviewer gate.
