# Spike 0025 — ACP `sessionUpdate: plan` live wire (#190)

- **Status:** PASS (in-repo + spike-asset evidence; live Hermes/Claude/Codex
  adapters were not on this VM)
- **Date:** 2026-08-13
- **Issue:** [#190](https://github.com/Nuncio-hq/crew/issues/190)

## Question

What is the real structured plan payload for Hermes, Claude Code, Codex, and
Grok/buzz-agent, and does `buzz-acp` already forward `sessionUpdate: plan` on
the observer path or drop it after the log line?

## Decision affected

D-056 / issue #190 — thread-visible declared plans. Authority is the latest
ACP snapshot per `(agentPubkey, conversationId)`, never a shared backlog.

## Hypothesis

`handle_session_update` logs `"plan update received"` and returns `false`.
The raw `session/update` frame is still observed as `acp_read` before that
match arm, so desktop can already see the payload if the adapter sent
`entries[]`.

## Scope

- In-repo ACP client, observer bus, desktop transcript parser, session ledger
  key, and prior spike assets
- Files:
  - `crates/buzz-acp/src/acp.rs` (`observe` then `handle_session_update`)
  - `crates/buzz-acp/src/observer.rs` / `session_ledger.rs`
  - `desktop/src/features/agents/ui/agentSessionTranscriptHelpers.ts`
  - `desktop/src/features/agents/conversationId.ts`
  - `desktop/src/features/messages/lib/sessionAgingStore.ts`
  - `docs/crew/spikes/assets/0018-spawn-granularity/`
- Time: source inspection on this cloud VM; no live `hermes-acp` /
  `claude-agent-acp` / `codex-acp` binaries

## Exclusions

- Live adapter capture on a machine with Hermes/Claude/Codex installed
- Inferring tasks from markdown, Mission markers, or GitHub issues
- Thread Workbench (#186) and Mission (#151)

## Pass criteria

Each runtime is labeled with evidence: native `plan` update, structured todo
tool only, or unknown. Observer forward vs drop is cited. Binding key is
cited. Explicit unknown is success.

## Fail criteria

Claiming a runtime has a plan protocol without a payload. Inventing a Crew
task store. Guessing Claude/Codex/Grok shapes.

## Environment

- Commit: Crew `main` at implementation start (`c67cd951a`)
- OS: Linux cloud agent
- Live adapters: **not present** (`~/.hermes/hermes-agent/acp_adapter/events.py`
  missing; no `hermes` / `claude` / `codex` on `PATH`)
- Authentication class: none

## Method

1. Read `handle_session_update` and the stdout read loops.
2. Read desktop `extractPlanText` + transcript tests.
3. Search spike assets for `"sessionUpdate": "plan"` and `todo_write`.
4. Read session ledger + `deriveAgentConversationId`.
5. Search buzz-agent tests for plan updates.

## Results

### 1. Hermes ACP `sessionUpdate: plan`

**Not live-captured on this VM.** The live adapter path named in the issue
(`~/.hermes/hermes-agent/acp_adapter/events.py`) is absent here.

In-repo evidence (labeled as such):

- Desktop already documents the standard ACP shape as `entries[]` of
  `{ status, content, priority }` with statuses
  `{pending, in_progress, completed}`. `entries` (even empty) is
  authoritative; `content` is only used when `entries` is absent
  (`agentSessionTranscriptHelpers.ts` `extractPlanText`).
- Tests pin empty `entries: []` as an empty checklist, not JSON fallback
  (`agentSessionTranscript.test.mjs`).
- The issue states Hermes translates its `todo` tool result into native ACP
  `sessionUpdate: plan`, including cancelled rows as `completed` with a
  `[cancelled]` prefix. That adapter behavior was **not re-verified live**.

Empty-list semantics used for #190: `entries: []` clears the retained
snapshot / rail card.

### 2. Claude Code ACP and Codex ACP

**Not live-captured.**

| Runtime | Evidence | Verdict |
| --- | --- | --- |
| Codex ACP | Upstream changelog: "parse codex ACP plan entries[] into checklist" (`CHANGELOG.md`, Buzz #1824). Comment on `extractPlanText` names `@agentclientprotocol/codex-acp` as the `entries[]` sender. | Native `plan` update **with `entries[]`**, from in-repo parser evidence, not a live session. |
| Claude Code ACP | No `sessionUpdate: plan` frames in spike assets. Desktop todo classifier matches buzz-dev-mcp `todo` exactly, not `TodoWrite`. | **Unknown** as a native plan protocol. If Claude only emits `TodoWrite` / similar structured todos, #190 uses the existing `parseAgentPlanTodos` fallback. Do not claim a native plan update. |

### 3. Does `buzz-acp` forward or drop plan updates?

**Forwards the raw frame; did not retain structured state (pre-#190).**

Evidence:

- Every stdout JSON line is `observe("acp_read", msg.clone())` **before**
  `handle_session_update` (`acp.rs` `read_until_response` and the idle read
  loop).
- The `"plan"` arm only logged and returned `false` (no idle-clock reset, no
  client field).
- Desktop `agentAttention.ts` already fingerprints `sessionUpdate === "plan"`
  as `"Plan updated"`, which only works if the observer frame arrived.
- Observer context already carries `channel_id`, `conversation_id`,
  `session_id` (`observer.rs`).

#190 therefore starts by **retaining** the structured `entries[]` snapshot on
the ACP client (and clearing on `entries: []` / `session/new`), not by
scraping tool text. The observer path did not need a new event kind.

### 4. Binding key

Reuse `(agentPubkey, conversationId)`:

- Session ledger: `(relay_url, agent_pubkey, thread_id)` where `thread_id` is
  the scheduler UUID (`session_ledger.rs` `SessionLedgerKey`).
- Observer `conversation_id` is that same scheduler id
  (`pool.rs` `observer_conversation_id = batch.channel_id`;
  `conversation.rs` `id_for_event` hashes channel UUID + thread-root event id).
- Desktop: `deriveAgentConversationId(channelId, rootEventId)`
  (`conversationId.ts`) — same `buzz-acp-conversation-v1` hash.
- `sessionAgingStore` keys `${normalizePubkey(agentPubkey)}:${conversationId}`.

Do not invent a second identity key. Agent owner is the Crew pubkey, not the
ACP binary (Hermes Dev vs Scout are two keys even on one Hermes adapter).

### 5. Grok / buzz-agent

**Unknown as a plan protocol.**

- Spike 0018 Grok assets advertise a `todo_write` tool in `_meta.tools` and
  contain **zero** `"sessionUpdate": "plan"` frames.
- `crates/buzz-agent` tests emit thought/message/tool/usage updates, not
  `plan`.
- buzz-dev-mcp in-process `{text, done}` todos die with the engine and are
  **not** ACP plan authority.

## Edge cases observed

- Unstructured plan `content` (markdown checklist) exists as a legacy parser
  fallback in the transcript. #190 must not treat that as declared tasks.
- `todo_write` on Grok is a tool name only in this evidence set; payload
  shape was not captured.

## Limitations

Live Hermes/Claude/Codex adapters were not installed on this VM. Runtime
labels above are in-repo / prior-spike evidence. A later live capture can
replace an "unknown" without changing the projection contract: missing
signal stays unknown.

## Verdict

**PASS.** Observer already forwards raw `plan` updates; the client did not
retain them. Binding key is `(agentPubkey, conversationId)`. Codex has
in-repo `entries[]` evidence. Claude native plan is unknown. Grok/buzz-agent
are unknown. Hermes native `entries[]` is documented in-tree, not
live-captured here.

## Follow-up test contract

- ACP client retains `entries[]` and clears on `[]` and `session/new`.
- Desktop projection: two same-ACP agents, wholesale replace, empty clear,
  unknown without guessing, identical text stays two rows, sleep keeps last
  declared, stale session is unknown, Mission Inbox gains no plan rows.

## Cleanup

None. No live adapter processes were started.
