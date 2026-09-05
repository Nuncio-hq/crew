# Native desktop released integration

Status: DONE; worktree `/Users/a1241968/Desktop/Oscar/crew-wt/upstream-0522`. Released target `desktop-v0.5.22` (`9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`). No index writes, commits or pushes by this worker. Root owns whole-upgrade gates, renderer integration, PR and release docs.

## Integrated behavior

- Merged remaining released native surface, using the real fork ancestry for files where the .18 snapshot was not the local ancestor. All native conflict markers resolved; release version/dependencies compiled.
- Released canonical effort descriptors, model/provider configuration, tri-state Save, alias removal, persona inheritance, provider mutation restrictions, pending access policy persistence/flush and rollback wired into the native update command. Update implementation is target-exact except Crew Hermes profile resolution/application/invariant checks. Old standalone effort persistence command removed with the new Save contract.
- Released Pi/OMP discovery and presets retained; Hermes remains a first-class runtime with profileArg `-p`, provider/profile binding and existing private readiness repair. Model metadata differs from release only for that Crew extension.
- Released workspace apply serialization/generation, managed-agent experiment state, identity archive/NIP-11 cache, member-only channel hydration plus separate open directory, team catalog, channel head cache, deep-link queues and PTT extraction mounted in the actual Tauri builder/invoke surface.
- Released agent directory mounted with verified ownership and memberships. Compiled owner-only builds additionally filter to the active viewer's verified agents; unmarked builds preserve cross-owner directory behavior. New same-viewer test covers the distinction.
- Released huddle/archive slice supplied by planner_337_338: archive target-exact; huddle target-exact except existing Crew 720x520 window dimensions. Speech endpoint/local-versus-remote buffer behavior follows release; upstream silence/onset/pre-roll changes are retained.
- Released signed created_at send response mounted for both normal and managed-agent sends, while retaining the captured relay/signer scope and Crew message tags. Released repository folder-open and project-owner announcement commands use Crew's existing ownership validation.

## Preserved Crew contracts

- Publish-before-wake priority ports remain wired through captured relay/signer, shared signed replay floor, provider payload revalidation and durable pending profile retention. Earlier focused evidence and RED mutation proof are in `260905-publish-before-wake-port.md`.
- Thread identity remains unconditional through native session policy, even if stale renderer state sends channel mode. Default parallelism remains 10. No generic agent-instance start prohibition or channel-level thread coalescing added.
- Existing process/runtime transition locks, owner/key-scoped processes and receipts, Windows durable receipt refusal, governor/agent-control/tool-pane, cowork/worktree, session aging and CLI environment handling retained.
- Managed lazy pool idle remains 1800 seconds with shared alias projection; separate ACP standalone timeout release change belongs ACP worker. Crew tracked-tool allowance remains separate.
- Custom provider deployment continues to capture/revalidate the active launch pair and rebuild authoritative payload at deploy time. Kept target-compatible wrapper signature.

## Validation

- `cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --all-targets -- -D warnings` passed; `/tmp/crew-native-release-clippy.log`.
- Full `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib`: **3431 passed, 19 ignored, 0 failed**, 42.56 seconds; `/tmp/crew-native-release-tests-final.log`.
- Initial concurrent compile/test run exposed the existing warm-path wall-clock assertion at 413ms against 300ms. Complete rerun after compiler load passed unchanged, including that test; no timing threshold relaxed.
- Extracted actionable log parsing into `storage-log-errors.rs`: storage 949 lines versus target 994. Tightened readiness recorded allowance from 1700 to 1673. Extracted two Crew nest template contract tests to avoid 1002-line growth. No native baseline expansion.
- Public released-function inventory found only the released `probe_node` wrapper absent, intentionally superseded by Crew's stronger installer/readiness path. Compilation confirms imported modules actually mount.

Limitations: these are native unit/compile checks, not a signed desktop launch or hosted-model smoke. Renderer and repository-wide baseline updates remain root-owned. Final clippy/focused nest check after the test-file extraction recorded separately in the parent handoff.

Docs impact: minor; this report. Root owns release changelog/compatibility accounting. Unresolved questions: none for native integration.
