# Phase 2 — Stop the GitHub drawer refresh loop (new issue to file)

**Blocks:** phase 3. #32 cannot restore PR-drawer content assertions while
opening that drawer wedges the page.

## The defect

Opening the PR, CI, or Issue drawer starts an unbounded refresh loop.

`ProjectThreadWorkspacePanel.tsx:109-117`

```tsx
React.useEffect(() => {
  if (activeDrawer === "issue" || activeDrawer === "pr" || activeDrawer === "ci") {
    void model?.refreshGitHub();
  }
}, [activeDrawer, model]);
```

`model` is the dependency, and `useProjectThreadWorkspaceModel` builds a **new
object literal on every render** (`useProjectThreadWorkspaceModel.ts:106-117`).
Nothing about it is memoized. So the effect re-runs on every render, and the loop
closes like this:

1. effect runs → `refreshGitHub()` → `load(target, force: true)`
   (`projectThreadGitHubStore.ts:86-89`)
2. `force: true` skips the TTL early-return at `projectThreadGitHubStore.ts:45`;
   the in-flight guard at line 46 does not help either, because a resolved entry
   is rewritten **without** its `promise` field (lines 51-54)
3. the invoke resolves → `entries.set` installs a *new* snapshot object → `notify()`
4. `useSyncExternalStore` (line 82) sees a new snapshot → re-render
5. re-render → new `model` object → back to step 1

Each turn of the loop is one `get_thread_github_status` invoke
(`agentControl.ts:107-111`), which on the Rust side reaches for GitHub. In the
e2e mock the invoke resolves on the microtask queue, so the loop tightens to the
render cadence — which is why the drawer "wedges page JS" and hangs CDP click and
evaluate. In production it is slower but it is the same loop: CPU burn plus a
`gh` call every round trip, for as long as the drawer stays open.

The TTL cache at `projectThreadGitHubStore.ts:17` does not save this — `force`
exists precisely to bypass it.

## Fix

Two independent defects; fix both, they are one commit's worth of work.

**a. Memoize the model.** Wrap the return of `useProjectThreadWorkspaceModel`
(`useProjectThreadWorkspaceModel.ts:106-117`) in `React.useMemo` over its real
inputs. Note the early return at line 89 (`if (!context || steps.length === 0)
return null`) sits above the derived values — hooks cannot follow a conditional
return, so the derivations must move above it, or the whole body must be
restructured so the memo is unconditional. Do not "fix" this by moving the early
return below a `useMemo` that reads possibly-null values.

This is the repo's documented React-perf trap, worth re-reading before editing:
`CLAUDE.md` § Common Gotchas #7 — a hook returning a fresh `{}` each render
defeats everything downstream of it.

**b. Narrow the effect's dependency.** The effect only needs the function, so
depend on `model?.refreshGitHub` rather than `model`. That is only stable once
(a) lands or once `refresh` itself is stable — it is already a `useCallback` over
`[target]` (`projectThreadGitHubStore.ts:86-89`), and `target` is memoized
(`useProjectThreadWorkspaceModel.ts:74-85`), so this alone likely breaks the
loop. Do both anyway: (b) fixes this call site, (a) fixes every future one.

Also consider making the store skip `notify()` when the freshly loaded value is
deep-equal to the stored one. That is defence in depth, not the fix — do it only
if it stays simple.

## Validation — must be a counting assertion

The failure mode is "too many invokes", so assert the count. A test that only
checks the drawer renders will pass against the loop.

- Unit/component test: mount the panel, open the PR drawer, let effects flush,
  assert `get_thread_github_status` fired **once**; assert it stays at one after
  a parent re-render.
- e2e: instrument the mock bridge to count `get_thread_github_status` calls,
  open the PR drawer, assert a small bound (≤ 2 including the mount load).

There is no existing test file for either module
(`src/features/messages/lib/` has no `projectThreadGitHubStore` test), so this is
new coverage.

```bash
cd desktop && pnpm test && pnpm lint
```

## File the issue

This is not tracked anywhere. Before starting, open it on `Nuncio-hq/crew` with
the reproduction chain above, and link it from #32 as the real blocker behind the
"CDP hang" note. Do not fold it silently into a #32 PR — the sticky-bar test debt
and a product-side refresh loop deserve separate history.
