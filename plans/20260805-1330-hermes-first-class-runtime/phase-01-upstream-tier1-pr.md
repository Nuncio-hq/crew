# Phase 01 — Upstream tier-1 PR to block/buzz (Slice 3)

- **Status:** Not started
- **Target repo:** `block/buzz` (upstream), NOT this fork
- **Fork-side artifact:** PR body draft + exact entry text, reviewed by
  the manager before submission

## Deliverable

One upstream PR: *Promote Hermes Agent to a tier-1 ACP runtime*.

## Proposed entry (mirrors the Claude model-ownership shape + Codex MCP shape)

Add to `KNOWN_ACP_RUNTIMES` in
`desktop/src-tauri/src/managed_agents/discovery.rs`:

- `id: "hermes"`, `label: "Hermes Agent"`
- `commands: &["hermes-acp", "hermes"]` — fixes the false "not
  installed" when only the `hermes` CLI is present
- `aliases: &["hermes-agent"]`
- `mcp_command: Some("buzz-dev-mcp")` — **required**: Hermes' terminal
  sandbox strips `BUZZ_*` credentials (provider-env blocklist +
  passthrough refusal), so the harness-provided MCP server is the only
  reply path. Same pattern and rationale as the existing Codex entry.
- `model_env_var: None`, `provider_env_var: None`,
  `provider_locked: true` — model/provider are owned by Hermes' own
  config (Claude precedent)
- `default_env: &[("HERMES_ACP_SKIP_CONFIGURED_MCP", "1")]` — relocate
  the buzz-acp special case (config.rs `default_agent_env`) to the
  declarative entry; keep the buzz-acp guard until both ship, then
  retire it upstream
- `auth_probe_args: None` + `login_hint: Some(...)` — no
  exit-code-truthful probe exists in Hermes v0.20.0 (spike 0010); the
  field can be filled in a follow-up when Hermes ships one
- `skill_dir: None` (Hermes skills are per-profile, not deployable by
  Buzz), `cli_install_commands: &[]` (no auto-install; docs link only),
  `supports_acp_model_switching: true` (adapter accepts
  `session/set_model`)
- `default_agent_args`: map `hermes` → `["acp"]` in **both**
  implementations (`discovery.rs` and `crates/buzz-acp/src/config.rs`)

Also:

- Remove the `hermes` entry from `PRESET_HARNESSES`; keep `hermes` in
  `BUILTIN_IDS`.
- Keep/move the existing logo mapping (`RuntimeIcon.tsx`,
  `harnessCatalogCopy.ts`); satisfy `presetLogos.test.mjs`.
- Tests per contributor guide: runtime identity normalization for
  `hermes`, default-args mapping, catalog entry shape.

## Evidence to cite in the PR body

- Live round-trip on a real relay with `hermes -p <profile> acp`
  (fork verification 0006): mention → turn → signed reply via
  `buzz-dev-mcp`.
- The sandbox-strip failure mode without `mcp_command`
  (`auth error: BUZZ_PRIVATE_KEY is required`) — why Some("buzz-dev-mcp")
  is required, not cosmetic.
- Existing upstream Hermes accommodations (env guard for
  block/buzz#3355) showing maintained interest.
- Vendor entrypoint verification: `hermes acp` is the documented ACP
  mode (`hermes acp --help`, adapter `agentInfo` handshake output).

## Steps

1. Draft the diff on a scratch branch cut from `upstream/main` (not from
   Crew main — Crew is 147 behind and carries fork-only history).
2. `cargo test --lib` (desktop Tauri) + `just desktop-typecheck` +
   focused buzz-acp tests.
3. Manager reviews entry text + PR body → submit to block/buzz.
4. Track review; on merge, receive via
   [`UPSTREAM-SYNC.md`](../../docs/crew/UPSTREAM-SYNC.md) and then:
   - retire the per-profile tier-3 JSON *harness* files' probe role
     (they remain as arg carriers until Phase 02 replaces them),
   - drop the manual `BUZZ_ACP_MCP_COMMAND` env from the runbook
     (C-16 sync-safety contract).

## Exit criteria

PR submitted upstream with manager-approved wording; fork docs updated
with the PR link; no fork code edited in this phase.
