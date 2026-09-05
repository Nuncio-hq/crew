# Publish-before-wake port

Status: DONE; independent review and full integrated CI remain the lead's gates.

Implements upstream #7154 publication-order correctness and required #6003 start-scope assertions on Crew's existing send-completion, channel membership, local runtime, provider, and ACP seams. No commit/index operations performed.

## Behavior

- Readiness and persona creation collect invocation wakes; channel membership completes synchronously. Provider persona creation explicitly suppresses automatic deploy while collecting wakes.
- Only successful relay publication flushes wakes; failures, canceled preparation, revoked mentions, and Crew project reference-only routing cannot trigger unaddressed starts.
- Floors are captured while queuing, deduplicated to the earliest value, then passed through detached start to local subprocess or provider launch policy.
- Detached callbacks bind relay, signer, and agent. In-flight entries survive A-B-A; rejection releases the entry; failure toasts remain scoped to the originating community and signer.
- Native local starts revalidate scope after awaited mesh preflight, then use the exact checked relay and owner for spawn. Provider deploys serialize by agent and rebuild/validate the payload after waiting.
- Local floor overrides saved environment and strips ambient inherited floor. Provider policy strips case variants from launch environment before asserting this invocation's floor. ACP clamps replay to at most 15 minutes.
- Ordinary starts remain unscoped and Crew thread parallelism, Hermes policy, membership, audience routing, drafts, and channel-first flow remain intact.

## Verification

- 29 frontend tests pass: 20 detached hook; six actual completion/readiness hook cases; three actual channelAgents IPC boundary cases (provider create, failed membership, ordinary synchronous attachment).
- Mutation proof: moving wake flush before await send causes three completion tests to fail (early wake, rejected publish, membership/publication order); original source restored.
- 45 native scope-filter tests pass, including post-preflight relay/identity switch refusal and stable bound runtime-key inputs.
- Four provider payload tests pass (case-insensitive floor collisions, ordinary start compatibility, missing/changed scope refusal, rebuilt payload after async wait).
- Two local floor tests pass, including execution of a real shell child receiving the floor.
- Desktop TypeScript and file-size ratchet pass. Biome formatted/checked all changed frontend files. Native changed files rustfmt formatted. Native clippy `--lib -- -D warnings` passed after the shared target lock released; log `/tmp/crew-0522-publish-clippy.log`. The restored completion suite was rerun after the mutation: six passed.
- ACP startup floor helper has five tests; ACP worker owns their final combined gate.

Logs: `/tmp/crew-0522-publish-frontend-tests.log`, `/tmp/crew-0522-publish-red-test.log`, `/tmp/crew-0522-scope-native-tests.log`, `/tmp/crew-0522-provider-native-tests.log`, `/tmp/crew-0522-replay-native-tests.log`, `/tmp/crew-0522-publish-types.log`.

## Integration notes

Lead imports released discovery/NIP-11 caching separately. Crew retains unconditional final authorization revalidation as a deliberate stronger-check exception; conditional reuse would need to preserve Crew audience and huddle awaits. New AppState accessors and CLI environment modules are cohesive extractions required by the existing size ratchet; limits unchanged. Databricks discovery call receives the upstream-required fourth optional filter argument (`None`), preserving prior discovery behavior.

Backend provider tests cover the exact preparation seam with real JSON and async scheduling, not an external provider executable. Local scope test covers exact production binding and runtime-key construction, not a full Tauri launch. Full desktop/browser integration remains the lead's gate after its broader release delta is applied.

Docs impact: minor; lead should roll this into the release coverage ledger/changelog.
Unresolved questions: none.
