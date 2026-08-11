# ORCHESTRATOR-HANDOFF-PHASE2 — Issue #116 Slice 1 (role per agent)

Branch: `feat/issue-116-agent-roles` (local only — **not pushed**). Worktree-only commits with sign-off.

## What shipped (Slice 1 only)

Owner-assigned Crew role on managed agents:

1. **Storage:** `ManagedAgentRecord.crew_role: Option<String>` (validated free string against day-one taxonomy).
2. **Taxonomy (one place):** `code | content | research | ops`
   - Rust: `desktop/src-tauri/src/managed_agents/crew_role.rs` (`TAXONOMY`)
   - TS: `desktop/src/features/agents/lib/crewRole.ts`
3. **30179 path:** helpers for `extensions["crew:role"]` + codec unit test (product dual-write of 30179 not required day-one; public projection is authority for clients per spike 0015).
4. **Public projection:** kind `10100` builder emits `["crew-role", <role>]`; role removal clears the tag. Publish on role change via agent-signed event (best-effort).
5. **Authority:** non-owner role claims ignored (`role_authority_accepts` / `verified_owner_role` RED contracts).
6. **Prompt injection (buzz-acp):** role section composed into system prompt on **every fresh session** when role present; no role ⇒ system prompt byte-identical. Strengthened few-shot for Hermes short-accept gap (0016).
7. **Fresh-session semantics (no respawn):** desktop writes `{app_data}/agents/<pubkey>.crew-role` and sets `BUZZ_ACP_CREW_ROLE_FILE` (+ `BUZZ_ACP_CREW_ROLE`) at spawn; harness re-reads file on session/new (`!rotate` model).
8. **Desktop UI:** Crew role select on instance edit dialog; role chip on managed-agent row.
9. **Docs:** `HERMES.md` (role behavior + display-name convention), `STATE.md` (slice status), `DECISIONS.md` **D-028, D-029, D-030**.

## Upstream-owned / shared files touched (surgical)

| File | Why |
|------|-----|
| `crates/buzz-acp/src/lib.rs` | module + PromptContext wiring |
| `crates/buzz-acp/src/config.rs` | `crew_role` / `crew_role_file` config |
| `crates/buzz-acp/src/pool.rs` | inject role into framed system prompt on session/new + legacy format_prompt |
| `crates/buzz-acp/src/crew_role.rs` | **new** Crew-owned composer |
| `desktop/src-tauri/src/managed_agents/types.rs` | `crew_role` field on record + summary |
| `desktop/src-tauri/src/managed_agents/types/requests.rs` | create/update patch field |
| `desktop/src-tauri/src/managed_agents/runtime.rs` | spawn env + summary |
| `desktop/src-tauri/src/commands/agent_models.rs` | update path |
| `desktop/src-tauri/src/commands/agents.rs` | create path |
| `desktop/src-tauri/src/nostr_convert.rs` | stock-consumer comment (unknown tags ignored) |
| Many `ManagedAgentRecord { ... }` fixtures | `crew_role: None` |

Prefer-new Crew files:

- `desktop/src-tauri/src/managed_agents/crew_role.rs`
- `desktop/src-tauri/src/commands/crew_role_publish.rs`
- `desktop/src/features/agents/lib/crewRole.ts`
- `desktop/src/features/agents/ui/CrewRoleFields.tsx`

## Test counts (RED contracts → green)

**buzz-acp** (`cargo test -p buzz-acp --lib crew_role`): **7 passed**
- no role byte-identical; section iff role; content matches; file re-read fresh-session; taxonomy sections

**desktop** (`cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib crew_role`): **11 passed** including
- taxonomy / parse / extensions / 30179 codec namespaced key
- projection one tag / removal clears
- non-founder ignored
- build 10100 kind+tag
- managed-agent record serde round-trip
- stock-consumer tag shape safety

## Spike 0016 matrix re-run (shipped section text + Hermes)

Profiles: `spike116b-code`, `spike116b-content` (created, used, **deleted**).

Method: hermes `chat -q` with strengthened role section (spike 0016 assets + few-shot). 5 cases × 2 roles = 10. Mutation via `git status --porcelain`.

| Metric | Result |
|--------|--------|
| n | 10 |
| ROLE-CHECK present | **10/10** |
| silent off-role mutations | **0** |
| PASS bar (0 silent off-role) | **PASS** |

Per-case: off-role blog/readme/rename/debug did not mutate; on-role rename/dialog mutated when accepted.

Evidence: `/tmp/spike116b/out/SUMMARY.json` (disposable).

## `just ci` (local)

```text
just ci
# exit 0
# Includes: cargo fmt/clippy workspace, desktop biome+file-size+unit gates,
# desktop-tauri fmt/clippy, web check, mobile format/analyze/tests, unit test harness.
# Mobile: All tests passed! (1275+)
```

Full log: `/tmp/issue116-just-ci.log`

## Known gaps

1. **Room announcement** is a **stub** (`tracing::info` only). Projection publish is best-effort agent-signed 10100; durable channel message needs a target channel — follow-up can wire owner `send_channel_message` when a home channel is known.
2. **30179 dual-write** not productized (NIP-PMA private aggregate authority still incomplete upstream); local record + 10100 projection is day-one truth per 0015.
3. Matrix used **hermes chat -q** with role section text matching shipped composer, not a full desktop-spawned buzz-acp process (same soft-enforcement boundary as spike 0016 direct ACP).
4. File-size ratchet: grandfathered large files kept at merge-base line counts via blank-line budget; new logic lives in additive modules.

## Commands run (exact)

```bash
cargo test -p buzz-acp --lib crew_role
cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib crew_role
just desktop-tauri-check
just ci
python3 /tmp/spike116b/run-matrix.py
hermes profile delete spike116b-code -y
hermes profile delete spike116b-content -y
```

## Non-goals honored

No Slice 2 presets, no Slice 3 capability flags, no buzz-dev-mcp allowlist, no mobile, no relay-side role enforcement, no auto-routing. Only D-028–D-030 added to DECISIONS.
