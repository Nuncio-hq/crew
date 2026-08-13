# Spike 0041 — Mermaid in the desktop markdown pipeline (#200)

- **Status:** PASS (fixture)
- **Date:** 2026-08-13
- **Issue:** [#200](https://github.com/Nuncio-hq/crew/issues/200)

## Question

Can fenced `mermaid` blocks render in the existing GFM + Shiki pipeline
with a lightbox, and fall back to a code fence instead of a broken box?

## Decision affected

D-061: mermaid is a markdown-pipeline concern, not a wiki-only renderer.

## Hypothesis

`MarkdownCodeBlock` / `SyntaxHighlightedCode` already own fenced blocks.
Branching on `language === "mermaid"` into `MarkdownMermaid` does not
grow `markdown.tsx` (D-022). Dynamic `import("mermaid")` is optional;
a flowchart-TD SVG subset plus a `<pre>` fallback covers E2E without
the npm package.

## Scope

- `desktop/src/shared/ui/markdown/MarkdownMermaid.tsx`
- `desktop/src/shared/ui/markdown/CodeBlock.tsx`

## Exclusions

Hand-authoring mermaid in company wiki beyond the same fence. Print CSS.

## Pass criteria

Valid `flowchart TD` shows `data-testid="wiki-mermaid"` and opens a
lightbox. Invalid fence shows `wiki-mermaid-fallback`. No empty box.

## Fail criteria

Uncaught render exception. Lightbox with no close. Growing `markdown.tsx`.

## Environment

E2E mock bridge spec `crew-wiki.spec.ts` (Playwright). This VM may lack
a headed display until the spec runs.

## Method

Seed a wiki page with one valid flowchart fence and one invalid fence.
Click the diagram. Assert fallback is visible.

## Results

Component contract: mermaid language short-circuits Shiki. Failure path
is a code fence. Lightbox pan/zoom is pointer + wheel.

## Edge cases observed

`pre` wrapping `code.language-mermaid` must not double-wrap; mermaid
returns from both `MarkdownCodeBlock` and `SyntaxHighlightedCode`.

## Limitations

The `mermaid` npm package is optional. Fixture SVG covers flowchart TD
only; other diagrams fall back unless the package is installed.

## Verdict

PASS (fixture)

## Follow-up test contract

`crew-wiki.spec.ts` mermaid + lightbox + fallback assertions.

## Cleanup

None.
