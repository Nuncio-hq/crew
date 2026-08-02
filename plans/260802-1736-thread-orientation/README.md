# Thread Orientation — plan index

**Status:** approved, ready to implement
**Owner:** Cursor Grok High Fast
**Surface:** desktop only (`desktop/src`). No relay, no event kinds, no mobile.
**Approved by:** Oscar, in `#NuncioCrew project`, thread `05ab752b`, after reviewing
`prototype.html` in this directory.

## Problem

An open thread panel never says which conversation it belongs to.

- The panel header is the hardcoded string `Thread` —
  `desktop/src/features/messages/ui/MessageThreadPanel.tsx:938`.
- A comment in `desktop/src/features/channels/ui/FocusThreadDrawer.tsx:129` already claims
  *"Orientation lives in the drawer header's breadcrumb"* — that breadcrumb was never built.
- The channel name only exists in the scrim's `aria-label="Back to #channel"`
  (`FocusThreadDrawer.tsx:186`) — screen readers get it, eyes do not.
- The timeline has no *"this thread is open"* state. `highlightedMessageId` is a 2-second
  jump-to-message flash (`TimelineMessageList.tsx:759-765`); `MessageThreadSummaryRow` has
  hover/focus states only.
- Focus mode covers the channel with a scrim and a 72px sliver
  (`threadFocusLayout.ts:17`) — the worst case.
- Nested heads are real: the panel head can have `depth > 0`
  (`MessageThreadPanel.tsx:646`).

## Outcome

With a thread open, the user can answer three questions without closing anything:

1. **Which channel and which message does this thread belong to?** → breadcrumb in the panel header.
2. **Where in the channel did it come from?** → persistent anchor state on the top-level message.
3. **How do I get back there?** → one click on the breadcrumb scrolls the timeline to the anchor and highlights it.

Split mode is the default target; focus mode must work too but is the secondary case.

## Phases

| Phase | File | Scope | Depends on |
|-------|------|-------|-----------|
| 1 | [phase-01-breadcrumb.md](phase-01-breadcrumb.md) | `threadOrientation.ts` lib + `ThreadBreadcrumb` header + click-to-anchor navigation (ideas **A** + **C**) | — |
| 2 | [phase-02-timeline-anchor.md](phase-02-timeline-anchor.md) | Persistent anchor state on the timeline row and its summary pill (idea **B**) | Phase 1 (anchor id comes from the lib) |
| 3 | [phase-03-ancestry-strip.md](phase-03-ancestry-strip.md) | Collapsed ancestor line inside the panel for nested threads (idea **D**) | Phase 1 |

Phase 1 is the only phase that must ship whole. Phases 2 and 3 are independently
shippable on top of it.

## Acceptance criteria

- [ ] With a thread open in split mode, the panel header reads `#<channel> › <Author>` (plus a
      truncated snippet), never the bare word `Thread` — unless the breadcrumb cannot be built,
      which falls back to `Thread`.
- [ ] Clicking the breadcrumb scrolls the channel timeline to the top-level message and flashes it.
      In split mode the thread stays open. In focus mode the drawer closes so the flash is visible.
- [ ] The top-level message that owns the open thread carries a persistent accent + tint and
      `aria-current`, and its summary pill reads `Viewing thread` instead of `View thread`.
- [ ] For a thread head at `depth > 0`, the breadcrumb grows a segment per ancestor and the panel
      shows a collapsed ancestry line; the timeline anchor stays on the **top-level** message.
- [ ] At a 380px panel width, `#channel` and the author name are still fully readable — only the
      trailing snippet truncates.
- [ ] `pnpm test`, `just ci`, and `pnpm test:e2e:smoke` pass.

## Non-goals

Explicitly out of scope; do not build these.

- Mobile parity (`mobile/lib/features/channels/thread_detail_page.dart:243` has the same bare
  `Text('Thread')`). Separate follow-up.
- Sticky mini-header on scroll (idea G).
- Accent colour derived from the thread-head id (idea I).
- Thread quick-switcher.
- Any change to relay, event kinds, `buzz-core`, or persistence. This is presentation only.

## Conventions that apply

- `desktop/src/features/messages/lib/` uses **camelCase** filenames (`threadPanel.ts`,
  `timelineSnapshot.ts`). Follow the neighbours — do not kebab-case new files here.
- **No arbitrary text-size literals.** `pnpm check:px-text` fails the build on `text-[13px]` and
  on `text-[0.9rem]` alike. Use stock rem tokens or the `text-2xs` / `text-3xs` tokens in
  `desktop/tailwind.config.js`.
- Unit tests are `node:test` in `*.test.mjs` importing the `.ts` module directly
  (see `features/messages/lib/timelineSnapshot.test.mjs`). Run with `pnpm test` from `desktop/`.
- E2E specs go in `desktop/tests/e2e/` and must be registered in `playwright.config.ts`
  (`smoke` project `testMatch`). Build via `pnpm test:e2e:smoke`, never a plain `pnpm run build`.
- Commit with `git commit -s` — the DCO check fails any commit without a `Signed-off-by` trailer.
- `just desktop-tauri-fmt` fails inside git worktrees and blocks the pre-commit hook
  (see `CLAUDE.md` gotcha 6). This change touches no Rust; if the hook trips on it anyway, run
  that recipe from the main checkout, then re-stage.

## Verification

```bash
cd desktop
pnpm test                 # node:test units, includes the new threadOrientation tests
pnpm check                # biome + file-size + px-text + pubkey-truncation guards
pnpm test:e2e:smoke       # builds with the mock Tauri bridge, then runs the smoke project
cd .. && just ci          # full gate before the PR
```

Screenshots for the PR: `just desktop-screenshot`, then `scripts/post-screenshots.sh <pr> <dir>`.
Before posting, `shasum -a 256 test-results/<dir>/*.png` — every hash must be unique, or two
shots captured the same state.
