# Phase 1 — breadcrumb + click-to-anchor

Ideas **A** (breadcrumb replaces the word `Thread`) and **C** (clicking it scrolls the timeline
back to the originating message).

## Context

The panel header today:

```tsx
// desktop/src/features/messages/ui/MessageThreadPanel.tsx:926-941
const threadHeaderContent = (
  <AuxiliaryPanelHeaderGroup
    backButtonAriaLabel="Back to conversation"
    backButtonTestId="message-thread-back"
    leading={headerLeading}
    onBack={isSinglePanelView && !isFocusMode ? onClose : undefined}
  >
    <AuxiliaryPanelTitle>Thread</AuxiliaryPanelTitle>
  </AuxiliaryPanelHeaderGroup>
);
```

`headerLeading` is the split/focus view-mode toggle, injected from `ChannelPane.tsx:697`. The
breadcrumb replaces only the `AuxiliaryPanelTitle` child — leave the `leading` slot and the
back button alone.

`TimelineMessage` carries what we need (`desktop/src/features/messages/types.ts:42-44`):
`parentId`, `rootId`, `depth`. `rootId` is the top-level message id for any reply;
`depth` is computed in `formatTimelineMessages.ts:388-413` by walking parents, with a
`rootId !== parentId ? 2 : 1` fallback when the parent is not loaded.

## Files

| File | Change |
|------|--------|
| `desktop/src/features/messages/lib/threadOrientation.ts` | **new** — pure breadcrumb builder |
| `desktop/src/features/messages/lib/threadOrientation.test.mjs` | **new** — node:test units |
| `desktop/src/features/messages/ui/ThreadBreadcrumb.tsx` | **new** — presentational header control |
| `desktop/src/features/messages/ui/MessageThreadPanel.tsx` | new props; render breadcrumb in the header |
| `desktop/src/features/messages/ui/MessageTimeline.tsx` | expose `jumpToMessage` on the imperative handle |
| `desktop/src/features/channels/ui/ChannelPane.tsx` | build the breadcrumb, wire the navigate handler |
| `desktop/tests/e2e/thread-orientation.spec.ts` | **new** — smoke spec |
| `desktop/playwright.config.ts` | register the new spec in the `smoke` project |

## Step 1 — the pure builder

`desktop/src/features/messages/lib/threadOrientation.ts`

```ts
export type ThreadBreadcrumbSegment = {
  /** Ancestor or head message this segment stands for. Carried whole so the
   *  ancestry strip can reopen it without a second lookup. */
  message: TimelineMessage;
  author: string;
  /** Only the terminal (thread-head) segment carries a snippet. */
  snippet: string | null;
};

export type ThreadBreadcrumb = {
  channelName: string;
  /** Top-level ancestor first, thread head last. Never empty. */
  segments: ThreadBreadcrumbSegment[];
  /** True when ancestors were dropped to satisfy the segment cap. */
  truncated: boolean;
  /** The message the main timeline should anchor on: always top-level. */
  anchorMessageId: string;
  anchorMessage: TimelineMessage | null;
};

export function buildThreadBreadcrumb(input: {
  channelName: string | null | undefined;
  threadHead: TimelineMessage | null;
  messageById: ReadonlyMap<string, TimelineMessage>;
}): ThreadBreadcrumb | null;
```

Rules:

1. Return `null` when `threadHead` is null **or** `channelName` is empty — the caller falls back
   to the literal `Thread`.
2. Build the chain by walking `parentId` through `messageById` from the head upward, stopping at
   `depth === 0`, at a missing parent, or after 8 hops (cycle guard). Reverse so the top-level
   ancestor is first.
3. `anchorMessageId`: the first segment's id when the walk reached a `depth === 0` message;
   otherwise `threadHead.rootId ?? threadHead.id`. The walk can break when scrollback has not
   loaded an ancestor — `rootId` is the fallback that still points at the right timeline row.
   `anchorMessage` is `messageById.get(anchorMessageId) ?? null`.
4. Segment cap: keep at most 3 segments. When the chain is longer, keep the **first** and the
   **last two** and set `truncated: true`. The first segment is the timeline anchor and the last
   two are the immediate context — the middle is the disposable part.
5. Snippet, terminal segment only: take `message.body`, strip fenced code blocks, collapse all
   whitespace runs (including newlines) to single spaces, trim, then truncate to 40 characters on
   a word boundary with a trailing `…`. Empty body (media-only message) → `null`.
6. Author label is `message.author` verbatim. Do not re-resolve profiles here; the timeline has
   already done that.

Keep the module free of React and under ~150 lines.

## Step 2 — unit tests

`desktop/src/features/messages/lib/threadOrientation.test.mjs`, `node:test` + `node:assert/strict`,
importing `./threadOrientation.ts` (see `timelineSnapshot.test.mjs` for the exact import shape).

Cover:

- depth-0 head → one segment, `anchorMessageId === head.id`.
- depth-1 head with the parent present → two segments, anchor is the parent.
- depth-2 head → three segments, anchor is the top-level ancestor.
- depth-2 head whose parent is **missing** from the map → walk breaks, anchor falls back to
  `rootId`, segments contain only the head.
- chain of 5 → `segments.length === 3`, `truncated === true`, first segment is still the top-level
  ancestor.
- parent cycle (`a.parentId = b`, `b.parentId = a`) → terminates, does not hang.
- snippet: newline collapsing, fenced-code stripping, 40-char truncation, empty body → `null`.
- `channelName: ""` → `null`.

## Step 3 — the breadcrumb component

`desktop/src/features/messages/ui/ThreadBreadcrumb.tsx`

- One `<button type="button">` for the whole trail. A single control keeps the tab order flat and
  matches the prototype, where clicking anywhere on the trail navigates.
- `aria-label={`Go to the original message in #${channelName}`}`.
- Content: `#channel`, then a `›` separator per segment, then each segment's author, then the
  terminal snippet as `: "…"`. When `truncated`, render a `…` segment between the first author and
  the rest — non-interactive, `aria-hidden`.
- **Truncation rule (decided, do not change):** `#channel` and every author name get
  `shrink-0 whitespace-nowrap`. Only the terminal snippet gets `min-w-0 truncate`. At the 380px
  default panel width (`AUXILIARY_PANEL_DEFAULT_WIDTH_PX`) the locating information must survive
  intact; the snippet is the only expendable part.
- Type sizes: match the existing `AuxiliaryPanelTitle` token for the channel and author segments,
  and use `text-xs` for the snippet. **No arbitrary `text-[…]` literals** — `pnpm check:px-text`
  fails on both px and rem literals.
- Colours: `text-foreground` for the channel and authors, `text-muted-foreground` for separators
  and snippet. Hover raises the whole trail to `text-foreground` with
  `focus-visible:outline-hidden` plus a ring, matching `MessageThreadSummaryRow`'s button.
- `data-testid="thread-breadcrumb"`.

Presentational only — it takes `breadcrumb: ThreadBreadcrumb` and `onNavigate: () => void`, and
holds no state.

## Step 4 — panel wiring

In `MessageThreadPanel.tsx`, add to `MessageThreadPanelProps`:

```ts
breadcrumb?: ThreadBreadcrumb | null;
onNavigateToAnchor?: (messageId: string) => void;
```

Replace the `AuxiliaryPanelTitle` child at line 938:

```tsx
{breadcrumb && onNavigateToAnchor ? (
  <ThreadBreadcrumb
    breadcrumb={breadcrumb}
    onNavigate={() => onNavigateToAnchor(breadcrumb.anchorMessageId)}
  />
) : (
  <AuxiliaryPanelTitle>Thread</AuxiliaryPanelTitle>
)}
```

Both props stay optional so `MessageThreadPanelSkeleton` and any other caller keep compiling.

## Step 5 — expose `jumpToMessage` on the timeline handle

`MessageTimeline.tsx` already has the function — `jumpToMessage` at line 448 delegates to
`scrollToMessage(messageId, { highlight: true, ...options })`. It is simply not on the handle.

```ts
// line 33
export type MessageTimelineHandle = {
  jumpToMessage: (messageId: string) => boolean;   // add
  scrollToBottomOnNextUpdate: () => void;
  settleAtBottom: () => boolean;
};
```

Add it to the `useImperativeHandle` at line 433, delegating to the existing `jumpToMessage`, and
add `jumpToMessage` to that hook's dependency array. `jumpToMessage` is declared at line 448,
**after** the `useImperativeHandle` at 433 — move the `useImperativeHandle` block below the
`jumpToMessage` declaration rather than hoisting anything, so the `const` is initialised before
the closure captures it.

Return value: `scrollToMessage` returns a boolean for "row found and scrolled". Propagate it —
the caller depends on it in step 6.

## Step 6 — ChannelPane wiring

`ChannelPane.tsx` already holds everything needed: `messageTimelineRef` (line 169),
`threadHeadMessage`, `activeChannel`, `messages`/`visibleMessages`, `threadAllMessages`,
`useFocusThreadDrawer` (line 508), `onCloseThread`.

Build the lookup map and the breadcrumb in one memo:

```tsx
const threadBreadcrumb = React.useMemo(() => {
  const messageById = new Map<string, TimelineMessage>();
  for (const message of messages) messageById.set(message.id, message);
  for (const message of threadAllMessages) messageById.set(message.id, message);
  return buildThreadBreadcrumb({
    channelName: activeChannel?.name,
    threadHead: threadHeadMessage,
    messageById,
  });
}, [activeChannel?.name, messages, threadAllMessages, threadHeadMessage]);
```

`threadAllMessages` is merged in second so a nested head's ancestors resolve even when they are
not in the main timeline window.

Navigate handler:

```tsx
const handleNavigateToThreadAnchor = React.useCallback(
  (messageId: string) => {
    // Scroll first: in focus mode the channel is still mounted under the scrim, so the
    // jump lands either way — and if the row is not in the loaded window we must not
    // close the drawer and leave the user with nothing.
    const jumped = messageTimelineRef.current?.jumpToMessage(messageId) ?? false;
    if (jumped && useFocusThreadDrawer) {
      onCloseThread();
    }
  },
  [onCloseThread, useFocusThreadDrawer],
);
```

**Decided behaviour, stated so it can be overridden rather than rediscovered:**

- *Focus mode closes the drawer, split mode keeps the thread open.* In focus mode the channel sits
  behind a 75–80% scrim; a highlight flash nobody can see is not navigation.
- *Order is scroll-then-close*, not close-then-scroll. It makes the "row is not loaded" case a
  clean no-op instead of a thread that vanished for nothing. The highlight animation runs for 2s
  (`route-target-highlight-fade_2s`), comfortably outliving the ~0.14s drawer exit.
- The channel section gets `inert` while covered (`ChannelPane.tsx`, `channelIsCovered`). `inert`
  blocks focus and pointer interaction, not programmatic scrolling — verify this by hand in focus
  mode as part of step 8.

Pass both props into `MessageThreadPanel` (around line 852):

```tsx
breadcrumb={threadBreadcrumb}
onNavigateToAnchor={handleNavigateToThreadAnchor}
```

## Step 7 — E2E smoke spec

`desktop/tests/e2e/thread-orientation.spec.ts`, registered in `playwright.config.ts` under the
`smoke` project's `testMatch`.

1. `installMockBridge(page)` — every mock-mode spec needs it, and any `page.addInitScript`
   seeding must run **before** it.
2. Open a thread from a channel with replies.
3. Assert `[data-testid="thread-breadcrumb"]` is visible and its text contains the channel name
   and the head author.
4. Scroll the timeline away from the root, click the breadcrumb, assert the root row is back in
   view.
5. Before any `page.screenshot()` / `locator.screenshot()`, call
   `await waitForAnimations(page)` from `../helpers/animations` — `toBeVisible()` resolves
   mid-animation.

Run with `pnpm test:e2e:smoke`. Never `pnpm run build` — that strips the mock Tauri bridge and
every spec fails with `Cannot read properties of undefined (reading 'invoke')`, which reads like a
product bug. If a stale server on port 4173 is serving old code, kill it and rebuild.

## Step 8 — manual verification

- Split mode, 380px panel: `#channel` and author fully readable, only the snippet ellipsised.
- Split mode: clicking the breadcrumb scrolls and flashes; the thread stays open.
- Focus mode: clicking the breadcrumb scrolls, then the drawer closes and the flash is visible.
- Nested thread (open a thread, then open a sub-thread from inside the panel): the breadcrumb grows
  a segment; clicking it still lands on the **top-level** message.
- Cmd +/- zoom: the breadcrumb scales with the rest of the header.
- Both themes (Latte, Macchiato).

## Risks

- Moving the `useImperativeHandle` block in `MessageTimeline.tsx` sits next to scroll-anchoring
  code with delicate ordering. Move only that block; touch nothing else in the file.
- `messages` is a new array reference on most timeline updates, so the breadcrumb memo will
  recompute often. It is O(n) map-building over an already-rendered window — acceptable. Do not
  reach for `useStableReference` unless a profile shows it matters.
