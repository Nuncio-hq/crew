# Stable 0.5.22 substrate integration

Status: substrate implementation and PostgreSQL validation complete; required Crew CI wiring added. Worktree `crew-wt/upstream-0522`; no git index mutations or commits by this worker.

## Scope and preserved Crew contracts

Imported released DB runtime/store layout and full auth/search source from `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`. Core adopts target network changes and kind 48104 huddle liveness while retaining Crew workflow/receipt/wiki/overlay kinds and modules. DB facade follows the released upstream seam; Crew receipt kind 46043 remains in activity query and thread reply counters.

All 33 existing Crew migrations remain byte-identical to Crew `871eecb18d7a243d87ec56a2eb154fbf2099d7ce`, including wiki allowlist 0031, workflow error codes 0032, and roster fence 0033. SHA-256 evidence in [260905-substrate-import.json](260905-substrate-import.json). New release migrations preserve target SQL bytes and append after those applied identities:

| Upstream | Crew | Migration |
|---|---|---|
| 0033 | 0034 | private_managed_agent_fts |
| 0034 | 0035 | replica_heartbeat_vacuum_truncate |
| 0035 | 0036 | relay_operators |
| 0036 | 0037 | relay_admin_actions |
| 0037 | 0038 | relay_admin_action_lease |
| 0038 | 0039 | relay_admin_outbox_claim_token |
| 0039 | 0040 | relay_operator_audit |
| 0040 | 0041 | push_message_kinds |
| 0041 | 0042 | nip_fi_identity_foundation |
| 0042 | 0043 | nip_fi_authorization_foundation |
| 0043 | 0044 | push_gateway_dogfood_profile |
| 0044 | 0045 | drop_nip_fi_ledger |

Migration tests/reference paths follow the new numbers. The brownfield regression applies unchanged Crew migrations through 33, preserves already-inserted wiki kinds 30023/30623, checks the live roster fence before/after applying 34–45, and verifies author-only kinds 30179/30350 remain excluded from FTS. Fresh install verifies Crew wiki allowlist, tenancy constraints, write fencing, and migration catalog. NIP-FI table creation and final released removal are both retained; populated ledger teardown is tested.

## Required schema wiring

Imported `scripts/reconcile-schema-after-pgschema.sql` and target `schema/schema.sql` (Crew roster migration reference retained). Replaced old partition-only companion calls in:

- `scripts/start-isolated-test-relay.sh`
- `scripts/start-relay-for-tests.sh`
- `scripts/run-nuncio-crew-project-relay-ci.sh`
- `.github/workflows/ci.yml` (both callers)
- `.github/workflows/nuncio-crew-ci.yml`

Narrow include-path fixes: relay `handlers/push_lease.rs` uses 0041; search FTS integration uses 0034. No broader relay/CI rewrite by this worker.

## Validation

All commands activate Hermit, use shared build target, and pin PostgreSQL to isolated port 5471. Final commands set `BUZZ_TEST_DATABASE_URL`, `TEST_DATABASE_URL`, and `DATABASE_URL` because released helpers use differing keys.

- `cargo check -p buzz-core -p buzz-auth -p buzz-db -p buzz-search`: PASS.
- Four library unit suites: auth 188, core 279, DB 123, search 3 PASS; DB Postgres cases separately exercised.
- Seven migration Postgres tests: PASS, including real `bin/pgschema` admin table/index parity, fresh install, populated upgrade, populated NIP-FI drop, and cancellation/destruction locking.
- Search integration: 19 PASS; covers community isolation, author-only privacy, prefix search, query boundaries, pagination.
- Full shared-database DB run: 248 PASS, five push cases failed due shared global queue leftovers and one remote-clock assumption. These were investigated, not suppressed.
- All 17 push tests: PASS in individual fresh databases with production fences intact. The ordering test now gets retry timestamp from PostgreSQL rather than host time, matching queue defaults; production code unchanged.
- Remaining 236 PostgreSQL tests: PASS on final serial rerun.
- Actual committed nextest runner: **272/272 DB+search PostgreSQL tests PASS in 36.774s** with desired-state setup and per-test wrappers active (129 unit tests intentionally filtered).
- `cargo fmt -p buzz-core -p buzz-auth -p buzz-db -p buzz-search`: PASS.

Logs: `/tmp/crew-0522-substrate-units.log`, `/tmp/crew-0522-substrate-migrations.log`, `/tmp/crew-0522-substrate-search.log`, `/tmp/crew-0522-substrate-db-nonpush-final.log`, `/tmp/crew-0522-substrate-push-isolated.log`, `/tmp/crew-0522-substrate-nextest.log`.

Committed CI uses separate databases for globally claiming push scenarios via `.config/nextest.toml` and the released setup/wrapper scripts. Added required `postgres` job to NuncioCrew CI, existing relay path relevance, merge-gate result check, rejection tests, and `just test-postgres`/`run-tests.sh` invocation. Restored executable bits on the four imported runner scripts. Disabled upstream CI remains disabled.

The runner demonstrated why isolation is required: Sharing the deletion-test database leaves deliberately fenced tenants; global queue DELETE then correctly fails closed. No test or migration disables those guards.

Docs impact: minor; this report, checksum map, and coverage ledger document the migration numbering and validation. Root owns release changelog/roadmap and final pin updates.

PostgreSQL discovery guard PASS across 494 Rust source files after six workflow tests were nested under postgres_tests; wrapper selftest PASS. Crew CI contract14/14, YAML syntax, Justfile parser PASS. Auth verifier compile-fail doctests3 PASS.

Unresolved: selected six workflow Postgres regression run in progress (ACP owner resumed file ownership); independent review remains root-owned. No unresolved substrate design questions.
