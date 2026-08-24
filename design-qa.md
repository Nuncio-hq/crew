# Design QA

## Project thread integration strip

- Layout reference: `plans/260731-1425-thread-integration-strip/prototype.html`
- App captures: `desktop/test-results/thread-worktree/01-integration-strip.png` and `desktop/test-results/thread-worktree/02-pr-history.png`
- Viewport: 380×701 thread panel inside the 1200×750 desktop E2E viewport.

| Check | Result | Evidence |
| --- | --- | --- |
| Two rows × three cells | Pass | Task/workspace/handoff above issue/PR/CI |
| Compact height | Pass | Closed strip leaves the thread composer and latest reply visible |
| Single shared drawer | Pass | PR and workspace details occupy the same region below the strip |
| Existing app color system | Pass | Existing semantic color classes |
| Typography | Pass | Named Tailwind text tokens |
| Narrow panel behavior | Pass | Labels truncate without overflow at 380px |
| Interaction coverage | Pass | Every cell is a button; Escape and close dismiss the drawer |

The prototype remains authoritative for layout and interaction only. Its custom palette was intentionally not copied.

## Add selected agent text to chat

- Source visual truth: `.scratch/cursor-add-to-chat-reference.png` (5114×2822, 2x desktop capture).
- Implementation: `desktop/test-results/add-selection-to-chat-action.png` and `desktop/test-results/add-selection-to-chat-composer.png` (1280×720 CSS px, device scale 1).
- State: agent text selected; action visible; then selection inserted into a non-empty channel composer.
- Interaction tested: select agent text, click Add to Chat, preserve existing draft, insert blockquote, focus composer.
- Console: no feature-related errors observed in the passing Playwright run.

### Comparison

- Full view: Cursor and Crew anchor a compact dark action above selected text. Crew intentionally omits Cursor's unrelated side-chat action and shortcut hints.
- Focused region: label, proximity, contrast, selection, and native composer blockquote are readable in the full 1280×720 captures; no extra crop needed.
- Typography: passed with Crew `text-xs` and composer tokens.
- Spacing: passed; the portaled action does not move message layout.
- Colors: passed with semantic popover, border, and foreground tokens.
- Assets: passed; the existing Lucide icon library supplies the icon.
- Copy: passed; exact requested label `Add to Chat` and exact selected text.

### Comparison history

- Pass 1: action was clipped by a transformed virtualized ancestor (P0). Fixed by portaling to `document.body` and clamping to the viewport.
- Pass 2: action and preserved-draft quote insertion verified in the passing E2E captures.

No actionable P0, P1, or P2 mismatch remains. P3: shortcut hints can follow if Crew adopts a discoverable shortcut.

final result: passed
