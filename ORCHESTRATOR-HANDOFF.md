# ORCHESTRATOR-HANDOFF — Issue #116 Slice 0 spikes

Branch: `feat/issue-116-agent-roles` (not pushed). Worktree-only commits with sign-off.

## Commits

1. `9bd534945` — docs(crew): spike 0015 role record projection PASS
2. `aa14c85f3` — docs(crew): spike 0016 role prompt adherence matrix PASS
3. `2540c2ac5` — docs(crew): spike 0017 capability spawn grant/deny PASS

Plan (untracked unless you add it): `plans/20260810-agent-roles-routing-capability/plan.md`

## Spike A — role record shape and projection — **PASS**

Record: `docs/crew/spikes/0015-role-record-projection.md`

Owner-signed kind `30179` carries role via namespaced
`extensions["crew:role"]="code"` (not a top-level field — `deny_unknown_fields`).
Public kind `10100` tag `["crew-role","code"]` survives isolated relay publish +
cold query. Stock `handle_agent_profile` still applies `channel_add_policy`
with the unknown tag present (SQL confirmed). Outer `30179` tags stay
`d/g/state` only. Decision-changing: live ingest currently **accepts** `30179`
even though NIP-PMA draft text says reject-until-CAS — day-one product surface
should still treat **public `10100` projection** as the safe client-visible
role; do not treat accepted `30179` as full private-aggregate authority yet.

## Spike B — role prompt adherence engine matrix — **PASS**

Record: `docs/crew/spikes/0016-role-prompt-adherence-matrix.md`

Engines: Hermes `spike116-code`, Hermes `spike116-content`, Claude Code ACP.
Method: direct ACP 10-case matrix with injected role section (live-relay full
30-mention publish path was flaky for replies; adherence boundary is model
behavior under the role section).

| Engine | n | ROLE-CHECK | off-role mutations | silent off-role risk |
|--------|---|------------|--------------------|----------------------|
| hermes-code | 10 | 8/10 | 0 | 0 |
| hermes-content | 10 | 9/10 | 0 | 0 |
| claude-code | 10 | 10/10 | 0 | 0 |

All three engines viable for soft role enforcement day one. Hermes sometimes
drops the mandatory first-line declaration on short accepts — strengthen
few-shot in Slice 1, not a FAIL. Refusals named the correct role in samples.

## Spike C — capability grant/deny + native half — **PASS**

Record: `docs/crew/spikes/0017-capability-spawn-grant-deny.md`

Per-agent `BUZZ_ACP_MCP_COMMAND` works: granted Hermes registered
`buzz-dev-mcp` and wrote the probe file; denied Hermes had empty `mcp_cmd`, no
MCP registration, no probe file; turn loops stayed healthy.

**Decision-changing for Slice 3 honesty:**

- Hermes is **not** MCP-only for FS: native terminal/write_file remain when MCP
  is withheld. Deny-MCP removes Buzz dev MCP (+ credentialed reply path) but is
  not a universal FS floor.
- Claude with empty MCP still wrote via native tools (harness used
  `bypassPermissions`).
- Native floors **are** spawn-settable and reproducible:
  - Codex: `-s read-only` blocks write; `-s workspace-write` allows (STATE.md
    earlier “blocked” note = config, not luck).
  - Claude: `--permission-mode plan` blocks; `acceptEdits` allows.

## Gate for Slice 1

All three spikes **PASS**. Orchestrator may approve Slice 1 RED contracts +
implementation planning. No production code was changed in this phase.

## Cleanup performed / remaining

- Throwaway Hermes profiles: delete with
  `hermes profile delete spike116-code -y` and
  `hermes profile delete spike116-content -y` (verify dir absence).
- Teardown isolated stack when finished reviewing:
  `tmux kill-session -t spike116-relay` (and any `spike116-*` harness sessions);
  `docker compose -p buzz-spike116 -f docker-compose.harness.yml down -v`
- Disposable tree: `/tmp/spike116/` (safe to rm -rf).
- Plan file still untracked under `plans/20260810-agent-roles-routing-capability/`
  — commit separately if the orchestrator wants it on the branch.

## Non-goals honored

No crates/desktop production edits, no RED tests yet, no DECISIONS/STATE edits,
no push, no PR.
