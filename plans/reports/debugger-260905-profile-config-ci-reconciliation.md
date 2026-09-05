# Profile, persona, provenance and config browser reconciliation

**Status: DONE.** Reconciled 15 failed CI cases in four existing browser specs; production code unchanged in this batch.

- Profile: released copy icons are present; explicit instance navigation opens Info and retains the disclosure state. Remote ownership uses a unique relay/profile seed with the actual mock viewer, avoiding Nadia's default owner-only registry entry. The no-registry case uses Mira's existing profile-only identity. Notification click marks its mention read; a distinct later unread verifies badge toggles without resurrecting read content. Zoom checks verify fixed 16px layout coordinates plus changed `--buzz-type-rem`. Settings compare actual inner card geometry and named secondary-copy rows.
- Persona: provider-free selection displays Default model; provider-specific choices remain exercised.
- Provenance: use the released sidebar setup marker.
- Config: effective values remain visible without removed source prose. The override fixture resolves to `claude-opus-4-20250514`, mixed-provider value is `openai`, and stopped agents show saved values plus unavailable ACP placeholders. Screenshot waits use the shared animation helper.

## Validation

Immutable E2E bundle `/tmp/crew-release-e2e-dist-agent-integration`, serial runner port 4190:

- Eight profile/persona cases passed in 18s: `/tmp/crew-profile-repaired-2.log`.
- Profile ingress passed in 15s: `/tmp/crew-profile-ingress-final.log`.
- Entire provenance/config smoke selection passed 9/9 in 26s: `/tmp/crew-config-provenance-final.log`.
- Four-file Biome, repository diff whitespace, and TypeScript checks passed. TypeScript output: `/tmp/crew-profile-config-tsc-final.log`.

Earlier failures from a foreign server disappearing on port 4192 were excluded from product diagnosis. These local runs do not claim complete browser CI or checks on the next commit.

## Documentation refresh

Refreshed all 1820 ledger path hashes against release target and pre-integration Crew bytes: 1420 target-exact, 358 adapted, 42 unchanged Crew. Eight previously exact paths now have reviewed browser/config adaptations. Composer right-click formatting commit #6683 moves to adapted: 24 ported, 155 adapted, 12 retained Crew divergences, zero pending.

Updated audit, integration record, plan, state and changelog to include live appearance samples and subsequent browser corrections. PR #342 remains open. Gate job and manual workflow passed on `7b58b8d2`; the encompassing CI workflow was cancelled after browser failures. New-head acceptance remains pending.

Docs impact: minor. No Git staging, commits, pushes or external changes.

Unresolved: coordinator's full browser acceptance and new-head remote checks.
