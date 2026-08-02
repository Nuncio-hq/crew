# Phase 2 — Channel timeline workspace badges

Depends on: Phase 1 (registry command + store).
Delivers: branch / PR / diff chips on project-thread summary rows, so the channel
timeline answers "which thread has a PR" without opening each thread.

Today the row says only `3 replies · last reply 12m ago`
(`MessageThreadSummaryRow.tsx:243-276`).

## Target rendering — layout A (chips), decided by Oscar (D6)

```
(avatars) 7 replies · last reply 1m · ⎇ brainstorm quản lí worktree · #21 ⏳ · #22 · +2 · +486 −12
```

Chips join the **existing** summary-row flex line. Layouts B (separate status
line) and C (bordered card) were rejected in `ui-preview.html`: A is the only
one that adds no vertical height to a channel full of project threads.

Rules:
- A chip renders only when it carries information. No worktree → no chips at all,
  and the row is byte-identical to today.
- No new row and no height change: chips join the existing flex line, `text-2xs`,
  `text-muted-foreground/70`, and the whole row keeps `min-w-0` truncation.
- **Worktree chip shows the derived label** (D7), capped at ~15rem with
  ellipsis; `title` carries the full branch. No label → the branch short id in
  mono, exactly as today's thread strip renders it.
- **PR chips: two, then `+N`.** The registry already returns
  `pull_requests` ranked (Phase 1 step 5), so the badge takes `slice(0, 2)` and
  emits an overflow count. Never render more than two: three PRs plus a label
  plus a diff chip is where the row starts wrapping at 1280px.
- PR chip colour follows state: open/green checks, draft/muted, failing/red,
  merged/violet. Reuse whatever `ProjectThreadGitHubRow.tsx` already uses for
  PR state styling rather than inventing a second palette.
- The chips are inside the existing `<button>`, so a click still opens the
  thread — no nested interactive elements (a nested `<a>` to the PR would break
  the button semantics; the PR link lives in the thread strip and the drawer).
  The `+N` chip is therefore **not** separately clickable in v1: opening the
  thread is the way to see the rest.

## Files

| File | Change |
| --- | --- |
| `desktop/src/features/messages/lib/projectThreadLabel.ts` | **new** — thread root body → display label or `null` |
| `desktop/src/features/messages/lib/projectThreadBadge.ts` | **new** — pure builder: registry entry + label → chip list |
| `desktop/src/features/messages/lib/useProjectThreadBadge.ts` | **new** — hook: message → badge or `null` |
| `desktop/src/features/messages/ui/MessageThreadSummaryRow.tsx` | render optional `badge` prop |
| `desktop/src/features/messages/ui/TimelineMessageList.tsx` | pass the badge at line 802 |
| `desktop/src/features/messages/ui/MessageThreadPanel.tsx` | pass the badge at lines 644 and 773 |
| `desktop/src/features/messages/lib/projectThreadLabel.test.mjs` | **new** — derivation tests |
| `desktop/src/features/messages/lib/projectThreadBadge.test.mjs` | **new** — builder tests |

## Label derivation (D7)

A project thread root is built by
`buildProjectChannelAgentMessage` as a Markdown **link reference definition**
followed by the human's text
(`desktop/src/features/projects/lib/project-channel-agent-context.ts:135-141`):

```
[buzz-project-context-<uuid>]: <buzz://project-workspace?…> "<title>"

@Claude Opus bây giờ hãy giúp mình brainstorm, để chúng ta …
```

`projectThreadLabel.ts` therefore:

1. drops leading lines matching `^\[[^\]]+\]:\s*<` (link reference definitions —
   this is also why the marker is invisible in the rendered timeline);
2. drops leading `@Name` mentions, using the same mention text the composer
   writes, so the label starts at the first real word;
3. collapses whitespace, takes the first line, truncates on a word boundary;
4. returns `null` when nothing usable remains (image-only root, mentions-only
   root) — the caller then falls back to the branch short id.

Pure function, no store, no I/O: identical output on every member's machine and
available before any agent has run.

## Data path

1. `message.body.indexOf("buzz://project-workspace?")` — cheap reject for the
   ~99 % of rows that are not project threads. Only then call
   `parseProjectThreadContext` (`projectThreadWorkspace.ts:33`), memoized per
   message id.
2. `context.localPath` → registry store keyed by canonical repo path (Phase 1).
3. `getProjectWorktreeEntryByRoot(repoPath, message.id)` — the thread root id
   **is** the summary row's message id.
4. `projectThreadLabel(message.body)` → label or `null`.
5. `buildProjectThreadBadge(entry, label)` →
   `{ label, branch, shortBranch, pullRequests, overflow, diff } | null`.

Perf discipline (CLAUDE.md gotcha #7): the hook must return a reference-stable
object or `null`. The registry store returns entries from a `Map` that is
replaced on refresh, so wrap the derived badge in
`shared/hooks/useStableReference.ts` content-equality caching, otherwise every
registry tick re-renders every summary row and defeats `React.memo` on the
timeline.

## Accepted v1 limitations

- **Zero-reply project threads show nothing.** `buildTimelineThreadSummary`
  returns `null` when `descendantCount === 0` (`threadPanel.ts:215-219`), so
  there is no row to hang chips on, and layout A puts the chips inside that row.
  A worktree only exists after an agent has run and an agent run produces a
  reply, so the gap is the few seconds of a first turn. Resolved question 3.
- **Other members see no chips** unless the project repo path resolves on their
  machine (D2). This is a silent, deliberate degrade — no "unavailable" state.
- **Only two PR chips**, then `+N`. The full list lives in the thread strip and
  the Phase 3 drawer.
- The `+N` chip is inert (see the nested-interactive rule above).

## Tests

`projectThreadLabel.test.mjs`:
- a real project-thread root (link reference line + blank line + `@Name` + text)
  → the text, mention and marker stripped
- multiple leading mentions → all stripped
- image-only / mentions-only root → `null`
- a root with no project-context marker at all → still derives from the body
- long root → truncated on a word boundary, no mid-word cut
- CRLF and multiple blank lines do not break the split

`projectThreadBadge.test.mjs`:
- entry without PR → worktree chip only
- open PR with green rollup → `#42 ✓` + diff chip
- draft PR → no check glyph, draft styling flag
- **four PRs → exactly two chips plus `overflow: 2`, ranked open first**
- `github: unavailable` → worktree chip only, no PR chip
- `label: null` → chip falls back to the branch short id, mono flag set
- entry with `root_event_id: null` never reaches the builder (guarded upstream)

Existing timeline tests must still pass unchanged for non-project rows — that is
the regression guard for "no layout change".

## Validation

```bash
just desktop-test
just desktop-check
pnpm --dir desktop check:px-text
```

Visual check with the E2E screenshot path (CLAUDE.md § PR Screenshots):

```bash
just desktop-screenshot --name thread-badge --messages /tmp/project-thread.json
```

Seed a mock message whose body contains a `buzz://project-workspace?` URL plus a
reply, then confirm the badge row against a baseline capture of the same channel
without the badge. Hash-distinctness rule applies before posting anything.

## Risk / rollback

- Highest risk is render churn on a busy channel; mitigated by the cheap
  `indexOf` reject and the stable-reference wrapper. If lag appears, measure with
  DevTools closed (gotcha #7) before changing anything.
- Rollback is one prop: stop passing `badge` and the row reverts exactly.
