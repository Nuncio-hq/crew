# Phase 02 — Implement and verify

## Status

Complete

## Requirements

- One Repository callback serves empty and populated Projects views.
- Folder picker runs before any write.
- Review shows the exact plaintext path and relay destination.
- Channel creation is reused on same-identity retry.
- Kind `30617` is cached only after acknowledgement and exact read-back.
- No standalone Local workspace strip remains.

## Architecture

`ProjectsView` exposes one optional Repository callback. Crew-owned code handles
picker, review, validation, channel creation, relay publication, read-back, and
query-cache insertion. Default Buzz creation remains unchanged when the
callback is absent.

## Risks

- A channel can exist without a Project if Project publication fails. The
  in-memory retry token reuses it for the same Project identity.
- App restart after that failure can leave an orphan channel; cleanup is
  outside this slice.
- Two concurrent app instances can still race between exact duplicate
  preflight and channel creation; deterministic channel IDs or relay CAS would
  be needed to eliminate the possible losing orphan.
- Git metadata is omitted until a separately spiked read-only native adapter.

## Security

- Never log or handle a private key; signing stays in Buzz's Keychain path.
- Require explicit confirmation before publishing the plaintext local path.
- Reject invalid paths, duplicate owner/d-tag identity, mismatched signed
  owner, or mismatched read-back.

## Validation

- Focused contracts.
- Full desktop test, typecheck, checks, and build.
- Rebuild and smoke-test `NuncioCrew.app`.
