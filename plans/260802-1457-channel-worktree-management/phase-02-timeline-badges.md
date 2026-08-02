# Phase 2 — Channel timeline workspace badges

Depends on: Phase 1 (registry command + store).
Delivers: branch / PR / diff chips on project-thread summary rows, so the channel
timeline answers "which thread has a PR" without opening each thread.

Today the row says only `3 replies · last reply 12m ago`
(`MessageThreadSummaryRow.tsx:243-276`).

## Target rendering

```
(avatars) 3 replies · ⎇ buzz/649566de · PR #42 ✓ · +214 −18
```

Rules:
- A chip renders only when it carries information. No worktree → no chips at all,
  and the row is byte-identical to today.
- No new row and no height change: chips join the existing flex line, `text-2xs`,
  `text-muted-foreground/70`, and the whole row keeps `min-w-0` truncation.
- PR chip colour follows state: open/green checks, draft/muted, failing/red,
  merged/violet. Reuse whatever `ProjectThreadGitHubRow.tsx` already uses for
  PR state styling rather than inventing a second palette.
- The chips are inside the existing `<button>`, so a click still opens the
  thread — no nested interactive elements (a nested `<a>` to the PR would break
  the button semantics; the PR link lives in the thread strip and the drawer).

## Files

| File | Change |
| --- | --- |
| `desktop/src/features/messages/lib/projectThreadBadge.ts` | **new** — pure builder: registry entry → chip list |
| `desktop/src/features/messages/lib/useProjectThreadBadge.ts` | **new** — hook: message → badge or `null` |
| `desktop/src/features/messages/ui/MessageThreadSummaryRow.tsx` | render optional `badge` prop |
| `desktop/src/features/messages/ui/TimelineMessageList.tsx` | pass the badge at line 802 |
| `desktop/src/features/messages/ui/MessageThreadPanel.tsx` | pass the badge at lines 644 and 773 |
| `desktop/src/features/messages/lib/projectThreadBadge.test.mjs` | **new** — builder tests |

## Data path

1. `message.body.indexOf("buzz://project-workspace?")` — cheap reject for the
   ~99 % of rows that are not project threads. Only then call
   `parseProjectThreadContext` (`projectThreadWorkspace.ts:33`), memoized per
   message id.
2. `context.localPath` → registry store keyed by canonical repo path (Phase 1).
3. `getProjectWorktreeEntryByRoot(repoPath, message.id)` — the thread root id
   **is** the summary row's message id.
4. `buildProjectThreadBadge(entry)` → `{ branch, pullRequest, diff } | null`.

Perf discipline (CLAUDE.md gotcha #7): the hook must return a reference-stable
object or `null`. The registry store returns entries from a `Map` that is
replaced on refresh, so wrap the derived badge in
`shared/hooks/useStableReference.ts` content-equality caching, otherwise every
registry tick re-renders every summary row and defeats `React.memo` on the
timeline.

## Accepted v1 limitations

- **Zero-reply project threads show nothing.** `buildTimelineThreadSummary`
  returns `null` when `descendantCount === 0` (`threadPanel.ts:215-219`), so
  there is no row to hang chips on. In practice a worktree only exists after an
  agent has run, which almost always produced a reply. Revisit only if it bites.
- **Other members see no chips** unless the project repo path resolves on their
  machine (D2). This is a silent, deliberate degrade — no "unavailable" state.
- Branch text is shown verbatim (`buzz/649566de51d6`); truncation is left to the
  existing row overflow rules.

## Tests

`projectThreadBadge.test.mjs`:
- entry without PR → branch chip only
- open PR with green rollup → `PR #42 ✓` + diff chip
- draft PR → no check glyph, draft styling flag
- `github: unavailable` → branch chip only, no PR chip
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
