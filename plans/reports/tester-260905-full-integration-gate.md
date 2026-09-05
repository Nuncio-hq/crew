# Full integration gate — 2026-09-05

Status: DONE. All required integration suites pass on final source. Docs impact: minor (metrics adaptation captured here and in coverage ledger).

## Wrapper-equivalent execution

`just test` runs `scripts/run-tests.sh all`. Root separately runs the infra-free `just test-unit` lane. This worker executed the integration function's exact test selections, replacing only its unsafe hardwired infrastructure bootstrap:

- `run-tests.sh` overwrites caller database/Redis exports with localhost5432/6379 defaults when `.env` is absent. Confirmed `.env` absent; did not read or create it.
- `_ensure-migrations` depends on `_ensure-services`, which checks hardwired `buzz-postgres`/`buzz-redis` container names and may start default Compose. Dedicated test services use different names. Did not invoke default-stack startup or migrate a shared database.
- Verified healthy `buzz-harness-postgres-1` and `buzz-harness-redis-1` under `colima-crew-recovery`. Created only `crew_upgrade_integration_260905_acp` on localhost5471 using public fixture credentials (`buzz`/`buzz_dev`). Reserved empty Redis DB14 on localhost6471 after coordination with the live-smoke worker using DB15.
- Ran the same migration and seed commands directly against that fresh database: `cargo run -p buzz-admin -- migrate`, then `scripts/seed-local-community.sh`. All **45 migrations** applied successfully; local host rows seeded only in the scratch DB.
- Ran `cargo test -p buzz-db -- --nocapture`, `scripts/postgres-test-run.sh`, and `cargo test --no-fail-fast --test '*' -- --nocapture`. `--no-fail-fast` preserves test selection and collects every target result. Kept stderr, unlike the wrapper's workspace invocation.
- `buzz-auth/tests` contains no integration targets; wrapper's no-integration-tests case applies.
- Added `/tmp/crew-nextest-0.9.136` to PATH for the PostgreSQL wrapper. Initial launch lacked this tool in Hermit PATH; corrected without changing repository tooling.

## Final results

| Gate | Result | Log |
| --- | --- | --- |
| Fresh scratch database migration/seed | 45 migrations, passed | `/tmp/crew-full-integration-migrate.log`, `/tmp/crew-full-integration-seed.log` |
| buzz-db regular tests | **126 passed, 253 ignored** | `/tmp/crew-full-integration-db-final.log` |
| Isolated PostgreSQL lane | **379 passed**, 1463 excluded by lane selection | `/tmp/crew-full-integration-postgres.log` |
| Workspace integration targets | **314 passed, 268 ignored**, 46 target runs | `/tmp/crew-full-integration-workspace-final.log` |
| Final modified huddle query on PostgreSQL | **1 passed** | `/tmp/crew-final-huddle-metrics-postgres.log` |
| Strict observability source guards | **3 passed** | `/tmp/crew-final-metrics-guard.log` |
| buzz-db + buzz-relay compile | Passed | `/tmp/crew-final-metrics-check.log` |
| Two changed production files rustfmt + diff-check | Passed | `/tmp/crew-final-metrics-fmt.log` |

Counts above are per lane, not unique-test totals; some source guards intentionally overlap. Live relay/client E2E tests remain ignored by the repository's normal integration selection; this is not a claim that every opt-in live test ran. Separate root/pool live-smoke evidence covers that surface.

## Concrete failures fixed

The full integration target suite exposed omissions not covered by the earlier lib-only/unit runs. Root approved both production corrections; no guard was weakened.

1. `crates/buzz-relay/src/workflow_sink.rs`: publishing workflow events still called generic `get_channel`, `get_members`, and `get_users_bulk`. Switched exactly these three production calls to the released `_for_event_write` variants. Same query helpers, writer pool, data and authorization; explicit event-write operation attribution now reaches metrics.
2. `crates/buzz-db/src/store/event.rs`: Crew's batched `huddle_started_links` read bypassed operation attribution through `fetch_all(pool)`. After the existing empty-input early return, acquire one writer connection with `WriterOperation::Authorization`, then execute the unchanged set-based query on that connection. This matches the creator-authenticated link proof used for huddle liveness access. SQL predicates, binds, batching and community boundaries are unchanged.

Independent planner review passed: wrappers preserve underlying query/return behavior and empty fast paths; huddle authorization acquisition preserves every query predicate and uses the same writer pool. No correctness findings.

## Cleanup and boundaries

Removed the owned scratch database after all tests completed. PostgreSQL wrapper cleaned its unique desired-state templates. Redis DB14 remained empty, so no flush was necessary. No shared database changes, `.env` access, production deployment, index updates, commits or pushes. Root retains overall gate/integration ownership.

Unresolved questions: none.
