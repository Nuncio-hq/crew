---
phase: 04
title: CLI --evidence flag on messages send
status: planned
priority: P0
effort: S
dependencies: ["03"]
---

# Phase 04 — `buzz messages send --evidence <kind>`

Turns C1 green. Delivers DoD checkbox 2.

## Seam

`buzz messages send` today has `--channel`, `--content`, `--kind`, `--reply-to`,
`--broadcast`, `--file`, `--mention` — and **no generic `--tag`** (`--tag` exists
only on `notes`). The clap variant is `MessagesCmd::Send` at
`crates/buzz-cli/src/lib.rs:398`; the handler is `cmd_send_message` at
`crates/buzz-cli/src/commands/messages.rs:574`, which builds the event through
`buzz_sdk::build_message` at `:664` and signs at `:680`.

**Key design choice — do not touch `buzz-sdk`.** Append the tag to the already
built `EventBuilder` before signing, using the precedent at
`crates/buzz-cli/src/client.rs:590` (`builder.tags([tag.clone()])`). That keeps
`crates/buzz-sdk/src/builders.rs` (+407 Crew lines today) at zero additional
delta and confines the change to the CLI.

## Files

| File | Change | Budget |
| --- | --- | --- |
| `crates/buzz-cli/src/lib.rs` | one clap arg on `MessagesCmd::Send` | ~6 lines |
| `crates/buzz-cli/src/commands/messages.rs` | field on `SendMessageParams` (`:564`), validate, append tag before sign | ~14 lines |
| new Crew-owned module (e.g. `crates/buzz-cli/src/commands/evidence.rs`) | kind enum + parse/validate + unit tests | new file |

`messages.rs` currently has **zero Crew delta** (1375 lines, identical to
upstream). Keeping the validation logic in a new Crew-owned module is what holds
that first edit to ~14 lines.

## Steps

1. Define the kind enum in the new Crew module: `test-run`, `metrics`,
   `before-after-visual`, `diff-stat`. One canonical string per variant; parse is
   exact-match (no aliases, no case folding) so the wire format stays stable.
2. Add `--evidence <kind>` to `MessagesCmd::Send`, following the existing
   add-a-CLI-flag playbook in that file.
3. Thread it onto `SendMessageParams` and validate **before** any network call or
   media upload — an invalid kind must exit 1 with a clear message and publish
   nothing.
4. After the builder is constructed at `:664`, append
   `["crew-evidence", <kind>]`, then sign as today.
5. Emit exactly one such tag. If the value is somehow already present, do not
   duplicate it.
6. Decide and document kind coverage: the tag is appended for whichever message
   kind the user selected; **only kind 9 renders a card in this slice** (phase
   05). Say so in `--help` text rather than silently rejecting other kinds.
7. Update `crates/buzz-cli/TESTING.md` if it enumerates `messages send` flags.

## What validation does and does not mean

`--evidence` validates **the enum value only**. It cannot and does not verify
that the message body actually contains evidence (RT-4). Do not describe it as
"validated evidence" in help text or docs — say "validated evidence kind".

## Acceptance criteria

- All C1 contract tests green.
- No change to the tag array of messages sent without `--evidence`.
- `crates/buzz-sdk/src/builders.rs` and `crates/buzz-core/src/kind.rs` unchanged.
- Exit code 1 with a readable error on an unknown kind; nothing published.
- `--evidence` composes with `--file`, `--reply-to`, `--mention`, `--broadcast`.

## Validation

```bash
cargo test -p buzz-cli
cargo clippy --all-targets -- -D warnings
just ci
```

Manual smoke against a local relay:

```bash
buzz messages send --channel <UUID> --content "…" --evidence test-run
buzz --format compact messages thread --channel <UUID> --event <ID>
```

## Anti-drift

Update `docs/crew/STATE.md` in the same PR (#117). Add the CLI surface to the
Crew CLI documentation if one enumerates flags.

## Risk

Low. Additive flag, no behavior change when omitted, trivially revertible.
