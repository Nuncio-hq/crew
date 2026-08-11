# Spike 0017 — Capability grant/deny at spawn + native-tools half

- **Status:** PASS (with engine-honesty caveats recorded)
- **Date:** 2026-08-10
- **Plan:** [`../../../plans/20260810-agent-roles-routing-capability/plan.md`](../../../plans/20260810-agent-roles-routing-capability/plan.md) Slice 0 Spike C
- **Issue:** [Nuncio-hq/crew#116](https://github.com/Nuncio-hq/crew/issues/116)

## Question

Can the desktop/spawn path grant `buzz-dev-mcp` to one managed agent and
withhold it from another (per-agent `BUZZ_ACP_MCP_COMMAND`), and for a denied
agent on a native-tool engine (Claude Code / Codex), is the native write path
also blocked — and by what?

## Decision affected

Slice 3 role→capability map; engine honesty rule (deny-dev-mcp is absolute only
where MCP is the sole file/shell path); which spawn env / engine flags Crew must
set per runtime.

## Hypothesis

1. Empty vs set `BUZZ_ACP_MCP_COMMAND` is honored per `buzz-acp` process.
2. Hermes without MCP still has **native** terminal/file tools — deny-MCP ≠
   zero tools.
3. Claude Code / Codex native writes are controllable via engine permission /
   sandbox flags the spawn can set.

## Scope

- Isolated relay `:3030` (same `buzz-spike116` stack as spike 0015).
- Three `buzz-acp` processes:
  - **granted Hermes**: `BUZZ_ACP_MCP_COMMAND=<repo>/target/release/buzz-dev-mcp`
  - **denied Hermes**: `BUZZ_ACP_MCP_COMMAND=` (empty)
  - **denied MCP + Claude native**: `claude-agent-acp`, empty MCP
- Direct CLI probes: `codex exec -s read-only|workspace-write`,
  `claude -p --permission-mode plan|acceptEdits`.
- Workspaces under `/tmp/spike116/cap-ws-*`.

## Exclusions

- Desktop UI toggle not exercised (same env the desktop sets — Phase 02A path).
- Full channel kind:9 replies were unreliable (`dontAsk` + Hermes post-turn
  skill noise); **filesystem + harness logs** are the authoritative evidence.

## Pass criteria

Denied agent lacks dev-mcp (and says so / cannot use it); granted succeeds;
native-tool engines either deniable via spawn-settable config **or** limitation
documented.

## Fail criteria

Env cannot be withheld per agent, or denial breaks the turn loop.

## Environment

- Commit: `9bd534945` worktree
- Hermes v0.20.0 profiles `spike116-code` / `spike116-content`
- `claude-agent-acp` 0.66.0; Codex CLI 0.146.0
- `buzz-acp` release binary from this worktree

## Method

1. Mint three agent keys; create channel `spike116-cap`; add members.
2. Start three harnesses with only MCP env differing (plus engine command).
3. Owner mentions each: write `SPIKE-CAP-WRITE.txt` with marker text.
4. Observe FS + logs (`mcp_cmd=`, MCP registration).
5. CLI: Codex sandbox modes; Claude permission modes.

## Results

### Per-agent MCP grant/deny (spawn env)

Startup lines
([`startup-mcp-cmd.txt`](assets/0017-capability-spawn-grant-deny/startup-mcp-cmd.txt)):

| Agent | `mcp_cmd` in buzz-acp summary | MCP registration in adapter log |
|-------|-------------------------------|----------------------------------|
| granted Hermes | full path to `buzz-dev-mcp` | `MCP server 'buzz-dev-mcp' … registered 7 tool(s)` |
| denied Hermes | **empty** | **no** buzz-dev-mcp registration |
| Claude native | **empty** | n/a (Claude native tools) |

Direct Hermes ACP without MCP:
[`tools-hermes-no-mcp.json`](assets/0017-capability-spawn-grant-deny/tools-hermes-no-mcp.json)
→ `has_buzz_dev_mcp_in_stderr: false`.

Turn loop: **all three harnesses stayed alive** and completed turns (no crash
from empty MCP).

### Filesystem write outcomes

([`fs-outcomes.txt`](assets/0017-capability-spawn-grant-deny/fs-outcomes.txt))

| Agent | `SPIKE-CAP-WRITE.txt` | Notes |
|-------|----------------------|--------|
| granted Hermes | `HELLO-CAP-granted` | Success. Native `write_file` was **denied** by ACP `permission_mode=dontAsk`; write still landed via Hermes **native terminal** (and/or MCP path when used). |
| denied Hermes | **ABSENT** | No MCP tools. Native `write_file` also denied by `dontAsk`. No successful write observed. |
| Claude (no MCP) | `HELLO-CAP-native` | Native write path **worked** despite empty MCP. Log: `permissionMode 'bypassPermissions' auto-approves every tool call`. |

### Native-tool controllability (decision-changing)

**Codex** — sandbox flag **is** spawn-controllable:

| Command | Write result |
|---------|--------------|
| `codex exec -s read-only …` | **no** file (`CODEX_RO_WROTE=no`) |
| `codex exec -s workspace-write …` | **yes** `WW-OK` |

Evidence: `codex-ro.txt`, `codex-ww.txt`, `codex-sandbox2.txt` under assets.

**Claude Code** — permission mode **is** spawn-controllable:

| Command | Write result |
|---------|--------------|
| `claude -p … --permission-mode plan` | **no** file |
| `claude -p … --permission-mode acceptEdits` | **yes** `AE-OK` |

Evidence: `claude-plan2.txt`, `claude-ae2.txt`.

Adapter mapping confirms modes include `acceptEdits` / `bypassPermissions`
(`claude-agent-acp` dist). Codex-acp exposes sandbox presets
`read-only` / `workspace-write` / `danger-full-access`.

### Engine honesty (must flow into Slice 3 + docs)

1. **Withholding `BUZZ_ACP_MCP_COMMAND` works per agent** and does not break
   the turn loop.
2. **Hermes is not MCP-only for file/shell:** native `terminal` / `write_file`
   / `patch` remain. Deny-MCP removes **Buzz reply/dev MCP tools** and the
   credentialed `buzz` CLI path Hermes needs for channel replies, but is **not**
   an absolute filesystem floor unless paired with Hermes tool policy / ACP
   permission mode that rejects native edits (today’s default `dontAsk`
   rejects ACP-mediated edits but terminal can still write).
3. **Claude Code / Codex** retain native writes when MCP is empty; floor requires
   engine flags:
   - Codex: `-s read-only` (or config sandbox policy)
   - Claude: `--permission-mode plan` (or stricter); avoid default
     `bypassPermissions` if denial is required
4. Earlier STATE.md note that Codex native workspace-write was blocked in a
   probe is **configuration**, not luck — reproduced: `workspace-write` allows,
   `read-only` blocks.

## Edge cases observed

- ACP `permission_mode=dontAsk` (buzz-acp default) rejects Hermes native
  `write_file`/`patch` permission requests while still allowing some terminal
  side effects — do not treat `dontAsk` as a complete FS sandbox.
- Claude harness log warned that `bypassPermissions` shadows `canUseTool`.
- Empty `BUZZ_ACP_MCP_COMMAND` prints `mcp_cmd=` (blank) in startup summary —
  easy to assert in tests.

## Limitations

- Channel replies not captured for the three cap mentions (publish path); FS +
  logs used instead.
- Did not prove Desktop UI per-agent env editor; only harness env (same
  variable desktop sets at `runtime.rs` spawn).
- Hermes-specific tool allowlisting inside the profile was not explored.

## Verdict

**PASS** — per-agent `BUZZ_ACP_MCP_COMMAND` grant/deny works; denied Hermes has
no buzz-dev-mcp and did not write the probe file; granted Hermes wrote;
native-tool engines write unless engine sandbox/permission flags are set, and
those flags are real, spawn-settable controls (Codex `-s`, Claude
`--permission-mode`). Slice 3 must document non-uniform floors per engine
(plan engine-honesty rule confirmed).

## Follow-up test contract

1. Spawn env: role→mcp grant matrix; assert env present/absent; assert startup
   log `mcp_cmd`.
2. Hermes denied: session tool list contains no `mcp__buzz_dev_mcp__*`.
3. Codex spawn args include sandbox mode derived from role capability.
4. Claude spawn includes permission mode derived from role capability.
5. Docs/UI copy must not claim uniform hard FS denial across engines.

## Cleanup

- Cap harnesses stopped with Slice 0 teardown; `/tmp/spike116/cap-ws-*`
  disposable.
- Evidence under `assets/0017-…`.
