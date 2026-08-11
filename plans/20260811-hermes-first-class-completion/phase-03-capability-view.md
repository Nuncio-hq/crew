# Phase 03 — Effective capability view (R3)

- **Status:** Proposed — not approved, not implemented
- **Issue:** #104 remainder / R3
- **Depends on:** spike S-B; founder decision Q2; #118 ownership decision
- **PR scope:** Read-only Rust facts + one Crew-owned projection/card; no editors

## Hard gate

This entire phase is conditional on S-B. If Hermes v0.20.x exposes no stable
JSON contract for skills, tools, computer-use, configured MCP, and
gateway/cron/webhook presence, **drop this phase**. Do not render invented
parity: Crew must show `Unknown`, not infer or fabricate capability facts.
The audit found no shipped capability view; only #118's planned descriptor
overlaps the surface
(`plans/20260810-hermes-profile-editing/plan.md:69-96`,
PR #123 / branch `docs/plans-issues-117-121`).

## Gate 1 — spike S-B protocol

Probe a disposable Hermes v0.20.x profile without reading secrets or copying
profile content into Crew:

| Probe | Candidate command/observation | Stable-contract requirement |
| --- | --- | --- |
| Skills | `hermes -p <profile> skills --help`, then documented JSON/list form if available | Stable names/count schema and exit semantics |
| Tools | `hermes -p <profile> tools --help`, then documented JSON/list form | Stable tool identity/status schema |
| Computer use | `hermes -p <profile> computer-use --help` or documented capability output | Explicit boolean/status, not tool-name inference |
| Configured MCP | Hermes documented profile/config inspection command | Server IDs/status with documented redaction |
| Gateway/cron/webhook | Hermes documented status/config command | Presence/status fields, no raw config dump |

Define outcomes before running:

| Result | Definition | Consequence |
| --- | --- | --- |
| PASS | Every requested category has documented, parseable, stable JSON fields across two clean runs | Proceed to RED and conditional design |
| FAIL | Any category is absent, text-only, unstable, or requires scraping private files | Drop R3; file a Hermes-side ask if useful |
| INCONCLUSIVE | Version/platform/provider differences prevent a stable conclusion | Keep R3 blocked; ask founder/upstream rather than degrade silently |

The existing MCP guard is a fact about Crew's spawn policy, not a profile
capability list: `crates/buzz-acp/src/config.rs:735-743` on main. It may be
surfaced as a diagnostic label, never as proof that configured MCP servers
exist.

## Ownership and conditional design

| Topic | Choice |
| --- | --- |
| Source | Rust parses only the stable JSON facts; TS receives typed counts/statuses |
| Privacy | Never copy SOUL, memory, credentials, raw config, plugin payloads, or gateway content into Crew |
| Unknown | Missing/unstable facts render `Unknown`; no inferred zero |
| Refresh | Re-read facts after an external profile change; no write-through |
| MCP guard | Show `configured MCP disabled by Crew sandbox` with source/path, not a configured-server count |
| Card ownership | Block on founder Q2; do not build a second card alongside #118 |

#118's plan chooses `{ modelSource, personaDoc, layer3 }` at the catalog
boundary (`plans/20260810-hermes-profile-editing/plan.md:69-96`, PR #123 /
`docs/plans-issues-117-121`). That descriptor is plan-only and belongs to the
Q2 ownership decision; R3 must not duplicate it or become a second profile card.

## RED contract table

Write tests only after S-B PASS, then run them RED before implementation:

| ID | Scenario | Expected | Forbidden |
| --- | --- | --- | --- |
| CV-01 | Stable skills JSON exists | Typed count/status appears | Rendering profile skill contents |
| CV-02 | Stable tools JSON exists | Typed count/status appears | Assuming every tool is executable |
| CV-03 | Computer-use fact exists | Explicit availability state | Inferring from a tool name |
| CV-04 | Configured MCP JSON exists | Redacted server names/statuses | Leaking URLs, tokens, or config |
| CV-05 | Gateway/cron/webhook fact exists | Presence/status only | Copying schedules, payloads, or endpoints |
| CV-06 | One category is unavailable | `Unknown` for that category | Invented zero/false parity |
| CV-07 | External profile changes | Refresh updates facts | Stale card with no refresh path |
| CV-08 | MCP guard is active | Visible sandbox diagnostic | Claiming configured MCP is available |
| CV-09 | #118 descriptor is present | One agreed card/owner | A second overlapping capability card |
| CV-10 | Profile contains sensitive fields | Values never cross boundary | Raw YAML/JSON in relay, logs, or screenshot |

## Implementation after approval

Only after S-B PASS, Q2/#118 ownership resolution, and RED tests that fail for
the missing public contract may implementation begin. Add the smallest typed
read-only Rust-to-Crew projection and one agreed surface. If any category
cannot be parsed from the stable JSON contract, keep that category `Unknown`;
do not backfill it from profile files, prose, tool names, or guessed defaults.

## Likely touch set

| Area | Likely file | Purpose |
| --- | --- | --- |
| Rust facts | `desktop/src-tauri/src/managed_agents/discovery.rs` | Project stable Hermes capability facts |
| Rust command | `desktop/src-tauri/src/commands/hermes_profiles.rs` | Read-only profile capability IPC |
| TS projection | `desktop/src/features/agents/hermesProfileReadinessPresenter.ts` | Reuse typed presenter seam only if Q2 agrees |
| Agent surface | `desktop/src/features/agents/ui/AgentHarnessField.tsx` | One agreed card, not a second card |
| Tests | `desktop/src/features/agents/hermesProfileReadinessPresenter.test.mjs` | Unknown/redaction/refresh contracts |

## Verification

```bash
. ./bin/activate-hermit
cargo test --manifest-path desktop/src-tauri/Cargo.toml
pnpm --filter buzz check
just desktop-typecheck
git diff --check
```

## Exit criteria

- [ ] S-B is PASS, with two reproducible JSON captures and no secrets.
- [ ] Q2 and #118 ownership are resolved before RED tests.
- [ ] CV-01…CV-10 are RED before implementation and GREEN afterward.
- [ ] Every unavailable fact renders `Unknown`; no invented parity ships.
- [ ] MCP guard is visible as a sandbox diagnostic only.
- [ ] No editor, profile-content copy, or second capability card is introduced.

## Out of scope

All editors; skills, memory, credential, plugin, cron, gateway, and webhook
mutation; raw `config.yaml`; auth; model/SOUL write-through (owned by #118);
remote profile custody; configured-MCP enablement; cloud sync; and any
capability claim without a stable Hermes JSON contract.
