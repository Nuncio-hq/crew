# Native released-source gap audit

Read-only audit of released `crates/` paths changed between 0.5.18 pin `39f8b46935736334cdd7045a4e4b5d7eb1a33888` and target `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`; excludes this worker's substrate, ACP, and workflow ownership. Compared actual file bytes with target and original Crew base. Detailed snapshot: [260905-native-gap-audit.json](260905-native-gap-audit.json).

Two concrete findings sent to root:

1. Relay `router.rs` duplicated GET registrations for `/workflows/{workflow_id}/runs` and `/workflows/{workflow_id}/runs/{run_id}/approvals`. Target registers each once; duplicate Axum route construction can panic although Rust compiles. Root owns removal.
2. `buzz-test-client/tests/e2e_project.rs` still matched old upstream bytes (491 vs target603 lines). Missing released NIP-OA helper and `test_agent_owner_can_delete_agent_project_but_third_party_cannot` coordinate-deletion regression. Root owns target import.

Other differences inspected: CLI message source/deep-link parser and Crew commands; dev-MCP wiki/desktop additions; relay NIP-OA owner backfill, receipt/workflow/wiki authorization; SDK Crew builders; migration-number include fix. These are retained Crew deltas, not evidence of missing released behavior. No other missing or unchanged-old native source paths found in the bounded comparison. Workflow sink remains under ACP worker; desktop/native app audited by its owner. This does not close CI/docs/release assets or E2E execution coverage.

Both findings now resolved by root: router.rs and e2e_project.rs are target-exact on final live byte comparison. Status: DONE.
