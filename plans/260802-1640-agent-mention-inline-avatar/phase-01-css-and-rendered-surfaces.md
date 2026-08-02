# Phase 01 — CSS modifier + rendered surfaces

## Context

The agent mention chip paints a lucide-bot mask on a `::before` box. This phase
adds an "avatar" variant of that box and wires the two React surfaces that
render already-sent text.

## Files

**New**

- `desktop/src/shared/ui/agentMentionAvatar.ts` — shared helper:
  `agentMentionAvatarStyle(avatarUrl: string | null | undefined)` returning
  `{ className, style }` (empty when no URL). Runs `parseAnimatedAvatarUrl`
  (poster frame only) → `rewriteRelayUrl` → CSS-escape → `url("…")`.
- `desktop/src/shared/ui/agentMentionAvatar.test.mjs` — unit tests.

**Modified**

- `desktop/src/shared/styles/globals/markdown.css` — add the
  `.agent-mention-avatar` modifier rule after the existing
  `.agent-mention-highlight::before` block (keeps descending-specificity order).
- `desktop/src/shared/ui/mentionChip.ts` — export
  `AGENT_MENTION_AVATAR_CLASS`.
- `desktop/src/shared/ui/markdown/types.ts` — add
  `agentMentionAvatarsByName?: Record<string, string>` to both prop types
  (lines ~29 and ~58, mirroring `agentMentionPubkeysByName`).
- `desktop/src/shared/ui/markdown.tsx` — thread the new map through the runtime
  context (~1838, ~1881, ~1898), consume it in `MarkdownMention` (~1667-1719),
  and add it to the memo comparator (~1999).
- `desktop/src/features/messages/ui/MessageRow.tsx` — build the map in the
  existing `agentMentionPubkeysByName` memo (211-224) from
  `profiles[pubkey]?.avatarUrl`; pass it down alongside the pubkey map (~381).
- `desktop/src/features/messages/ui/SystemMessageRow.tsx` — `ProfileName` gains
  an `avatarUrl` prop; callers read it from the `profiles` lookup they already
  hold.

## Steps

0. Bump `--agent-icon-size` from `0.95em` to `1.05em` (`markdown.css:11`). This
   token has no other consumer (grep: `markdown.css:11, 203-204, 213-214`), so
   it enlarges the agent mention glyph in **both** states — avatar and robot
   fallback — which is required to keep the box identical across them.
1. Write `agentMentionAvatarStyle`. Contract:
   - `null`/`undefined`/blank → `{}` (chip keeps the robot glyph).
   - animated avatar → poster URL.
   - relay `/media/…` → proxied via `rewriteRelayUrl`.
   - escape `\`, `"`, and newlines before interpolating into `url("…")`.
   - returns `{ className: AGENT_MENTION_AVATAR_CLASS, style: { "--agent-mention-avatar": 'url("…")' } }`.
2. Add the CSS modifier. Only override paint — never geometry:
   `background: var(--agent-mention-avatar) center / cover no-repeat;`
   `border-radius: 50%; mask: none; -webkit-mask: none;`
3. Extend the markdown runtime type + context and consume in `MarkdownMention`.
   Look the avatar up by the same lowercased `mentionName` key already used for
   `agentMentionPubkeysByName` — one key space, no second normalisation path.
4. Build the map in `MessageRow`. It must reuse the existing memo so no extra
   pass over `profiles` is introduced (the comment at `MessageRow.tsx:191-194`
   is explicit that per-row rescans were a measured regression).
5. Wire `SystemMessageRow.ProfileName`.

## Validation

```bash
cd desktop && pnpm lint && pnpm check:px-text
cd desktop && pnpm test          # unit
cd desktop && pnpm test:e2e:smoke
```

Visual check — an agent mention in the timeline:

```bash
just desktop-screenshot --name agent-mention-avatar --messages /tmp/agent-mention.json
```

Confirm: avatar disc replaces the robot, name unchanged, chip height and
baseline identical to a human `@mention` chip on the same line.

## Risk / rollback

Low. CSS is additive behind a modifier class; dropping the class restores the
robot exactly. Rollback = revert the commit.
