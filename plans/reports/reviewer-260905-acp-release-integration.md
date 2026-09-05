# Independent ACP/workflow release review

Read-only review of ACP/workflow/workflow sink in `/Users/a1241968/Desktop/Oscar/crew-wt/upstream-0522`, after the implementation report's completed release section. No edits to those packages by reviewer.

One actionable release gap found: `crates/buzz-acp/src/config.rs` still used standalone idle 900 seconds and locked that value in a regression test, although released #7185 raises it to 1500. Root confirmed this is not an intentional exception. Sent to ACP worker to port with budget tests while preserving tracked-tool2400 and managed-pool1800. Parent tracks final fix evidence.

No additional correctness findings from the reviewed boundaries:

- Authored workflow message text passes from stored step through executor/action sink; only targets present in authored and rendered mentions receive authority-bearing tags. Listener attribution validates relay signature, NIP-11 identity, canonical unique owner/mention tags and current target before applying owner/allowlist policy.
- Normal and setup listeners consume the same private authorized capability; subscription matching, raw-signer org routing/control, edit hold and exact conversation UUID routing remain distinct. NIP-11 retry/generation logic retains upstream availability tradeoff on failed refresh; no new bypass identified.
- Aggregate routing-channel ready-queue cap covers normal push, failed/no-slot requeue, native steer release and orphan recovery, evicts oldest head across threads and preserves independent channels. Running/cancelled/withheld state remains separately bounded as described in implementation report.
- Project home resolution validates repository membership/maintainer authority and ambiguity; exact typed context resolves before ACP session creation/workspace lease. Indeterminate result requeues with bounded retry and returns a healthy process to pool. Metadata refresh uses bounded cached fallback.
- Rate-limited OK handling correlates refused observer event ID and reparks only that event; permanent refusal retires it. Existing per-channel CLOSED recovery remains.
- Prompt semantic framing preserves authored bodies; untrusted metadata/attributes use escaping; actual cwd errors propagate rather than inventing root workspace. Crew thread-default raw-root identity exception remains explicit, with no silent ledger migration.

Implementation evidence inspected: report records 1175 library +14 ACP integration tests, doctest follow-up, package clippy and six Postgres regressions. This review did not duplicate those full runs; native compilation also traversed affected shared dependencies.

Status: DONE_WITH_CONCERNS pending the accepted 900-to-1500 default correction. No other unresolved questions.
