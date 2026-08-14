# Spike 0046 — Token-coverage audit (#204)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#204](https://github.com/Nuncio-hq/crew/issues/204)

## Question

Where do hardcoded colors bypass semantic tokens, and can a CI guard keep
new hex/hsl literals out of component files?

## Decision affected

D-063 Color is information; chrome is achromatic.

## Hypothesis

Most decorative hue in `desktop/src` is Tailwind palette classes
(`text-amber-*`, `text-emerald-*`, `text-purple-*`) plus a smaller set of
hex/hsl literals in token sheets, avatars, logos, and canvas.

## Scope

- `desktop/src/**/*.{ts,tsx,css}`
- Guard pattern: `desktop/scripts/check-px-text.mjs`

## Exclusions

Mobile/Flutter. Brand/logo assets. Layout redesign.

## Pass criteria

Inventory exists. Status colors map to reserved tokens. A check script
fails on new numeric `hsl()` / `rgb()` / `#rrggbb` in non-allowlisted
files.

## Fail criteria

No inventory, or status chips still use decorative palette classes.

## Environment

Workspace scan (Python/ripgrep) of `desktop/src`.

## Method

1. Count Tailwind `{text,bg,border,fill}-{amber,emerald,green,purple}-*`
   in feature/UI files.
2. Count hex / numeric hsl/rgb literals; separate token sheets from
   components.
3. Convert status uses to `success` / `attention` / `merged` /
   `destructive`.
4. Add `desktop/scripts/check-literal-colors.mjs`.

## Results

- ~85 files used Tailwind status palettes; converted to semantic
  tokens (`text-success`, `text-attention`, `text-merged`, …).
- Hex/hsl literals remain in token sheets (`theme.css`,
  `crew-theme.css`, `crew-tokens.ts`), Shiki/theme machinery, tests,
  avatars, logos, QR, mermaid, and canvas editors — allowlisted.
- Guard scans `desktop/src` and fails on new literals outside that
  allowlist. `hsl(var(--…))` is not a hit.

## Edge cases observed

GitHub issue numbers (`#204`) look like 3–4 digit hex. The guard only
matches 3/6/8 digit hex and skips comment lines.

Language-color chips in `projectLanguages.ts` encode language identity,
not CI/agent state; they stay on the language ramp.

## Limitations

The guard does not ban Tailwind palette *classes* (`text-amber-500`).
Those were converted in this issue; a class-level linter is future
work.

## Verdict

PASS — inventory + conversion + CI guard.

## Follow-up test contract

`pnpm check:literal-colors` in `desktop` `check`. Contrast tests in
`crew-tokens.test.mjs`.

## Cleanup

None.
