# PR hub comment refresh loop — 2026-09-05

Status: DONE. Concrete cause reproduced; root-owned fix independently reviewed and covered by passing regressions. Production was read-only for this worker. Docs impact: none beyond this report.

## Symptom and causal path

Pool's Playwright case `desktop/tests/e2e/thread-pr-hub.spec.ts:319` reaches a visible/enabled/stable Comment on GitHub button, then hangs during click. Browser evaluation and screenshot capture also stop responding (`/tmp/crew-e2e-seventh.log:634`). The mock comment handler only appends fixture data; no external comment was sent.

1. `ThreadPrHub.tsx` constructs a fresh `{ owner, name, number }` reference each render.
2. A successful comment invokes `invalidateThreadForgePullRequestStore()`, permanently advancing `reloadGeneration` above zero, then calls refresh.
3. The original store effect depended on the reference object's identity and used `generation > 0` as `force` for `load`.
4. Each completed fetch replaces the cached snapshot and notifies subscribers. That renders a new reference object, reruns the effect, and forcibly fetches again despite the fresh TTL. The loop can starve the browser thread after the click has already dispatched.

Mounted the actual store hook with the same fresh-reference calling pattern and bounded mock native responses. Evidence in `/tmp/crew-forge-loop-probe.log`: initial one detail request, one diff request, two renders; one invalidation produced seven detail and seven diff requests/eight renders before the diagnostic held request seven. No additional user action. This proves a store reload loop rather than a stuck comment request or pointer-action overlay.

`ProjectThreadWorkspacePanel` is not required for the reproduction. Its PR subject effect returns early for the same thread PR/root identity. No speculative changes proposed there or in the discussion submit handler.

## Root-owned fix and review

Root stabilized the hook's PR reference using scalar owner/name/number dependencies, then reused that value for cache key, load effect and refresh callback. Worktree/base dependencies, cache epoch rejection, invalidation generation and explicit refresh behavior remain unchanged. The fix removes render identity as a reload trigger without preventing a different PR from loading.

## Regression coverage

Added only `desktop/src/features/messages/lib/threadForgePullRequestStore.test.mjs` (158 lines):

- One initial detail/diff pair; comment-style invalidation plus explicit refresh shares one further in-flight pair; completion and equal-valued reference rerenders trigger no more requests.
- Refresh callback remains stable across equivalent reference objects.
- Null reference performs no load; switching owner/repository/PR number loads the new identity.
- Changing worktree loads the correct diff; manual refresh sends the latest base and worktree.

The regression fixture manually resolves each request pair. An old-code reload creates a detectable extra pending pair and fails the count assertion; no artificial production limit, timer dependence or unbounded immediate promise loop is needed.

**9 targeted tests passed**, zero failures/skips, including existing forge contracts (`/tmp/crew-forge-loop-regression.log`). Biome and scoped diff-check pass. Root owns compilation and pool owns rebuilt browser replay.

## Final bounded integration review

Root requested three additional read-only checks; all passed with no findings:

- `MembersSidebarMemberCard.tsx`: management marker is now outside the name/pubkey opacity-swap subtree. The marker remains visible and interactive on row hover while Crew's existing hover/focus pubkey display is preserved.
- `ThreadFocusForgeSplit.tsx`: only narrow layouts give the tool column `flex-1`; selected-pane visibility is unchanged. Wide mode retains its explicit clamped width.
- `BloomMenu.tsx` and `theme.css`: shadow offsets, blur, spread and alpha values exactly match the released values. `--floating-shadow: 0 0% 0%` is the same neutral black as the previous RGB literal; the token is globally loaded and has no overriding declaration. No color-policy allowlist expansion.

Existing browser coverage remains root/pool-owned; no redundant unit tests or production edits for these checks.

Unresolved questions: none.
