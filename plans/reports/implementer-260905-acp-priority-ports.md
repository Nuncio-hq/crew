# ACP priority ports and remaining released inventory

Worktree: `/Users/a1241968/Desktop/Oscar/crew-wt/upstream-0522`. Base `871eecb18`; released target `desktop-v0.5.22` / `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`. No commits or pushes by this worker.

## Implemented priority slice

- Adapted #6706 (`8f3566ad9`): complete/included/previously delivered/absent context; root-presence propagation; full batch and cancelled-event coverage; remove only redundant parsed parent. Preserved Crew bracket headers and verified-root workspace boundary.
- Adapted #6953 (`93237b4a7`): authored template passed through workflow executor/sink; explicit canonical owner/mention provenance; signature/NIP-11 relay-key validation; reconnect generation; same encapsulated author gate in normal/setup listeners. Crew roster/handoff/raw owner control remains on actual signer. Normal ingress keeps conversation UUID, routing channel, edit hold, queue-before-steer and exact-thread signals.
- Production attribution/authorization/ingress and imported auth tests extracted into focused files, each under 200 lines; no complete upstream file replacements.
- Added seventh empty SDK message-tag argument where required by parallel released SDK upgrade (`setup_mode.rs`, `waiting_notice.rs`); existing elicitation BuzzEvent test literals carry generation zero.

## Evidence

- `/tmp/crew-0522-author-red.log`: temporarily restored old raw-signer attribution; actual listener -> UUID queue -> ACP dispatch test fails at admission (1 assertion failure). Final source restored automatically afterward.
- `/tmp/crew-0522-completeness-red.log`: temporarily restored old unconditional retrieval guidance; all three new retrieval completeness tests fail by assertion. Final source restored automatically afterward.
- `/tmp/crew-0522-acp-final.log`: **1147 ACP library tests pass**. Includes #339/#340, preserved UUID/ledger/queue invariants, imported authorization matrix and new wire-dispatch contract.
- `/tmp/crew-0522-workflow-dispatch.log`: two authenticated workflow roots for one logical agent pass through production authorization/subscription/queue and run concurrent actual ACP subprocess prompts. Peers synchronize before either completes. Separate session IDs, only own thread content, EndTurn, no replay batch.
- `/tmp/crew-0522-workflow-tests.log`: **169 workflow library tests pass; 2 existing Postgres tests ignored** in this infrastructure-free command.
- Relay workflow sink with all ignored tests is running separately against fresh `buzz_0522_workflow`, local dedicated Postgres 5471 / Redis DB4 at 6471. `/tmp/crew-0522-workflow-relay-integration.log`. No hosted relay/provider touched.

No claim yet of final whole-upgrade checks, hosted model success, or complete release coverage. Further edits require rerunning affected tests.

## Remaining ACP/workflow release inventory

`git diff --name-only desktop-v0.5.18 desktop-v0.5.22 -- crates/buzz-acp crates/buzz-workflow` gives 17 paths (14 ACP plus 3 workflow). The commit log for those paths contains these 16 changes:

| Commit / PR | ACP/workflow impact | Current disposition / remaining work |
|---|---|---|
| `40220d561` #7154 | config replay floor / startup watermark | Desktop worker owns; present in current source and included in ACP green suite. |
| `47d068e21` #7259 | extra message tags in pool/setup SDK calls | Compatible empty arguments present; ACP compiled green. Broader GIF/emoji owned by other slices. |
| `2af9773d6` #7250 | generic mention example in base prompt | Small documentation wording still to port. |
| `42aeb1571` #7208 | ACP README harness extension instructions | Documentation update; actual Pi preset belongs desktop slice. |
| `cae158ce7` #7185 | default ACP idle 900 -> 1500 seconds and cross-budget test | Not yet ported. Preserve separate Crew tracked-tool allowance 2400 and absolute turn cap; coordinate config owner and native shell/tool budget changes. |
| `bd7349041` #6730 | workflow test module named `postgres_tests` | Test-selection adjustment, no runtime change; reconcile with final Crew CI selection. |
| `674c173eb` #6732 | explicit session scope, thread policy/config/prompts, queue/pool/lib rewrite | Crew thread-default UUID/ledger/worktree identity preserved. Must account equivalent crash/requeue/exact-thread behavior and deliberate API/default exception. Aggregate per-routing-channel queue bound and mixed-case root normalization need separate review; neither is established equivalent merely because threads already run concurrently. |
| `2f3dd850d` #6961 | ACP rate-limited OK correlation and refused observer-frame requeue | Important compatible remaining relay.rs port; paired with root's relay acknowledgement changes. Preserve #340 channel-local CLOSED recovery. |
| `93237b4a7` #6953 | complete workflow owner contract | Implemented, tests above; relay Postgres result pending. |
| `f463e726d` #6950 | base prompt cold memory CLI commands and owner approval guidance | Remaining prompt wording update. No engram fetch algorithm change in this commit. |
| `69096c9a8` #6946 | safe multiline channel descriptions with paragraph preservation | Remaining; depends on semantic escaping in framing slice. |
| `8f3566ad9` #6706 | prompt completeness | Implemented, tests above. |
| `de188ebf0` #6590 | project-home resolver/new prompt_project.rs, queue metadata, pool context, relay project APIs, config builders | Largest remaining additive feature. Adapt repository/project distinction and retain Crew workspace authority. Six files, ~1100 changed lines upstream. |
| `f177f4909` #6701 | paired standing/per-turn semantic prompt sections | Remaining; share framing helpers with #6501, preserve Crew edit/context/compaction additions. |
| `f99532585` #6501 | semantic prompt framing, engram section, base prompt structure | Remaining: new prompt_framing.rs plus adapted queue/engram/pool/base changes. |
| `025425591` #6487 | completed-before-control stop logging + sorted delivered-event diagnostic | Remaining small pool port; preserve Crew observed rotation and success-before-receipt ordering. |

Paths requiring reconciliation beyond priority files: `base_prompt.md`, `config.rs`, `engram_fetch.rs`, new `prompt_framing.rs`, new `prompt_project.rs`, README; scope/session-model files require deliberate Crew equivalence/exception accounting. `pool.rs`, `queue.rs`, `lib.rs`, `relay.rs` also carry the remaining framing/project/scope/observer work; never label them fully integrated from these two ports alone.

Unresolved questions: none for implemented slice. Complete release integration still needs the above source/test evidence and final scope compatibility decision recorded by parent.

## Remaining released ACP slices completed

- #6487: completion-before-control and ordinary successful turns record sorted event IDs, with routing-channel diagnostics while delivery state remains keyed by conversation UUID. Observed rotation and success-before-receipt semantics preserved.
- #6501/#6701: actual working-directory resolution errors instead of inventing `/`; static base precedes dynamic workspace/persona; paired standing and per-turn sections retain authored body bytes. Crew checkout notice remains inside the workspace boundary, and Crew role/routing/org context and edit-body replacement remain intact. `prompt_framing.rs` provides released helpers. Imported huddle standing sections absent from Crew were not invented.
- #6590: `prompt_project.rs` validates bounded project-home identity through existing NIP-MP project/repository events. Project authority resolves before ACP session creation, initial message, and workspace lease, using **routing channel** from the batch. The exact typed result is reused for prompt context. Indeterminate local relay state preserves healthy ACP process and bounded retry; existing Crew UUID/worktree/ledger authority is unchanged.
- #6946: every prompt refreshes channel metadata with bounded fallback; descriptions preserve paragraphs and escape semantic delimiters. Cached metadata refresh is one attempt; first discovery retains existing retry.
- #6950/#7250: cold-memory CLI/hygiene guidance and generic display-name mention example; retained Crew publish_message guidance and evidence/acceptance requirements. Session-model prose now describes actual Crew thread-default/DM behavior.
- #6961: only rate-limited correlated `OK` acknowledgements pause observer publishing; only the refused observer frame is reparked with bounded capacity. Previously ported Crew #340 channel-local CLOSED recovery remains.
- #6730: planner renamed/nested six Postgres tests under `postgres_tests`; discovery guard passes and the actual committed Postgres runner passed all six. Earlier workflow sink full integration run passed 25/25 after migrating fresh `buzz_0522_workflow`.
- #6732 compatible backport: 500 ready-queued events **aggregate across all conversation UUIDs in a routing channel**. Applies on normal push, retry, no-slot restoration, native-steer release and orphan recovery. Evicts globally oldest head, preserves remaining per-thread order and separates channels. Existing per-conversation cap still applies. Like released upstream this bounds ready queues; running, cancelled and withheld entries retain their separate lifecycle/caps.
- Cancellation `control_result` now echoes optional `requestId` while retaining exact conversation/turn targeting and Crew queued-cancel behavior.

## Deliberate session-scope exception

Crew keeps its verified thread-default identity: `conversation::id_for_event` hashes channel UUID + raw root with `buzz-acp-conversation-v1`; top-level channel messages establish independent thread sessions, DMs retain channel identity. No new channel-default flag, `SessionScope` replacement, or ledger-key migration was adopted. Released upstream lowercases root text; Crew retains historical raw-case hashing. Case normalization requires explicit identity/ledger migration and remains a separate follow-up. Queue cap was a real non-equivalence and is fixed above. Released owner/busy-thread scheduling does not independently solve all starvation scenarios; no post-release #7337 behavior is claimed.

## Final package evidence

- `/tmp/acp-release-final-tests.log`: **1175 ACP library + 14 integration tests passed**. The command then exposed two existing prose blocks treated as Rust doctests; marked the explanatory block `text`, with isolated doctest rerun in `/tmp/acp-release-doctests.log`.
- `/tmp/acp-release-clippy.log`: `cargo clippy -p buzz-acp -p buzz-workflow --all-targets -- -D warnings` passed.
- `/tmp/crew-0522-workflow-postgres.log`: **6/6 Postgres regressions passed** through committed runner after test discovery relocation.
- New tests cover aggregate multi-root bound, independent channels, FIFO survivors, failed/no-slot requeue, both native-steer restore paths, deterministic delivery diagnostics, and exact checkout framing. Existing production listener -> signed workflow -> concurrent ACP wire test remains green.
- `git diff --check` passed for ACP/workflow/workflow_sink. No index writes, commits, pushes, hosted provider calls, or UUID migrations by this worker. Parent still owns independent review and whole-upgrade gates.

Docs impact: minor; this report and base prompt updated. Root owns release changelog/compatibility record and ACP README. Unresolved questions: none for these ports; raw-case identity migration intentionally outside this upgrade.
