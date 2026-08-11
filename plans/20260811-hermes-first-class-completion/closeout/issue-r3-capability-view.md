Title: Add a truthful read-only Hermes capability view

## Problem

No shipped surface currently provides Hermes skills counts, computer-use
availability, configured-MCP status, plugin tools, or gateway/cron/webhook
presence. PR #149 already ships #118's narrower
`{ modelSource, personaDoc, layer3 }` descriptor: the capability module defines
it (`desktop/src/shared/api/runtimeCapabilities.ts:1-14`, PR #149 / branch
`agents/hermes-profile-editing`) and the catalog mapper derives it at the
projection boundary
(`desktop/src/shared/api/fromRawAcpRuntimeCatalog.ts:44-83`, PR #149 / branch
`agents/hermes-profile-editing`). The original design record remains
(`plans/20260810-hermes-profile-editing/plan.md:69-96`, PR #123 / branch
`docs/plans-issues-117-121`); Q2 still decides ownership of the expanded view.

## What to solve

Implement the conditional R3 from
[`phase-03-capability-view.md`](../phase-03-capability-view.md):

- Run spike **S-B** against Hermes v0.20.x and require stable documented JSON
  contracts for skills, tools, computer-use, configured MCP, and
  gateway/cron/webhook presence.
- Keep the entire issue blocked on the still-open founder question **Q2**
  (capability-card ownership).
- If S-B fails or is inconclusive for any required category, **drop the issue;
  do not fake parity**. Crew must render `Unknown`, never invented zero/false
  capability facts.
- If S-B passes and Q2 resolves, add only read-only typed Rust-sourced
  counts/statuses, refresh-after-external-change, and one agreed card.
- Surface `HERMES_ACP_SKIP_CONFIGURED_MCP=1` as a sandbox diagnostic, not as
  proof of configured-server availability
  (`crates/buzz-acp/src/config.rs:735-743` on main).

## Definition of Done

- [ ] S-B has PASS/FAIL/INCONCLUSIVE criteria and a reproducible result.
- [ ] Q2 resolves ownership before RED contracts are written.
- [ ] RED contracts CV-01…CV-10 fail before implementation and pass afterward.
- [ ] Every unavailable category renders `Unknown`.
- [ ] Capability facts are parsed in Rust and exposed as typed read-only data.
- [ ] External profile changes can refresh the view.
- [ ] No second capability card is introduced alongside #118.

## Evidence required

- Two clean-run JSON captures for every claimed Hermes category, with no secrets.
- A source-to-presenter trace showing typed Rust facts and redaction.
- Tests for Unknown, refresh, MCP sandbox diagnostic, and card ownership.
- If S-B fails or is inconclusive, preserve the spike record and close/drop this
  issue rather than shipping a degraded parity claim.

## Non-goals

No editors of any kind; no profile-content copying; no raw config, memory,
skills, credentials, plugins, cron, gateway, or webhook mutation; no auth
badge; no configured-MCP enablement; no second capability card; no model/SOUL
write-through (owned by #118/#149); and no invented parity.

## Dependencies

- Spike S-B is a hard gate.
- Founder question Q2 is open and must resolve before implementation.
- #118/#149 capability-card ownership must be reconciled before this issue
  creates a surface.
