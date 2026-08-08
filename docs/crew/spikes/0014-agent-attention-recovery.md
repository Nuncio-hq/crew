# Spike 0014 — Can Crew distinguish agent liveness from progress with existing events?

**Issue:** [#105](https://github.com/Nuncio-hq/crew/issues/105)
**Date:** 2026-08-08
**Verdict:** PASS for a bounded desktop projection; FAIL for enabling job lifecycle kinds; INCONCLUSIVE for cross-runtime Known wait.

## Question

Can Crew surface `Possibly stalled`, `Lost contact`, `Telemetry unavailable`,
`Needs you`, and receipt-backed `Ready to review` without inventing a Crew-only
task registry or enabling kinds `43002`–`43006`?

## Decision affected

Issue #105 blocks production implementation until the authority, replay, health,
and durable terminal/review seams are frozen.

## Hypothesis

Existing Buzz seams are sufficient if Crew:

1. separates observer-frame freshness from semantic progress;
2. treats stall as a projection, not a durable event;
3. reconstructs pending user input and receipts from durable events; and
4. uses an owner-authored NIP-25 check reaction as explicit review evidence.

## Scope

Inspected:

- ACP observer frames and liveness emission;
- desktop observer, active-turn, Needs You, outcome, Mission Inbox, and receipt paths;
- relay admission and database feed queries;
- replay, reconnect, deduplication, clock skew, file-size ratchets, and test seams.

Three independent read-only agents audited protocol/state, desktop/UI ownership,
and RED-to-GREEN verification.

Excluded:

- a new task database or Crew-only lifecycle registry;
- enabling ACP job lifecycle kinds `43002`–`43006`;
- claiming a generic executing tool is a trustworthy Known wait;
- a new request-changes protocol event.

## Pass/fail criteria

PASS requires:

- `lastSeenAt` and `lastSubstantiveProgressAt` have distinct authorities;
- heartbeat/token/usage frames cannot hide a stall;
- unhealthy or untrusted telemetry cannot masquerade as a stall;
- unresolved kind `46040` and terminal kind `46043` survive restart/reconnect;
- reading a thread is not review;
- no new authoritative Crew-only registry is introduced.

FAIL if job lifecycle kinds are required for the first complete slice, or if
current durable events cannot reconstruct Needs You and review readiness.

## Environment

- Repository: `Nuncio-hq/crew`
- Baseline: `origin/main` at `bf926054456322e3397dc4e9494e60b575a1801b`
- Branch: `feat/agent-attention-recovery`
- Baseline `pnpm run check`: PASS

## Method

1. Traced `run_prompt_task` liveness and terminal guards through observer kind
   `24200` publication and desktop ingestion.
2. Traced `46040 → 46041 → 46042` elicitation and kind `46043` receipt paths.
3. Compared live subscriptions with cold feed/history reconstruction.
4. Located every current status derivation and recovery action.
5. Added RED contracts for heartbeat-only progress, telemetry health, durable
   user input, receipt-backed readiness, and explicit review.

## Results

### Authority

Per `(agentPubkey, turnId)`:

- `lastSeenAt` advances on an authoritative current-turn observer frame.
- `lastSubstantiveProgressAt` advances only on a classified semantic transition:
  turn start, changed plan, tool start/status transition, retry phase, or another
  explicit structured transition.
- `turn_liveness`, token/thought chunks, usage counters, duplicate frames, and
  raw transport reads/writes do not advance substantive progress.
- deduplication keeps the existing composite observer watermark for this bounded
  slice; signed durable events deduplicate by event id and protocol references.

### Telemetry

- Active turn + observer `open` + recent current-turn lease: telemetry trusted.
- Trusted observer connection + stale lease: `Lost contact`.
- Observer `idle|connecting|closed|error`, or no trusted current-turn sample:
  `Telemetry unavailable`.
- `Possibly stalled` is permitted only with trusted live telemetry and stale
  substantive progress.

### Known wait

No current protocol fact proves an expected wait. An executing tool alone is
not enough. The projection retains a Known-wait type but v0 does not emit it
until a typed reason/expected window exists.

### Durable attention

- `46040` is durable but absent from the native Needs Action feed. Exhaustive
  paginated hydration for `46040`–`46042` overlaps live subscriptions and
  reconstructs pending requests without weakening owner authorization.
- `46041` is accepted only from the request's intended verified owner. A
  same-owner sibling is still an agent, not a substitute for human input.
- kind `46043` already survives relay/thread history. Adding it to Home activity
  makes receipt-backed readiness available to Mission Inbox.
- owner-authored `✅` kind `7` reaction linked to the exact receipt is explicit,
  durable, idempotent review evidence. Ambient read state is not review.
- `Request changes` remains an ordinary durable reply in the same thread; the
  existing conversation/worktree routing continues the same mission.

### Job lifecycle kinds

Do not enable `43002`–`43006` for #105. They lack a complete ACP emission,
relay authorization, correlation, and conflict-resolution contract. Enabling
them would create a second authority instead of fixing the existing seam.

## Edge cases

- `turn_completed` can precede a later `turn_error`; error dominates.
- a newer turn suppresses older terminal/review evidence for active-state UI.
- a replayed request cannot resurrect after a known answer/resolution tombstone.
- community reset clears module-level receipt and snooze projections.
- delayed replay must not be treated as fresh live telemetry.

## Limitations

- Cross-runtime progress taxonomy is intentionally conservative.
- Known wait remains unreachable until explicit evidence exists.
- The first implementation uses desktop-side paginated history queries rather
  than changing the owner-addressing schema of kind `46040`.
- Repeated live certification is still required for the issue's percentage
  targets; one deterministic run cannot prove those operational metrics.

## Verdict

**PASS**: proceed with the bounded desktop projection and existing durable event
seams. **FAIL** the proposal to enable job lifecycle kinds in this change.
**INCONCLUSIVE** for generic Known wait, so do not emit it in v0.

## Follow-up test contract

Blocking tests must cover:

- heartbeat and token stream update `lastSeenAt` but not progress;
- stale progress + healthy telemetry → `possibly-stalled`;
- telemetry loss never → `possibly-stalled`;
- kind `46040` remains until `46042` and reconstructs after cold start;
- completed observer outcome without receipt does not create review readiness;
- valid receipt creates exactly one ready row;
- read state does not clear it;
- owner `✅` reaction clears only the exact reviewed receipt;
- all state surfaces use the same precedence.

Playwright supplies integrated state/card/navigation evidence, but correctness is
also pinned in blocking Node/Rust tests because desktop smoke CI is advisory.

## Cleanup

The read-only audits modified no files. Prototype artifacts remain under local
untracked `.hermes/` and must not be included in the production PR.
