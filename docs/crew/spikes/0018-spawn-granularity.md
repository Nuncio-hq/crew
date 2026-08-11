# Spike 0018 — spawn granularity: channel-session capability

## Question

Spike 0017 proved per-spawn grant/deny of `BUZZ_ACP_MCP_COMMAND`, but not what a spawn corresponds to. Can capability differ per channel-session of one agent, or only per agent process?

## Verdict definitions

- **PASS:** Slice 3 can use a channel-scoped hard floor: a channel's role assignment determines the dev-mcp grant and engine permission flag for that channel session.
- **FAIL:** the hard floor degrades to the union of assignments per agent process; per-channel discipline remains prompt-level only.
- **INCONCLUSIVE:** the required runtime execution could not be performed; no stronger runtime claim is made.

## Method

1. Inspected the shipped desktop managed-agent spawn path, ACP process/session ownership, configuration construction, channel session cache, and ACP `session/new` implementation.
2. Ran the narrow existing ACP tests covering MCP serialization and channel-origin forwarding.
3. Ran a real stdio ACP wire probe with one child process and two `session/new` requests. The probe sent a dev-mcp server for session A and an empty list for session B, and recorded the child PID and raw request/response frames.
4. A full local relay/real-engine two-channel run was not performed: the environment had no running relay/database stack or authenticated test agent identity. The wire probe is therefore protocol evidence, not a desktop+harness runtime proof.

## Code-level evidence

### Process boundary

The desktop runtime key is `(agent pubkey, relay URL)`, not channel (`desktop/src-tauri/src/managed_agents/runtime_types.rs:8-17`). The managed runtime constructs one `Command` and sets the harness environment before spawning it (`desktop/src-tauri/src/managed_agents/runtime.rs:522-547`). The environment includes `BUZZ_ACP_AGENT_ARGS` and `BUZZ_ACP_MCP_COMMAND` (`:539-547`).

The ACP client documents the boundary directly: “One `AcpClient` per agent process. Multiple sessions can be created on the same client” (`crates/buzz-acp/src/acp.rs:145-148`). `AcpClient::spawn` starts the agent subprocess once with command, args, and environment; subsequent sessions use that client (`crates/buzz-acp/src/acp.rs:462-500`).

Channel sessions are cached by channel UUID and created lazily. An existing channel reuses its session ID; a new channel calls `create_session_and_apply_model` and stores the resulting ID (`crates/buzz-acp/src/pool.rs:1770-1805`). Thus the shipped arrangement is one ACP/agent process per managed-agent runtime, with multiple ACP sessions in that process—not one process per `(agent, channel)`.

### MCP configuration

`BUZZ_ACP_MCP_COMMAND` is a harness configuration field (`crates/buzz-acp/src/config.rs:249-261`) and is copied into `Config` during startup configuration construction (`crates/buzz-acp/src/config.rs:941-945, 1089-1095`). `build_mcp_servers(&config)` converts that one configured command into the shared `PromptContext.mcp_servers` list (`crates/buzz-acp/src/lib.rs:1939-1967, 5163-5180`; `crates/buzz-acp/src/pool.rs:575-612`). It is not re-read from the environment per channel session.

There is, however, a session-scoped ACP transport seam. `session_new_full` accepts `mcp_servers: Vec<McpServer>` and places it directly in each `session/new` request as `mcpServers` (`crates/buzz-acp/src/acp.rs:638-688`). Channel creation passes a channel-specific copy to that call (`crates/buzz-acp/src/pool.rs:1001-1018`). Current code only appends channel/agent origin environment metadata in `mcp_servers_with_git_origin` (`crates/buzz-acp/src/pool.rs:1115-1165`); it does not add or remove the base dev-mcp server from role assignment.

**Code-level MCP result: PASS for transport capability, not for the current policy implementation.** One process can create channel sessions with different `mcpServers` lists, so a future role-scoped policy can make dev-mcp available in one channel session and absent in another without respawning. The shipped code currently supplies the same base list to all sessions.

### Engine arguments and permission mode

`agent_args` are normalized once and stored in `Config` (`crates/buzz-acp/src/config.rs:807-830, 941, 1093`). They are cloned into the one ACP subprocess spawn (`crates/buzz-acp/src/lib.rs:4738-4755`). Therefore Codex `-s` and Claude CLI startup flags, including `--permission-mode` when passed as engine args, are process-level in this path.

The harness also has a `PermissionMode` config and applies it using session-addressed ACP `session/set_config_option` after `session/new` (`crates/buzz-acp/src/pool.rs:1103-1110, 1242+`). But the mode is read from shared process `PromptContext`, not selected from channel assignment. The evidence establishes a possible session-addressed ACP mechanism, not that every engine supports or enforces a different mode per session.

**Code-level engine result: FAIL for startup-argument separation; unverified/conditional for ACP session config.** Codex/Claude process flags cannot differ between channel sessions of the same spawned engine process. A session-level permission mode may be possible where the ACP agent advertises it, but that is not the current role policy and was not runtime-tested here.

## Runtime evidence

### ACP wire probe

Asset: `docs/crew/spikes/assets/0018-spawn-granularity/wire-session-probe.json`

The probe used one fake ACP agent child process (PID `22888`) and sent two `session/new` requests:

- Session A request contained one `buzz-dev-mcp` server.
- Session B request contained `"mcpServers": []`.
- Both responses reported PID `22888`.

Recorded result:

```json
{
  "sameProcessForBothSessions": true,
  "sessionAHasDevMcp": true,
  "sessionBHasDevMcp": false
}
```

The raw frames show the complete `session/new` requests and responses, including the differing MCP lists. This is a faithful wire-level demonstration that ACP session configuration can differ while the agent process remains the same.

The probe does **not** execute the shipped `buzz-acp` pool against a local relay and real engine, and therefore does not prove that the current desktop policy derives different lists from two real channel role assignments.

### Existing tests

- `pool::tests::public_session_forwards_channel_origin_to_mcp` — PASS; confirms channel-session creation forwards channel-origin metadata through the MCP server definition.
- `acp::tests::session_new_mcp_server_has_required_fields` — PASS; confirms the ACP MCP server payload serializes the required fields.

These tests passed with:

```text
cargo test -p buzz-acp pool::tests::public_session_forwards_channel_origin_to_mcp -- --nocapture
cargo test -p buzz-acp acp::tests::session_new_mcp_server_has_required_fields -- --nocapture
```

Raw code excerpts are retained in `docs/crew/spikes/assets/0018-spawn-granularity/code-evidence.txt`.

## Verdict

> **Superseded in part.** The engine-flag `FAIL` and the `INCONCLUSIVE`
> runtime row below were both overturned by the authenticated real-engine run
> in “Real-engine two-session run (authoritative)”. Read that section for the
> current answer; this section records what code inspection alone showed.

- **Code-level MCP/session granularity: PASS.** One ACP process serves multiple channel sessions, and ACP `session/new` carries a distinct `mcpServers` list per session.
- **Code-level engine startup-flag granularity: FAIL.** `agent_args` and Codex/Claude CLI startup permission flags are process-level. The current shared `PermissionMode` source is not channel-scoped; ACP session config is only a conditional, untested escape hatch.
- **Runtime desktop+harness two-channel experiment: INCONCLUSIVE.** The wire probe passed, but a real relay/engine run was not available in this environment.

## Slice 3 implication

Slice 3 can make the dev-mcp hard floor channel-session scoped through per-session `mcpServers`, but native Codex/Claude permission flags cannot be independently hard-floored per channel in the current process-spawn model; they require a separately verified session-config path or an honest process-level union limitation.

## Corrected real-engine rerun

The earlier Pi and Hermes attempts used the wrong authentication paths for this
credential. This retry used the provider/runtime path identified by the shipped
catalog:

- runtime: `codex`, launched through `codex-acp`;
- CLI: `codex-cli 0.147.0`;
- ACP adapter: `@agentclientprotocol/codex-acp 1.1.14`;
- provider/auth key: `openai-codex`;
- decoded auth file: `~/.codex/auth.json`, mode `0600`.

The catalog identifies Codex's config path as `~/.codex/config.toml`, its
runtime command as `codex-acp`, and its login path as `codex login`. It does
not expose a provider environment variable for Codex because the Codex CLI
owns provider authentication. The credential blob was decoded only to the
Codex auth path; no credential material was copied into this repository or
the spike assets.

`codex-acp` initialized successfully and advertised the real Codex adapter:

```text
agentInfo.name = @agentclientprotocol/codex-acp
agentInfo.version = 1.1.14
protocolVersion = 1
authMethods = api-key, chat-gpt
```

However, both `session/new` attempts were rejected before a session was
created:

```text
Internal error: plan type is required for chatgpt authentication
```

The standalone Codex CLI independently reached the real ChatGPT backend, but
the model request returned HTTP 401. Its credential recovery log reported:

```text
auth_recovery_outcome="recovery_failed_permanent"
Turn error: Your access token could not be refreshed because you have since
logged out or signed in to another account. Please sign in again.
```

Evidence assets:

- `assets/0018-spawn-granularity/codex-acp-auth-attempt.json`
- `assets/0018-spawn-granularity/codex-cli-auth-attempt.txt`
- `assets/0018-spawn-granularity/hermes-acp-auth-attempt.txt`

Hermes was also obtainable as `Hermes Agent v0.19.0 (2026.7.20)`, but its
ACP entry point failed before initialization with:

```text
ModuleNotFoundError: No module named 'acp'
```

The Hermes failure is an adapter installation/runtime blocker, not evidence
about Codex authentication or either engine's session semantics.

No ACP session was created during that attempt, so it produced no transcript
for either context isolation or session-addressed `set_config_option`. That
attempt is retained as history; it is **superseded** by the authenticated
real-engine run below.

## Real-engine two-session run (authoritative)

The owner completed device-code login for two engines, so both questions were
answered against live engines instead of a stub:

- `codex-cli 0.147.0` with `@agentclientprotocol/codex-acp 1.1.14`
  (`codex login --device-auth`, `chat-gpt` auth method);
- `grok 1.0.0` (`3cd0d0cbce`), official xAI Grok Build CLI, ACP over
  `grok agent stdio` (`grok login --device-auth`, `cached_token` auth method).

Each run spawned **one** engine process and created **two** sessions on it. No
credential material was copied into this repository; the probe scripts and
recorded frames are credential-free.

### Q1 — context isolation across sessions on one process: **PASS, both engines**

Session A was told a code word; session B was asked, in its own session, what
code word it had been given; session A was then asked to repeat it as a
positive control.

| Engine | PID | Session A turn 1 | Session B | Session A control | Leaked? |
|--------|-----|------------------|-----------|-------------------|---------|
| Codex  | 481372 | `STORED` | `NONE` | `PLUMBUS-7742` | no |
| Grok   | 483848 | `STORED` | `NONE` | `PLUMBUS-7742` | no |

Both engines kept the two sessions' contexts separate while sharing one
process, and the control turn proves the probe could have detected a leak.

Assets: `assets/0018-spawn-granularity/real-codex-two-session-probe.json`,
`assets/0018-spawn-granularity/real-grok-two-session-probe.json`.

### Q2 — per-session native-tool floor on one process: **PASS, both engines**

This **reverses** the earlier negative finding. Codex ACP advertises a
session-scoped `mode` config option (`read-only` / `agent` /
`agent-full-access`) in its `session/new` result, and honors a
session-addressed change to it:

```json
{"method": "session/set_config_option",
 "params": {"sessionId": "<B>", "configId": "mode", "value": "read-only"}}
```

With session B switched to `read-only` and session A left at `agent`, on PID
483192, both sessions were asked to create a file with their native file
tool. Session A answered `WROTE` and the file exists; session B answered
`DENIED` and its file does not exist. The only file in the workspace
afterwards was session A's.

Grok rejects `set_config_option` (`missing field configId`) but accepts
session-addressed `session/set_mode`:

```json
{"method": "session/set_mode", "params": {"sessionId": "<B>", "modeId": "plan"}}
```

On PID 483848 with a permission-approving client, session A wrote its file and
session B refused (`DENIED`, no file). Under a permission-**denying** client
(`real-grok-per-session-floor.json`) neither session could write, because
Grok's default mode routes the write through a client permission request that
the probe rejected — the distinction there is *where* the refusal comes from,
not whether the floor differs.

The earlier `FAIL` was drawn from Crew's own spawn path — `agent_args` such as
Codex `-s` and Claude `--permission-mode` are fixed per process
(`crates/buzz-acp/src/config.rs:807-830`, `crates/buzz-acp/src/lib.rs:4738-4755`).
That remains true, but it is a limitation of **how Crew configures the engine**,
not of the engines: both tested engines expose a session-addressed permission
control that they enforce. Crew already has the seam for it —
`pool.rs:1103-1110` applies `PermissionMode` via session-addressed
`session/set_config_option` after `session/new`; the value simply comes from
shared process context rather than the channel's role assignment.

Assets: `assets/0018-spawn-granularity/real-codex-per-session-floor.json`,
`assets/0018-spawn-granularity/real-grok-per-session-floor.json`, probe
scripts `acp_two_session_probe.py` and `acp_per_session_floor_probe.py`.

### Revised verdict

- Context isolation per thread: **PASS** (Codex, Grok) — already satisfied
  today, no change needed.
- Per-session native-tool floor: **PASS** (Codex via `set_config_option`
  `mode`, Grok via `set_mode`) — a per-thread hard floor does **not** require
  one engine process per `(agent, thread)`.
- Not tested: Claude (`claude` is not installed in this environment) and
  Hermes (ACP entry point missing its `acp` module). For any engine that
  advertises no session-scoped permission control, the process-level
  limitation still stands and must be stated per engine rather than globally.

### Slice 3 implication (revised)

Slice 3's honest ceiling is higher than shipped. Role-decided dev-mcp per
channel session stays as is; on top of it, the channel's role should also
select the session's ACP permission mode, so a role-denied channel gets a
real read-only floor on Codex and Grok instead of a prompt-level rule. The
"a rule, not a wall" sentence must be narrowed to engines that expose no
session-scoped permission control, and must not be applied to Codex or Grok
on the strength of this run.

## Limitations

- No product code or existing source file was changed.
- No authenticated local relay/database/real-engine two-channel run was available, so runtime behavior of actual engines remains unverified.
- The fake-agent wire probe demonstrates ACP transport semantics only; it does not establish engine enforcement of session-level permission settings.
- No relay or engine process was started by this spike.
