# Phase 01 — Hermes tier-1 runtime entry in Crew (supersedes the upstream-PR plan)

- **Status:** In progress
- **Target repo:** `Nuncio-hq/crew` — per **D-020** there is NO upstream
  PR to block/buzz; the previous version of this file (upstream-targeted)
  is historical.
- **Branch:** `feat/hermes-tier1-runtime` cut from Crew `main`.

## Deliverable

Crew ships the Hermes tier-1 `KnownAcpRuntime` entry directly, merged
through `NuncioCrew Gate`. Crew accepts and owns the resulting fork
delta in the touched files (re-verified on every upstream sync).

## Entry shape (mirrors the Claude model-ownership shape + Codex MCP shape)

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
  config (Claude precedent; D-019)
- `default_env: &[("HERMES_ACP_SKIP_CONFIGURED_MCP", "1")]` — mirrors
  the buzz-acp `default_agent_env` special case declaratively; both
  layers agree, so behavior is identical whichever spawns
- `auth_probe_args: None` + `login_hint: Some(...)` — no
  exit-code-truthful probe exists in Hermes v0.20.0 (spike 0010)
- `skill_dir: None` (Hermes skills are per-profile), no install
  commands (docs link only), `supports_acp_model_switching: true`
- `default_agent_args`: map `hermes` → `["acp"]` in **both**
  implementations (`discovery.rs` and `crates/buzz-acp/src/config.rs`)

Also:

- Remove the `hermes` entry from `PRESET_HARNESSES`; keep `hermes` in
  `BUILTIN_IDS`.
- Keep the logo working across the preset→tier-1 move
  (`RuntimeIcon.tsx`, `harnessCatalogCopy.ts`, preset-logo test).
- Tests: runtime identity normalization for `hermes`/`hermes-acp`/
  `hermes-agent`/path/Windows-shim forms; default-args mapping; entry
  shape.

## Quality gates

`cargo test -p buzz-acp --lib`, desktop Tauri `cargo test --lib`,
`cargo fmt` (root + Tauri), `just desktop-typecheck`, preset-logo test,
then full `just ci` before the PR.

## Fork-delta note (D-020)

This edit intentionally exceeds the old "one route + one nav entry"
upstream-file budget. The delta is confined to the runtime
catalog/args/logo files listed above; every upstream sync must diff
these files against upstream and re-verify. If upstream ever ships its
own Hermes tier-1 entry, upstream's shape wins and this delta retires.

## Steps

1. Implement on `feat/hermes-tier1-runtime` (from Crew `main`).
2. Gates green locally → PR into Crew `main`, referencing issue #51
   (no `Closes` — the issue tracks the whole feature).
3. `NuncioCrew Gate` green → merge.
4. After merge: update `docs/crew/HERMES.md` (drop the manual
   `BUZZ_ACP_MCP_COMMAND` step and the "preset probes hermes-acp only"
   gap note), and retire the per-profile tier-3 JSONs' probe role.

## Exit criteria

Entry merged into Crew `main`; desktop catalog shows Hermes available
with only `hermes` on PATH; spawn gets `buzz-dev-mcp` + env guard
automatically; runbook updated.
