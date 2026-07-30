# Project local workspace — relay-native plan

Status: complete

## Outcome

A Project already registered through Buzz Desktop or `buzz create project`
can link or relink one local folder. The raw absolute path remains metadata on
the signed kind `30617` announcement; `(pubkey, d)` and Git metadata do not
change. Agent mentions in that Project's canonical channel receive the latest
relay-confirmed path while `session/new.cwd` remains unchanged.

## Locked scope

- Relay acknowledgement and exact event read-back are mandatory.
- Read-modify-write preserves clone, web, protection, channel, and unknown tags.
- No local authoritative registry or optimistic-success fallback.
- No Git/worktree behavior, board, mobile, Rust, or Buzz restyling.
- Existing Project and message UI remain the visual base.

## Evidence

- Spikes 0001, 0002, and the real Tauri picker spike 0003 passed.
- The original six RED contract files and later review-regression contracts
  describe parsing, replacement ordering, relay failure, privacy copy, channel
  matching, agent-send behavior, consent readiness, and channel retry.
- Twenty existing Project tests pass, proving the loader and fixtures are
  healthy.

## Phases

1. [Tauri folder-boundary spike](phase-01-tauri-folder-boundary-spike.md)
2. [Relay-native link/relink](phase-02-relay-native-link-and-relink.md)
3. [Agent context and validation](phase-03-agent-context-and-validation.md)

## Smallest fork surface

Two existing behavior files:

- `desktop/src/app/routes/projects.tsx`
- `desktop/src/features/messages/ui/useMentionSendFlow.ts`

All feature logic and UI are new files. Using Tauri's supported JavaScript
dialog API additionally changes `desktop/package.json` and `pnpm-lock.yaml`.

## Approved exceptions

1. Count the dependency manifest and lockfile as mechanical dependency changes;
   the fork still has only two existing behavior-file edits.
2. In this no-Rust slice, a folder is known to exist when selected. After app
   restart it is shown as `linked, not locally verified`; a moved, deleted, or
   denied folder is surfaced when an agent/tool actually uses it. Proactive
   restart-time classification requires a later Rust capability spike.

The manager approved both exceptions on 2026-07-30.
