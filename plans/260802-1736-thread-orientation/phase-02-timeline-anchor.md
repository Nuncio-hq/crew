# Phase 2 — timeline anchor state

Idea **B**: the top-level message that owns the open thread declares itself in the channel
timeline, so in split mode both surfaces see each other.

Depends on phase 1 for `breadcrumb.anchorMessageId`.

## Why not reuse `highlightedMessageId`

`highlightedMessageId` is a *transient* jump-to-message flash: a 2s
`route-target-highlight-fade_2s_ease-out_forwards` animation applied at
`TimelineMessageList.tsx:759-765`. The anchor is the opposite — a persistent state that lives as
long as the thread is open. Keep them separate props; a message can legitimately be both at once
(click the breadcrumb and the anchor flashes *and* stays anchored).

## Where the treatment goes

A top-level message with replies renders through the `summary && onReply` branch of
`MessageRowItem` (`TimelineMessageList.tsx:757-816`). That branch already has a wrapper div that
carries the transient highlight — the same wrapper carries the anchor state:

```tsx
// TimelineMessageList.tsx:760-766, today
<div
  className={cn(
    "group/message relative mx-1 mb-1 flex flex-col gap-0 rounded-2xl px-0 py-1 transition-colors hover:bg-muted/50 focus-within:bg-muted/50",
    isHighlighted && "-mx-4 px-4 before:absolute … animate-[route-target-highlight-fade_2s…]",
  )}
>
```

`MessageRowItem` is a plain function, not `React.memo` — no comparator to update. But the row
renderer that calls it is a `useCallback` (`TimelineMessageList.tsx:236-285` with its dependency
array immediately after); the new prop **must** be added to that dependency array or the anchor
will not repaint when the user switches threads.

## Files

| File | Change |
|------|--------|
| `desktop/src/features/messages/ui/TimelineMessageList.tsx` | `openThreadAnchorId` prop → `MessageRowItem` → wrapper class + `aria-current` |
| `desktop/src/features/messages/ui/MessageThreadSummaryRow.tsx` | `isActive` prop: persistent surface + `Viewing thread` label |
| `desktop/src/features/messages/ui/MessageTimeline.tsx` | pass `openThreadAnchorId` through |
| `desktop/src/features/channels/ui/ChannelPane.tsx` | feed it from `threadBreadcrumb.anchorMessageId` |
| `desktop/tests/e2e/thread-orientation.spec.ts` | extend the phase-1 spec |

## Step 1 — thread the prop

`ChannelPane.tsx`, next to the existing `splitThreadPanelOpen` prop on `MessageTimeline`
(line ~688):

```tsx
openThreadAnchorId={threadBreadcrumb?.anchorMessageId ?? null}
```

`MessageTimeline.tsx`: add `openThreadAnchorId?: string | null` to `MessageTimelineProps`
(near `splitThreadPanelOpen` at line 109), default it to `null` in the destructure, and forward it
to `TimelineMessageList`.

`TimelineMessageList.tsx`: add `openThreadAnchorId?: string | null` to `TimelineMessageListProps`
(next to `highlightedMessageId` at line 61), default `null`, add it to the `Pick<>` union of
`MessageRowItemProps` (line 688), pass it in the `case "message"` render (line 238), and **add it
to the renderer `useCallback` dependency array**.

Using the anchor id — not `openThreadHeadId` — is the point: for a nested thread head the anchor is
the top-level ancestor, because the nested head has no row of its own in the main timeline.

## Step 2 — anchor treatment on the wrapper

In the `summary && onReply` branch:

```tsx
const isThreadAnchor = openThreadAnchorId != null && message.id === openThreadAnchorId;
```

On the wrapper div:

- `aria-current={isThreadAnchor ? "location" : undefined}`
- `data-thread-anchor={isThreadAnchor ? "true" : undefined}` — the E2E hook.
- Classes when anchored: a persistent left accent bar plus a soft surface tint. Use a
  `before:` pseudo-element bar (`before:absolute before:inset-y-0 before:left-0 before:w-0.5
  before:rounded-full before:bg-primary before:content-['']`) and `bg-primary/5` on the wrapper.
  Keep the negative-inset gutter trick the transient highlight uses so the bar sits in the gutter
  rather than over the text.
- **Composition:** the transient highlight also uses `before:`. When a message is both highlighted
  and anchored, one `before:` wins and the other silently disappears. Give the anchor bar its own
  `after:` pseudo-element instead, so the two never fight. Verify by clicking the breadcrumb with
  the thread open: the flash and the accent bar must both be visible.
- These are geometry utilities, not text sizes — `w-0.5` and friends are fine.
  `pnpm check:px-text` only polices text-size literals.

## Step 3 — the summary pill

`MessageThreadSummaryRow.tsx` — add `isActive?: boolean` (default `false`). It is a plain exported
function, not memoized, so no comparator work.

When `isActive`:

1. **Label.** Today the pill cross-fades `last reply <time>` → `View thread` on hover
   (lines 246-262). When active, render `Viewing thread` as the resting label and drop the
   hover swap — the state is not an affordance, it is a fact.
2. **Aria label.** `summaryAriaLabel` (line 96) becomes
   `Viewing thread with N replies` / `…, last reply <time>` instead of `View thread with …`.
3. **Surface.** The hover/focus surface span (line ~216, `data-testid="message-thread-summary-surface"`)
   becomes always-on: `opacity-100`, `bg-background/95`, `ring-1 ring-border/70`. Keep the hover
   and focus classes so a hovered active pill does not lose its ring.
4. **Text colour.** `text-foreground` at rest instead of `text-muted-foreground`.

Pass `isActive={isThreadAnchor}` from `MessageRowItem` (line 802).

Do **not** touch the panel-internal `MessageThreadSummaryRow` at `MessageThreadPanel.tsx:645` —
that one is the head's own collapsed-replies control and is never "the anchor".

## Step 4 — tests

E2E, extending `desktop/tests/e2e/thread-orientation.spec.ts`:

- Open a thread → the originating timeline row has `[data-thread-anchor="true"]` and
  `aria-current="location"`; its summary pill text contains `Viewing thread`.
- Close the thread → neither attribute remains on any row.
- Open a **different** thread → the anchor moves; exactly one row carries `data-thread-anchor`.
- Open a nested sub-thread from inside the panel → the anchor **stays** on the top-level row.

No unit tests here; the logic is a single id comparison and the interesting part is rendering.

## Step 5 — manual verification

- Split mode: anchor is visible without scrolling the timeline when the root is on screen.
- Anchor + transient highlight together (click the breadcrumb with the thread open) — both render.
- Latte and Macchiato: `bg-primary/5` must stay legible over `hover:bg-muted/50` in both.
- Unread state on the same row (the pill can show `(N new)`) still reads correctly.

## Risks

- The `before:`/`after:` collision in step 2 is the one real trap. It fails silently and only in
  the combined state, which is exactly the state the breadcrumb click produces.
- The wrapper div sets `hover:bg-muted/50`; a static `bg-primary/5` underneath can make hover feel
  dead. If it does, raise the anchored hover to `hover:bg-primary/10` rather than dropping the
  tint.
