# Phase 02 — Composer decoration carries the avatar

## Context

The composer highlights `@Agent` with a ProseMirror inline decoration that only
sets a class (`mentionHighlightExtension.ts:272-279`). It shares the
`.message-markdown` CSS scope with the timeline, so phase 01's modifier works
here too — the decoration just has to emit the class and the inline style.

Without this phase the composer shows a robot for a mention that renders as an
avatar the moment it is sent.

## Files

- `desktop/src/features/messages/lib/mentionHighlightExtension.ts` — storage
  gains `agentAvatarsByName: Record<string, string>`; `buildDecorations` and
  `addMatchesForPatterns` pass per-match `style` through to
  `Decoration.inline`.
- `desktop/src/features/messages/lib/useMentions.ts` — derive and return
  `agentAvatarUrlsByName` next to `agentKnownNames` (~496-509, ~982), keyed by
  lowercased name, sourced from the `profiles` lookup already in scope.
- `desktop/src/features/messages/ui/MessageComposer.tsx` — pass it to the
  editor hook (~243).
- `desktop/src/features/messages/lib/useRichTextEditor.ts` — accept the prop
  (~81, ~199), write it into storage, and **add it to the sync effect deps**
  (~643-657) so the `mentionHighlightKey` meta transaction re-fires when an
  avatar resolves late.
- `desktop/src/features/messages/lib/mentionHighlightExtension.test.mjs` (or
  the existing spec file) — cover style emission and the no-avatar path.

## Steps

1. Add `agentAvatarsByName` to extension storage (default `{}`).
2. `addMatchesForPatterns` gains an optional resolver so the agent pass can look
   the matched name up (lowercased, trimmed) and, when a URL exists, pass
   `{ class: "... agent-mention-avatar", style: '--agent-mention-avatar:url("…")' }`
   to `Decoration.inline`. Reuse `agentMentionAvatarStyle` from phase 01 —
   serialise its style object to a CSS declaration string rather than
   duplicating the escaping.
3. Derive the map in `useMentions` and thread it through the composer.
4. Extend the sync effect deps.

## Validation

```bash
cd desktop && pnpm lint
cd desktop && pnpm test
cd desktop && pnpm test:e2e:smoke
cd desktop && pnpm test:e2e:integration   # mentions.spec.ts lives here
```

The mention specs already assert `.agent-mention-highlight` counts in the
composer input; the class is preserved, so they gate this phase for free.

Manual: type `@` + an agent name in the composer — the chip shows the avatar
immediately, and the sent message shows the identical chip.

## Risk / rollback

Medium-low. The decoration rebuild path is perf-sensitive (`useRichTextEditor`
notes that `editor.storage` mutation is the only working channel, and rebuilds
are already gated on mention-boundary edits). Adding a `style` attr does not
change rebuild frequency; adding the avatar map to the effect deps does add
re-decorations when profiles resolve — bounded by profile-lookup settling, the
same cadence `mentionNames` already has.

Rollback = revert; phase 01 remains valid on its own.
