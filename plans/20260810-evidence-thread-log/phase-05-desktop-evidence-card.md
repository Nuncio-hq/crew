---
phase: 05
title: Desktop evidence card (four kinds)
status: planned
priority: P0
effort: L
dependencies: ["03"]
---

# Phase 05 — Desktop evidence card

Turns C2 green. Delivers DoD checkbox 3.

## Seams

| Need | Seam |
| --- | --- |
| Tag reaches the renderer | `desktop/src/features/messages/lib/formatTimelineMessages.ts:520` already assigns `tags: applyEditTagOverlay(...)` onto `TimelineMessage`; `types.ts:50` types it as `string[][]`. **Zero plumbing work.** |
| Per-kind body dispatch | `desktop/src/features/messages/ui/MessageRow.tsx:368` `renderBody()` switch; evidence rides kind 9, so it lands in the `default:` branch at `:415` |
| Crew-owned body already in that branch | `MessageRowDefaultBody` (`:429-446`), a Crew-owned 127-line file that already receives `message` and `imetaByUrl` |
| Card structure precedent | `AgentReceiptCard.tsx` (Crew-owned, 173 lines) — testids, PR-reference link resolution defaulting to `Nuncio-hq/crew` |
| Tolerant parse precedent | `agentReceipt.mjs` `parseAgentReceipt()` returns `null` and the renderer falls back |

## The ratchet constraint (R-1) — read before writing code

`MessageRow.tsx` is **980 lines against a 1000-line cap**
(`desktop/scripts/check-file-sizes.mjs:8`, `MAX_LINES = 1000`). D-022 and
`docs/crew/UPSTREAM-SYNC.md:63-66` forbid raising the limit or granting an
exception — the required move is extracting Crew deltas into Crew-owned files.

**Budget: ≤8 added lines in `MessageRow.tsx`.**

Preferred route: pass the props the card needs down into the Crew-owned
`MessageRowDefaultBody` at `:429-446` and branch on the tag *there*. That is
roughly six added prop lines in the upstream-derived file and keeps all logic
Crew-side.

If the budget cannot be met, **extract** existing Crew additions out of
`MessageRow.tsx` into a Crew-owned module until it fits. Do not raise
`MAX_LINES`. Do not add an allowlist entry.

## Files

| File | Owner | Change |
| --- | --- | --- |
| `desktop/src/features/messages/ui/MessageRow.tsx` | upstream-derived | **≤8 lines** — prop pass-through only |
| `desktop/src/features/messages/ui/MessageRowDefaultBody.tsx` | Crew | branch on evidence kind, delegate to the card |
| `desktop/src/features/messages/lib/evidenceTag.{mjs,ts}` | Crew (new) | `parseEvidenceKind(tags)` → kind or `null` |
| `desktop/src/features/messages/ui/EvidenceCard.tsx` | Crew (new) | shell + per-kind layouts |
| per-kind subcomponents if the file approaches ~200 lines | Crew (new) | keep files small |

## Rendering rules

| Kind | Layout | Image dependency |
| --- | --- | --- |
| `metrics` | compact number table (before / after / delta) | none |
| `test-run` | red→green block, failing excerpt above passing | none |
| `diff-stat` | summary line + PR reference link (reuse `AgentReceiptCard`'s href resolution) | none |
| `before-after-visual` | side-by-side images via the existing `imeta` media pipeline | required; must degrade to captions + links when images fail to load |

Hard requirements:

- The three text kinds must be **fully legible with no images loaded** — this is
  the phone-friendly requirement and is asserted in C2.
- The card body is **agent-authored, untrusted**. Render through the existing
  Markdown pipeline; no raw HTML, no new sanitizer bypass.
- Parse defensively: an unknown kind, a malformed body, or a missing image
  renders the ordinary message body. Never an error boundary.
- **kind 46043 keeps the receipt card and ignores `crew-evidence`** (R-2).
- Text sizing: rem-based Tailwind tokens only. No `text-[13px]`, no arbitrary rem
  literals — `pnpm check:px-text` fails the build otherwise (root `AGENTS.md`).

## Steps

1. Write `parseEvidenceKind` with the tolerant-parse contract.
2. Build `EvidenceCard` with the three text layouts first — they carry the
   phone-friendly requirement and need no media plumbing.
3. Add `before-after-visual` last, reusing the `imetaByUrl` path
   `MessageRowDefaultBody` already receives.
4. Wire the branch, measuring `MessageRow.tsx` line count before and after.
5. Run the ratchet and px-text guards explicitly, not just via `just ci`.

## Acceptance criteria

- All C2 contract tests green.
- `MessageRow.tsx` ≤ 1000 lines with `MAX_LINES` unchanged at 1000.
- `pnpm check:px-text` green.
- Non-evidence messages render byte-identically to today.
- No new upstream-file edits beyond the ≤8 lines in `MessageRow.tsx`.

## Validation

```bash
cd desktop && pnpm check:px-text && node scripts/check-file-sizes.mjs
cd desktop && pnpm test:e2e:smoke
just ci
```

## Anti-drift

Update `docs/crew/STATE.md` in the same PR (#117). If harness capability facts
change, update `desktop/src/features/agents/AGENTS.md` in the same PR.

## Risk

Highest-risk phase in the plan. Two guards can trip (file-size ratchet, px-text)
and both have a tempting wrong fix (raise the limit, allowlist the line). Neither
is permitted. Rollback is removing the branch — published evidence messages then
render as ordinary messages, which is exactly the pre-slice behavior.
