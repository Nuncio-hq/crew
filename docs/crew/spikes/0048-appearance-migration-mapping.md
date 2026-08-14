# Spike 0048 — Stored appearance migration mapping (#204)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#204](https://github.com/Nuncio-hq/crew/issues/204)

## Question

What are the real stored preference shapes, and how does each map onto
Crew chrome + a Shiki syntax name without breaking
`communityThemePreference`?

## Decision affected

D-063 chrome/syntax split; community preference intact.

## Hypothesis

`buzz-theme` holds a Shiki name or `buzz` / `buzz-dark`. Luminance of
that name chooses Crew Dark vs Light. If the name is a real Shiki
palette (not the Buzz aliases), keep it as syntax; otherwise default
`dark-plus`.

## Scope

- `localStorage` keys `buzz-theme`, `buzz-follow-system`,
  `buzz-community-theme.v1:*`
- `theme-preference-migration.ts`
- `communityThemePreference.ts` v1 JSON

## Exclusions

Mobile prefs. New accent options.

## Pass criteria

Table of inputs → `{ chrome, syntax, toast? }`. Fresh profiles get Crew
Dark + `dark-plus` and no toast. Community v1 JSON still parses legacy
Shiki `theme` values.

## Fail criteria

Catppuccin Macchiato users lose their syntax, or community records go
`null`.

## Environment

Node unit tests (`theme-preference-migration.test.mjs`,
`communityThemePreference.test.mjs`).

## Method

Exercise migrate + parse against names seen in e2e seeds and the old
picker: `buzz`, `buzz-dark`, `catppuccin-latte`,
`catppuccin-macchiato`, `github-light`, `houston`, `dracula`,
`light` / `dark` / `system`.

## Results

| stored `buzz-theme` | chrome | syntax | toast |
|---|---|---|---|
| (missing) | crew-dark | dark-plus | no |
| catppuccin-macchiato | crew-dark | catppuccin-macchiato | kept syntax |
| catppuccin-latte | crew-light | catppuccin-latte | kept syntax |
| buzz | crew-light | dark-plus | default syntax |
| buzz-dark | crew-dark | dark-plus | default syntax |
| github-light | crew-light | github-light | kept syntax |
| houston | crew-dark | houston | kept syntax |
| light / dark / system | crew-light / crew-dark / crew-dark | dark-plus | default syntax |
| already `crew-*` + split flag | unchanged | stored or dark-plus | no |

`buzz` / `buzz-dark` are **not** Shiki palettes (`isShikiPaletteName`
false). Community parse still accepts those names and any Shiki name
on `theme`; optional `syntax` is additive. Default community record is
`crew-dark` / `dark-plus` / `followSystem: false`.

## Edge cases observed

If `buzz-theme-split-v1` is set and `buzz-theme` is still a Shiki name,
chrome falls back to Crew Dark (do not set the split flag in tests
unless chrome is already a Crew name).

## Limitations

Remote community events that still carry `theme: "houston"` apply as
Crew Dark chrome + Houston syntax via `applyAppearance`.

## Verdict

PASS — mapping locked in unit tests.

## Follow-up test contract

Migration unit tests + e2e toast from `catppuccin-macchiato`.

## Cleanup

None.
