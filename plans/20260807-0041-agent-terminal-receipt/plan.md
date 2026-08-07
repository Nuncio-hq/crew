# Agent terminal receipt plan

## Scope and contract

- Add the next free workflow kind, `46043`, as `KIND_AGENT_RECEIPT`.
- Receipt events are durable channel events with an `h` channel tag and an `e`
  tag for the terminal turn’s thread root. The builder and validator must agree
  on the existing NIP-10 root-marker convention used by ACP thread parsing.
- Content shape:
  `{summary, verify, lights: [{label, status}], engineering: {pr_ref?, branch?, files_changed?, ci?}}`.
  Validate required string/array/object types and bounded field sizes; reject
  malformed content at ingest.
- Gate emission behind `--agent-receipts` / `BUZZ_ACP_AGENT_RECEIPTS`, default
  `false`. No receipt is emitted for failed, cancelled, or summary-less turns.
- No mobile UI, action buttons, or integration-suite work in this change.

## Verified implementation seams

- `crates/buzz-core/src/kind.rs`: agent user-input kinds `46040`–`46042` and
  `ALL_KINDS`; add `46043` beside them with the schema doc comment.
- `crates/buzz-relay/src/handlers/ingest.rs`: `required_scope_for_kind`,
  `requires_h_channel_scope`, `ingest_event_inner`, and the kind `44200`
  `validate_agent_turn_metric_envelope` + `db.is_agent_owner` pattern.
- `crates/buzz-relay/src/handlers/event.rs`: `kind_label` range currently ends
  at `46042`.
- `crates/buzz-sdk/src/builders.rs`: existing durable user-input builders and
  colocated builder tests provide the event-construction seam.
- `crates/buzz-acp/src/elicitation.rs`: `QuestionRuntime::publish` and
  `publish_resolution` use `RelayEventPublisher::publish_event` for `46040`+
  events; `pool::run_prompt_task` owns terminal success/failure branching.
- `crates/buzz-acp/src/acp.rs`: `agent_message_chunk` is observed but not
  accumulated; the receipt needs an explicit per-turn summary accumulator.
- `desktop/src/shared/constants/kinds.ts`,
  `features/messages/lib/formatTimelineMessages.ts`, and
  `features/messages/ui/MessageRow.tsx`: kind sets, timeline admission, and
  structured body dispatch.
- `mobile/lib/shared/relay/nostr_models.dart`: `EventKind` constants; add the
  constant only, per task scope.

## Implementation phases

### 1. Core and SDK event contract

Files/symbols:

- `crates/buzz-core/src/kind.rs`: add `KIND_AGENT_RECEIPT = 46043` and include
  it in `ALL_KINDS`.
- `crates/buzz-sdk/src/builders.rs`: add
  `build_agent_receipt(channel_id, thread_root_id, content)` using the same
  signed-event builder path and validated h/e tags as the user-input events.

Tests:

- `agent_receipt_kind_is_registered`
- `agent_receipt_builder_sets_kind_channel_and_thread_root`
- `agent_receipt_builder_rejects_invalid_thread_root`

### 2. Relay admission and ownership

Files/symbols:

- `crates/buzz-relay/src/handlers/event.rs::kind_label`: extend the workflow
  range through `46043`.
- `crates/buzz-relay/src/handlers/ingest.rs`: add `46043` to the messages-write
  scope allowlist and channel-scope enforcement; add a focused receipt envelope
  validator called from `ingest_event_inner` before DB writes.
- Reuse the existing agent ownership lookup used by kind `44200`; keep the
  check fail-closed and ensure the event signer is the agent authorized for the
  referenced terminal turn/root.

Tests in the existing `ingest.rs` test module:

- `agent_receipt_kind_is_in_messages_write_scope`
- `agent_receipt_requires_h_and_e_tags`
- `agent_receipt_rejects_missing_h_tag`
- `agent_receipt_rejects_missing_e_tag`
- `agent_receipt_rejects_malformed_json`
- `agent_receipt_rejects_wrong_author`
- `agent_receipt_accepts_canonical_payload`

### 3. ACP terminal publishing and flag wiring

Files/symbols:

- `crates/buzz-acp/src/config.rs`: add the clap/env flag to `CliArgs`, map it
  in `Config::from_args`, include it in `Config::summary`, and update all
  `Config` fixtures in `config.rs` and `lib.rs`. Document it in
  `crates/buzz-acp/README.md` and the ACP section of `.env.example`.
- `crates/buzz-acp/src/pool.rs::PromptContext`: carry the feature flag, the
  relay event publisher, and the turn’s channel/root identity.
- `crates/buzz-acp/src/acp.rs`: accumulate `agent_message_chunk` text per
  prompt, expose a take/reset operation, and ensure a new prompt cannot inherit
  the previous summary.
- `crates/buzz-acp/src/pool.rs::run_prompt_task`: on the successful terminal
  branch only, build the receipt payload and publish it through the same
  `RelayEventPublisher::publish_event` path used by `QuestionRuntime`; publish
  before returning the successful `PromptOutcome`. Keep every error/cancel/
  timeout branch receipt-free.

Tests:

- `receipt_flag_defaults_off`
- `receipt_flag_parses_env`
- `agent_message_chunks_accumulate_and_reset_terminal_summary`
- `successful_turn_with_summary_publishes_agent_receipt`
- `successful_turn_without_summary_publishes_nothing`
- `failed_turn_does_not_publish_agent_receipt`

### 4. Desktop structured receipt card

Files/symbols:

- `desktop/src/shared/constants/kinds.ts`: add `KIND_AGENT_RECEIPT`; add it to
  the channel event/timeline sets. Decide its unread classification explicitly
  while preserving the existing “message kinds drive unread” rule.
- `desktop/src/features/messages/lib/formatTimelineMessages.ts::isTimelineContentEvent`:
  admit kind `46043`.
- Add `desktop/src/features/messages/lib/agentReceiptModel.mjs` as the pure
  JSON-to-render-model parser and colocated
  `agentReceiptModel.test.mjs`.
- Add `desktop/src/features/messages/ui/AgentReceiptMessage.tsx` and dispatch
  it from `MessageRow.tsx` for `KIND_AGENT_RECEIPT`.
- Render a summary line, lights/status rows, verification text/link, and a
  closed-by-default native details section for engineering fields. No action
  buttons. Return the ordinary Markdown/raw-body fallback for malformed JSON.

Node tests:

- `parseAgentReceipt_maps_summary_lights_verify_and_engineering`
- `parseAgentReceipt_keeps_engineering_collapsed_by_default`
- `parseAgentReceipt_returns_null_for_malformed_json`
- `parseAgentReceipt_rejects_invalid_field_types`

### 5. Mobile constant

- `mobile/lib/shared/relay/nostr_models.dart::EventKind`: add
  `agentReceipt = 46043`; do not add mobile UI or query behavior in this task.

## Verification commands

Run from the repository after implementation, with no Postgres/Redis-dependent
integration suite:

```bash
. ./bin/activate-hermit
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p buzz-core -p buzz-relay -p buzz-acp
pnpm run check
cd desktop && pnpm run test
cd ../mobile && dart format --output=none --set-exit-if-changed .
```

Also run the focused Node test directly if the desktop test script does not
cover new files, and verify `git diff --check`. In ACP tests, unset
`BUZZ_ACP_LAZY_POOL` if the harness injects it; rerun ACP tests serially with
`-- --test-threads=1` if parallel process-global environment tests are flaky.

## Unresolved questions / concerns

1. This checkout has no dedicated relay validator for kind `46040`; the issue’s
   “mirror 46040 ownership-check pattern” cannot be verified literally. The
   plan uses the concrete kind `44200` DB ownership seam and requires the exact
   turn/root-to-agent binding to be resolved before implementation.
2. The requested receipt schema has no explicit agent or turn identifier beyond
   the `e` root and event signer. Confirm whether root-event lookup is the
   authoritative binding or whether a stable `agent`/`turn_id` tag is required.
3. The ACP currently exposes `StopReason`, not a result summary. The summary
   source must be the accumulated `agent_message_chunk` text unless the harness
   supplies a separate structured result.
4. The issue body mentions downstream consumer upgrades (#81/#83); they remain
   out of this bounded task unless explicitly added to acceptance criteria.
