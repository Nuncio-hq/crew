# Recover project-thread GitHub status

Status: implementation green; full verification pending

## Outcome

A temporary GitHub probe failure recovers without restarting Crew. The degraded
GitHub chip explains the safe failure detail and retries when clicked.

## Evidence

- Live `gh pr list` for `buzz/3e5bc0e499ac` succeeds and returns no PR.
- The store's 30-second TTL had no scheduled revalidation.
- RED contracts captured automatic recovery and the dead chip.

## Implementation

1. Preserve a bounded probe detail on the existing Tauri status response.
2. Revalidate mounted degraded entries after the existing 30-second TTL.
3. Reuse in-flight request deduplication and community reset protection.
4. Make the degraded chip trigger an immediate refresh.

## Verification

- Focused store and UI contracts
- Tauri `thread_github` unit tests
- Desktop typecheck and checks
- NuncioCrew Gate

## Rollback

Revert the single issue commit; the wire-field addition is optional for clients.

## Unresolved questions

None.
