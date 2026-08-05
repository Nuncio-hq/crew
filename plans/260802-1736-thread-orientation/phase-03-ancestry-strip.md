# Phase 3 — ancestry strip for nested threads

Idea **D**: when the open thread head is itself a reply, the panel shows the message it hangs off,
collapsed to one line and clickable.

Depends on phase 1 (`ThreadBreadcrumb.segments` already carries the ancestor messages).

## Why

A nested head is a real state, not a hypothetical — `MessageThreadPanel.tsx:646` passes
`depth={threadHead.depth}` and it can be `> 0`. At `depth ≥ 2` the breadcrumb alone gets long, and
there is no way to step *up* one level: the parent thread has no row in the panel and no row in the
main timeline. Without this the user can only get back to the top-level message, never to the
intermediate one.

## Files

| File | Change |
|------|--------|
| `desktop/src/features/messages/ui/ThreadAncestryStrip.tsx` | **new** |
| `desktop/src/features/messages/ui/MessageThreadPanel.tsx` | render it above the head; new prop |
| `desktop/src/features/channels/ui/ChannelPane.tsx` | wire `onOpenAncestorThread` |

## Step 1 — the component

`desktop/src/features/messages/ui/ThreadAncestryStrip.tsx`

Props:

```ts
{
  /** Ancestors above the head, top-level first. Empty → render nothing. */
  segments: readonly ThreadBreadcrumbSegment[];
  truncated: boolean;
  onOpenThread: (message: TimelineMessage) => void;
}
```

Render:

- Nothing at all when `segments` is empty. A depth-0 head must look exactly as it does today.
- One row per ancestor: small avatar + author + one-line snippet, whole row a `<button>`,
  `truncate` on the snippet, `shrink-0` on the author.
- Cap at **2** visible rows. When `truncated` (or more than 2 ancestors), render a single leading
  row reading `N earlier messages` that opens the **top-level** ancestor. Deeper history belongs
  in the breadcrumb, not here.
- Visually subordinate to the head: `text-xs`, `text-muted-foreground`, a left rule
  (`border-l border-border/45`) echoing the thread depth guides in
  `MessageThreadSummaryRow`, and no avatar stack.
- `data-testid="thread-ancestry-strip"`, each row `data-testid="thread-ancestry-row"`.
- No arbitrary text-size literals — `text-xs` / `text-2xs` only.

The snippet is already built by `buildThreadBreadcrumb`; do not re-derive it here. Note that
phase 1 only fills `snippet` on the terminal (head) segment — extend the builder to fill it for
every segment as part of this phase, and add a unit case for it in
`threadOrientation.test.mjs`.

## Step 2 — placement in the panel

In `MessageThreadPanel.tsx`, inside `threadScrollRegion`, immediately **above** the
`data-testid="message-thread-head"` block (line 560), inside the same
`THREAD_PANEL_MESSAGE_GUTTER_CLASS` gutter so it aligns with the head:

```tsx
{ancestorSegments.length > 0 ? (
  <div className={cn(THREAD_PANEL_MESSAGE_GUTTER_CLASS, "pt-2")}>
    <ThreadAncestryStrip
      onOpenThread={onOpenAncestorThread}
      segments={ancestorSegments}
      truncated={breadcrumb?.truncated ?? false}
    />
  </div>
) : null}
```

`ancestorSegments` is `breadcrumb?.segments.slice(0, -1) ?? []` — every segment except the head
itself.

Add the prop:

```ts
onOpenAncestorThread?: (message: TimelineMessage) => void;
```

Guard the strip on both `ancestorSegments.length > 0` **and** `onOpenAncestorThread` being present,
so a caller that does not wire it does not render dead controls.

It sits inside the scroll region, not the header — it scrolls away with the head, which is right:
it is context for the head, and the breadcrumb is the persistent orientation.

## Step 3 — ChannelPane wiring

`ChannelPane` already has an open-thread handler taking a `TimelineMessage` — the same one passed
as `onReply={... onOpenThread}` to `MessageTimeline` (line 674). Pass it straight through:

```tsx
onOpenAncestorThread={onOpenThread}
```

Opening an ancestor replaces the panel contents with that ancestor's thread. The breadcrumb and
the phase-2 anchor both recompute from the new head automatically — no extra wiring.

Respect the archived-channel guard the timeline already applies
(`activeChannel?.archivedAt ? undefined : onOpenThread`) so an archived channel behaves the same
way in the panel as in the timeline.

## Step 4 — tests

E2E, extending `desktop/tests/e2e/thread-orientation.spec.ts`:

- Depth-0 thread → `[data-testid="thread-ancestry-strip"]` is absent.
- Open a sub-thread from inside the panel → the strip appears with one row naming the parent's
  author.
- Click that row → the panel head becomes the parent message and the strip disappears.

## Step 5 — manual verification

- Depth-1 and depth-2 heads, both themes.
- The strip must not push the head below the fold on a short panel; if it does, drop its vertical
  padding rather than adding a collapse control.
- Keyboard: tab order is breadcrumb → ancestry rows → head, all with visible focus rings.

## Risks

- Ancestors are resolved from `messageById`, which merges `messages` and `threadAllMessages`
  (phase 1, step 6). An ancestor outside both windows simply will not appear as a segment — the
  breadcrumb's `rootId` fallback still gives a correct anchor, and the strip renders one row
  fewer. That degradation is acceptable; do not add a fetch for it.
