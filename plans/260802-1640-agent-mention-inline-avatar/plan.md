# Agent inline mention: avatar instead of robot glyph

Status: accepted — ready to implement
Branch: `buzz/58cbb30a801a`
Preview: [preview.html](preview.html) (reviewed and accepted by Oscar 2026-08-02)

## Decisions locked

| Decision | Value | Source |
|---|---|---|
| Glyph size | `--agent-icon-size: 1.05em` (up from `0.95em`) | Recommended after visual review; `1.2em` overflows the chip's `--inline-chip-min-height`. Overridable by Oscar. |
| Shape | Circle (`border-radius: 50%`) | Matches the circular `UserAvatar` in the message-row gutter. |
| No-avatar fallback | Keep the robot glyph | No new empty-circle state. |

**The size bump applies to both states, not just the avatar.** `--agent-icon-size`
is used only by the agent mention chip (`markdown.css:11, 203-204, 213-214` —
grepped, no other consumer), so raising it enlarges the robot fallback by the
same amount. That is deliberate: the fallback box and the avatar box must stay
identical, otherwise a late-arriving avatar would resize the chip and shift the
line. Zero-shift is the property that makes async profile lookup safe here.

Measured on the preview build (Playwright, both states side by side):

```
robot  → ::before 13.2969px × 13.2969px, padding-left 19.3px
avatar → ::before 13.2969px × 13.2969px, padding-left 19.3px
```

Identical box; only `border-radius` and the paint differ.

## Outcome

An inline `@Agent` mention chip renders `[agent avatar] [agent name]` instead of
`[robot icon] [agent name]`, in the message timeline, in system-message rows,
and in the composer while typing.

## Current behaviour (verified)

The robot is **not** a React icon — it is a lucide `bot` SVG painted as a CSS
mask on a `::before` pseudo-element:

- `desktop/src/shared/styles/globals/markdown.css:199-224` —
  `.message-markdown .agent-mention-highlight` reserves an inline box of
  `--agent-icon-size` (`0.95em`, declared at `markdown.css:11`) via
  `padding-left`, and `::before` fills it with `background: currentColor`
  masked by an inline lucide-bot data URL.

Three call sites add the `agent-mention-highlight` class:

| Surface | File | Mechanism |
|---|---|---|
| Message timeline | `desktop/src/shared/ui/markdown.tsx:1667-1719` (`MarkdownMention`) | React `className` |
| System messages | `desktop/src/features/messages/ui/SystemMessageRow.tsx:228-262` (`ProfileName`) | React `className` |
| Composer (TipTap) | `desktop/src/features/messages/lib/mentionHighlightExtension.ts:272-279` | ProseMirror inline `Decoration` (class string only) |

The composer and the timeline share the same `.message-markdown` CSS scope
(`useRichTextEditor.ts:492` puts `MESSAGE_MARKDOWN_CLASS` on the editor root),
which is why one CSS rule drives all three.

Note: agent mention chips deliberately drop the `@` prefix
(`markdown.tsx:1681-1688`) — the glyph replaces it. That stays true with an
avatar.

## Approach

Keep the `::before` box exactly where it is and change **what it paints**. The
chip gains a modifier class plus a CSS custom property holding the avatar URL:

```css
.message-markdown .agent-mention-highlight.agent-mention-avatar::before {
  background: var(--agent-mention-avatar) center / cover no-repeat;
  border-radius: 50%;
  -webkit-mask: none;
  mask: none;
}
```

Why this over rendering a real `<img>` child:

- **One visual definition for all three surfaces.** The composer is a
  ProseMirror decoration; it can set `class` + `style` on an inline range but
  cannot insert a child element without a widget decoration (which perturbs
  caret/selection behaviour). CSS custom property works identically in React
  and in the decoration.
- **Zero layout change.** The box is already sized and positioned; only its
  paint changes. No reflow risk in the timeline.
- **`agent-mention-highlight` keeps its name**, so all ~20 existing e2e
  selectors (`desktop/tests/e2e/mentions.spec.ts`,
  `persistent-agent-audience.spec.ts`, `team-mentions.spec.ts`,
  `channels.spec.ts`, `send-channel-binding.spec.ts`) stay green untouched.

**Fallback:** an agent with no `avatarUrl` keeps the robot glyph — the
modifier class is simply not applied. No new empty-circle state.

## Data plumbing

`avatarUrl` already exists on `UserProfileSummary`
(`desktop/src/shared/api/types.ts:128`), and every surface already holds the
`profiles` lookup it needs:

- Timeline: `MessageRow.tsx:211-224` already builds `agentMentionPubkeysByName`
  from `profiles` — the avatar map is built in the same pass.
- System rows: `SystemMessageRow.tsx` `ProfileName` callers already receive
  `profiles`.
- Composer: `useMentions.ts` holds `profiles` and already derives
  `agentKnownNames` (`useMentions.ts:496-509`).

URLs must go through `rewriteRelayUrl` (`shared/lib/mediaUrl.ts`) — relay
`/media/...` URLs need the localhost proxy; `data:` and external Blossom URLs
pass through unchanged. Animated avatars use their **poster frame** only
(`parseAnimatedAvatarUrl`) — no hover animation in a text chip.

Emoji avatars are `data:image/svg+xml,` + `encodeURIComponent(svg)`
(`ProfileAvatarEditor.utils.ts:221`), so quotes are `%22` and the URL is safe
inside `url("...")`. The shared builder still escapes `"`, `\`, and newlines
defensively.

## Phases

1. [phase-01-css-and-rendered-surfaces.md](phase-01-css-and-rendered-surfaces.md)
   — CSS modifier + shared URL builder + timeline and system-message chips.
2. [phase-02-composer-decoration.md](phase-02-composer-decoration.md)
   — composer decoration carries the avatar so typing matches the sent result.

Both phases ship in one PR; phase 1 alone would leave the composer showing a
robot for a mention that renders as an avatar once sent.

## Acceptance criteria

- Timeline, system rows, and composer all show the agent's avatar + name for a
  mention of an agent that has an avatar.
- An agent with no avatar still shows the robot glyph (no regression, no blank).
- Chip height, baseline alignment, and wrapping are unchanged (same `::before`
  box, `0.95em`).
- Zoom-safe: sizing stays on `em`/rem tokens — no px literals
  (`pnpm check:px-text` guard).
- Existing e2e mention specs pass without selector edits.
- `just ci` green.

## Non-goals

- Changing the robot glyph anywhere it is a **label** next to an existing
  avatar — mention autocomplete subtitle (`MentionAutocomplete.tsx:167`),
  members sidebar (`MembersSidebar.tsx:933`,
  `MembersSidebarMemberCard.tsx:186`), new-DM row
  (`NewMessageResultRow.tsx:128`), `MessageAgentOwner.tsx`. Those already show
  avatar + name; the bot icon there marks the "agent" *word*.
- Team mention chips (`Users` icon) — unchanged.
- Any change to avatar assignment or agent creation.

## Risks

| Risk | Mitigation |
|---|---|
| Avatar arrives after first render (profile lookup is async) | Chip starts with the robot glyph and swaps in place — same box, no layout shift. |
| Broken avatar URL → empty circle | CSS cannot detect load failure. Bound the blast radius by only applying the modifier when a URL is present; a 404 shows the chip background, not a hole. Revisit only if it shows up in practice. |
| Composer decoration rebuild misses avatar-map changes | Add the avatar map to the `useRichTextEditor` sync effect deps so the meta transaction re-fires. |

## Open questions

None blocking. The two questions raised at proposal time (size, shape) are
resolved in **Decisions locked** above.
