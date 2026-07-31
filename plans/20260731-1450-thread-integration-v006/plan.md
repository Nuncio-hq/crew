# Thread integration strip + worktree lifecycle — v0.0.6

- **Status:** In progress — Phases 01–03 complete
- **Base:** `40773ea6d` (`crew-v0.0.5`, `origin/main`)
- **Branch:** `buzz/eb791333c0ee` (thread worktree, already fast-forwarded to base)
- **Release:** stable `v0.0.6` → immutable tag `crew-v0.0.6`
- **Owners:** plan Claude Opus · implementation Codex GPT 5.6 · review Cursor Grok High Fast

## Outcome

A Project thread reads as one feature branch: who is working, which worktree,
and the GitHub state of its PR — in two compact rows instead of three stacked
cards, with destructive lifecycle actions available and guarded.

## Accepted decisions

| Decision | Value | Source |
| --- | --- | --- |
| Strip layout | 2 rows × 3 columns | Oscar, prototype review |
| Colors | existing `theme.css` tokens + Tailwind stock only; no new color vars | Oscar |
| GitHub row visibility | only once the thread branch has a PR | Oscar |
| Lifecycle actions | `Close PR`, `Delete branch`, `Remove worktree` | Oscar |
| Worktree base | `origin/<default-branch>` after fetch; local HEAD only as fallback | Oscar |
| Handoff list | must include agents mentioned in replies, not just the thread root | Oscar |
| Devin | add catalog entry now even though the CLI is not in use yet | Oscar |
| Release channel | stable | Oscar |

## Non-goals

- No manual thread↔issue linking UI. Issue is derived from the PR.
- No change to the panel-hidden-for-non-authors behavior (`MessageThreadPanel.tsx:517-523`) — Oscar deferred it.
- No new HTTP endpoints. GitHub data comes from the local `gh` CLI.

## Phases

| Phase | Title | File |
| --- | --- | --- |
| 01 | Worktree base from origin + lifecycle commands | [phase-01-worktree-base-and-lifecycle.md](phase-01-worktree-base-and-lifecycle.md) |
| 02 | Thread integration strip (2×3) + handoff from replies | [phase-02-thread-integration-strip.md](phase-02-thread-integration-strip.md) |
| 03 | Provider avatars for preset harnesses + Devin | [phase-03-preset-avatars.md](phase-03-preset-avatars.md) |
| 04 | Local release build + computer-use verification | [phase-04-release-verification.md](phase-04-release-verification.md) |

Phases 01–03 can land as separate commits on one branch and one PR.
Phase 04 gates merge.

## Acceptance criteria

1. A thread opened while the source checkout is behind `origin/main` still cuts
   its worktree from the fetched remote tip.
2. The workspace panel renders as two 3-column rows; every cell opens a detail
   drawer; no new color token is introduced.
3. An agent mentioned only in a reply appears in the handoff list, labelled as
   added in a reply, and reaches `working` then `done`.
4. `Close PR` / `Delete branch` / `Remove worktree` each confirm first, and
   `Remove worktree` refuses while the worktree has uncommitted changes.
5. Creating an agent on any preset harness gives it a real vendor avatar.
6. `just ci` green; `cargo test --manifest-path desktop/src-tauri/Cargo.toml`
   green; desktop e2e smoke green.
7. A local release build is installed and exercised through computer use before
   merge.

## Prototype

`plans/260731-1425-thread-integration-strip/prototype.html` — layout and
interaction reference only. **Its colors and avatars are not authoritative**:
it invented `--ok` / `--warn` tokens and placeholder gradient avatars. Use the
app's own tokens and the real logo registries instead.

## Unresolved questions

None. Devin's installed official CLI verifies `devin acp` as an ACP stdio
server.
