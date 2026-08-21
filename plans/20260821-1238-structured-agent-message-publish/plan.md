# Structured agent message publish

Status: approved by founder request in Buzz thread `e9b9303ab538…`

## Goal

Prevent shell quoting from corrupting agent-authored multiline Markdown before
it reaches the relay.

## Plan

- [x] Reuse the existing `buzz-cli` message boundary through a shell-free child process.
- [x] Add a `publish_message` tool to `buzz-dev-mcp` that passes content without a shell.
- [x] Prefer that tool in the ACP base prompt; retain stdin CLI fallback.
- [x] Add exact-content regression tests for newlines, backticks, `$()`, and quotes.
- [x] Run package tests, formatting, lint, and independent review.
- [ ] Open the PR and confirm `NuncioCrew Gate`.
- [ ] Merge only after `NuncioCrew Gate` is green.

## Boundaries

- No Desktop or relay normalization.
- No decoding of literal `\\n`.
- No event/schema changes.
- Initial structured tool covers text, reply target, mentions, kind, and broadcast;
  attachments/evidence keep the existing CLI stdin path.

## Risks

- Dev MCP can be capability-disabled, so the CLI stdin fallback remains documented.
- Publishing must preserve existing auth, membership, mention, and reply validation.
