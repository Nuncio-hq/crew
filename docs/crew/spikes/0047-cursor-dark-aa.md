# Spike 0047 — Cursor Dark calibration + AA (#204)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#204](https://github.com/Nuncio-hq/crew/issues/204)

## Question

Which four surface values, text tiers, and one accent meet WCAG AA on
every elevation for Crew Dark (Cursor-flavored) and Crew Light?

## Decision affected

D-063 Crew Dark default; AA required.

## Hypothesis

Cursor Dark near-blacks (`#181818` family) work as chrome. Issue-indicative
meta at white/40% fails AA on the overlay surface. One blue cannot be
both a 4.5:1 link on dark content *and* a 4.5:1 white-on-fill button.

## Scope

- Cursor Dark reference (founder daily driver)
- `desktop/src/shared/theme/crew-tokens.ts`
- Contrast unit tests

## Exclusions

Accent picker options. Mobile theme.

## Pass criteria

All three text tiers and the semantic set (success, danger, attention,
merged) are ≥ 4.5:1 on all four surfaces, both themes. Light filled
primary keeps white labels at AA.

## Fail criteria

Any text tier or semantic color below 4.5:1 on overlay.

## Environment

WCAG relative-luminance in `crew-tokens.test.mjs`.

## Method

Settle hex palettes, convert to shadcn HSL component strings, assert
contrast in unit tests.

## Results

Crew Dark surfaces: `#141414` shell · `#181818` content · `#1f1f1f`
raised · `#262626` overlay.

Text (calibrated, not raw opacities): primary `#ededed` · secondary
`#acacac` · meta `#979797` dark / `#6a6a6a` light (indicative 40% failed
AA on overlay/shell).

Accent: dark `#3b82f6` (link-on-content 4.83). Light `#2563eb` (white-on-fill
5.17). No single blue hits both.

Semantic dark: success `#3fb950` · danger `#f85149` · attention
`#d29922` · merged `#a371f7`. Light retuned: `#1a7f37` / `#cf222e` /
`#8a5a00` / `#8250df`.

White on dark `#3b82f6` is 3.68 — below AA for *normal* text on a filled
dark primary. Kept as Cursor-flavored chrome; light theme uses `#2563eb`
so filled buttons pass.

## Edge cases observed

Charts `--chart-1..5` are the semantic set + a neutral, not pastels.

## Limitations

Dark filled primary labels are a known AA miss for 14px body text; the
accent is still the interactive color.

## Verdict

PASS — tokens calibrated; tests lock AA for text tiers + semantic set.

## Follow-up test contract

`crew-tokens.test.mjs` AA assertions.

## Cleanup

None.
