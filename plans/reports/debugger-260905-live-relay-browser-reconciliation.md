# Live relay browser reconciliation

## Changes

- User-input fixture authenticates event submission with a signed NIP-98 event, including exact URL, method, and body hash. The relay requires that verified authentication timestamp before materializing NIP-OA ownership. Registration/request/answer assertions remain real relay reads and writes.
- Forum join discovers `get_open_channel_directory`; `get_channels` intentionally contains only current memberships. The sender then joins and publishes through the existing commands.
- Workflow relay bridge now returns the published event ID as revision, rejects stale expected revisions, and signs the native `expected-revision` tag. The fixture supplies the returned revision and waits for the next timestamp second before an ordered replacement, avoiding the protocol's legitimate same-second event-ID tie conflict.
- Ancestor-island test samples each half viewport during real wheel scrolling. The previous 6000px jump skipped virtualized rows between samples, reporting only 45/100 on an isolated relay. The same nonce-filtered >90/100 reachability assertion passes with continuous sampling.

## Verification

- Two original failing integration cases: 2 passed, 4.0s.
- Full bridge mutations and integration specs: 17 passed, 22.4s, isolated database and relay; final test waits for recipient EOSE and verifies the actual plain-message WebSocket EVENT/OK acceptance.
- Ancestor-island baseline: failed with 45 reachable rows. Corrected sampling: 1 passed, 20.1s, fresh isolated database and relay.
- TypeScript and focused Biome checks pass. Independent source review found no actionable issue in the authentication, directory, workflow revision, or sampling changes.
- Full desktop unit suite: 7006 passed, 1 existing skip, 0 failed. Full desktop check passes.

Logs: `/tmp/crew-root-real-integration.log`, `/tmp/crew-root-live-ws-readiness.log`, `/tmp/crew-root-parity.log`, `/tmp/crew-root-parity-fixed.log`, `/tmp/crew-final-browser-reconciliation-js.log`, `/tmp/crew-final-browser-reconciliation-check.log`.

All relay runs used the dedicated recovery Docker services and disposable databases; no production relay restarted.

## Readiness regression

The cross-user test now correlates the actual live channel REQ and EOSE before sending, then checks the sender's matching WebSocket OK before the unchanged recipient timeline assertion. The complete 17-case run passes with these explicit protocol boundaries. No production delivery change was needed; final remote checks remain pending.
