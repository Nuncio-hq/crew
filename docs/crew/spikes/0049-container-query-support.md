# Spike 0049 — Tailwind container-query support (#205)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#205](https://github.com/Nuncio-hq/crew/issues/205)

## Question

Does the current desktop Tailwind build emit CSS container queries for
`@container` / `[@container(…)]:` utilities so a pane can adapt to its
own width rather than the viewport?

## Decision affected

D-064 container-first responsiveness; convert pane-internal `sm:`/`md:`.

## Hypothesis

Tailwind v4 already ships container queries. One pane converted
end-to-end (declared-plans body + header) is enough proof.

## Scope

- `desktop/package.json` Tailwind version
- `ThreadPanelDeclaredPlansBody`, `AuxiliaryPanelHeader`, `AuxiliaryPanelShell`

## Exclusions

Viewport-level shell (sidebar collapse, 800×500 floor). Mobile / `web/`.

## Pass criteria

`@container` appears in a built CSS chunk, or source already uses it
successfully in-tree (`ThreadAgentStatusChip`, `UnifiedAgentsSection`).
A converted pane compiles.

## Fail criteria

Utilities are stripped or ignored; pane still needs `sm:`.

## Environment

- Commit: working tree for #205
- Tailwind: `^4.3.0` in `desktop/package.json`

## Method

Grep in-tree `@container` usage. Confirm `sm:` in pane files can be
rewritten to `[@container(min-width:40rem)]:` without a new plugin.

## Results

Tailwind 4.3 is already on the desktop app. Multiple production panes
already declare `@container`. Header `sm:` tokens converted to
container variants in this landing.

## Edge cases observed

Stacked variants such as
`[@container(min-width:40rem)]:group-hover/message:opacity-100` must
keep the container query outside the group-hover chain.

## Limitations

First-paint width `0` (ResizeObserver) must default to the stacked /
narrow behavior so we never paint a squeezed side column.

## Verdict

PASS. Container queries are the production tool for pane internals.

## Follow-up test contract

`responsiveContract.test.mjs` (stack threshold) + E2E matrix at 300–720.

## Cleanup

None. Production conversion is the landing, not spike leftovers.
