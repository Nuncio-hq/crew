# Hermes profile lifecycle hardening — granular readiness + archive-on-offboard

- **Status:** Planned (not started) — planning-only artifact, no branch pushed
- **Date:** 2026-08-10
- **Issue:** [#119](https://github.com/Nuncio-hq/crew/issues/119) (authoritative spec)
- **Feature:** [`docs/crew/features/0001-hermes-first-class-runtime.md`](../../docs/crew/features/0001-hermes-first-class-runtime.md)
- **Builds on:** [`../20260805-1330-hermes-first-class-runtime/plan.md`](../20260805-1330-hermes-first-class-runtime/plan.md)
  (Phase 03 profile lifecycle, Phase 04 picker)
- **Decisions in force:** D-008 (spike → RED → implement), D-019, D-020
  (PRs target `Nuncio-hq/crew` only), D-022 (extract, never raise the
  file-size limit), D-024 (trusted one-manager boundary), D-025 (generic
  ACP first, Hermes-specific labelled)
- **Phases:** 7 · **Validate:** pass · **Red-team:** 9 findings, 8 applied,
  1 rejected with rationale (see § Validation and red-team)

## Goal

Close the two gaps issue #119 found in the shipped Hermes profile
lifecycle:

1. **Readiness is one question deep.** `hermes_profile_directory_exists()`
   (`desktop/src-tauri/src/managed_agents/hermes_profile_lifecycle.rs:108`)
   is the only health question Crew asks. A profile with a corrupt
   `config.yaml`, a missing model id, or a `hermes` binary that vanished
   from PATH reads as healthy until the spawn fails mid-flight — the exact
   "agent stuck and I don't know why" class the attention line
   (#105 → #114) exists to kill.
2. **Profile delete is irreversible.** `usePersonaActions.ts:297-317`
   runs `hermes profile delete -y`; months of memory, skills and
   credentials die on one misclick. No archive, no restore.

The product frame is D-024's: a Hermes profile is an **employee record**.
Offboarding a person files their record; it does not shred it. This plan
turns that sentence into behavior.

## Scope (from the issue — verbatim intent)

**A. Readiness granularity (ambient + preflight)**

- Five named states replacing the boolean: `ready` / `missing` /
  `broken-config` / `binary-missing` / `auth-unknown`.
- State visible **on the agent card in AgentsView**, glanceable, plus
  detail in the edit dialog.
- **Spawn preflight**: evaluate before a turn starts; on
  `missing` / `broken-config` / `binary-missing`, fail fast with
  actionable copy routed through the existing attention / Needs-You
  surfaces — never a silent mid-turn stall.

**B. Archive-on-offboard**

- Offboarding's "delete profile" branch becomes **archive**: pack the
  profile to a backups area, excluding caches, showing an estimated size
  before the action.
- Archive carries a manifest: profile name, timestamp, bound agent name +
  pubkey, optional free-text offboard reason.
- **Restore**: list archives with manifest info; restore unpacks to
  `~/.hermes/profiles/<name>` and offers re-bind. Collision with a live
  profile blocks with a clear message.
- **Permanent delete** exists only on an *archive*, behind
  type-the-profile-name confirmation.
- **Running-agent guard**: destructive profile actions require the
  agent's runtime pairs stopped first; the UI disables the action and
  states the reason while running.

## Non-goals (from the issue, restated as plan boundaries)

- Scheduled or automatic profile backups.
- Rename detection (indistinguishable from deletion; the orphan +
  re-bind path already covers it).
- Any auth probe implementation inside Crew.
- Changes to the create flow, occupancy checks (C-10), or the
  owner-only / local invariants — already correct, do not touch.

## Named Buzz seams

Every deliverable hangs off an existing Buzz seam. Named here so no phase
invents a parallel mechanism.

| # | Seam | Location | What hangs off it |
| - | ---- | -------- | ----------------- |
| S1 | `Requirement` enum — named blocking reasons | `desktop/src-tauri/src/managed_agents/readiness.rs:284`, hermes orphan variant `:333` | The three *blocking* readiness states extend this enum instead of a new type |
| S2 | `AgentReadiness::{Ready,NotReady{requirements}}` | `readiness.rs:340` | Unchanged shape; the evaluator keeps returning it |
| S3 | Hermes requirements evaluator | `readiness/hermes.rs` — `hermes_requirements(effective)` | Crew-owned file where config-parse + binary-probe checks land |
| S4 | Spawn setup payload | `runtime.rs:~575-630` — builds `BUZZ_ACP_SETUP_PAYLOAD` when `NotReady`, sets `spawned_setup_mode` | The preflight *already exists*; phase 03 extends its reach, does not rebuild it |
| S5 | `buzz-acp` setup-listener → kind:9 `buzz:config-nudge` fenced sentinel | `crates/buzz-acp` setup mode | Unchanged transport for the actionable message |
| S6 | `extractConfigNudge()` + TS `Requirement` mirror | `desktop/src/shared/lib/configNudge.ts:69` (`hermes_profile_directory_missing`) | New variants mirrored here, one arm each |
| S7 | Config-nudge card renderer | `desktop/src/shared/ui/config-nudge-attachment.tsx:43,139,162,421` | New repair rows attach beside `HermesProfileOrphanRepairRow.tsx` |
| S8 | Runtime status projection | `runtime_types.rs:90` (`local_setup: bool`) → `types.ts:296` (`localSetup`) → `managedAgentRuntimeStatus.ts:12` | The boolean the issue wants replaced by named states |
| S9 | Agent card + badge | `AgentsView.tsx:~218`, `ManagedAgentRow.tsx`, `AgentStatusBadge.tsx` | Where the glanceable state renders |
| S10 | Agent attention + Needs-You | `desktop/src/features/agents/agentAttention.ts`, `needsYouStore.ts` (kinds 46010/46040) | Where preflight failure becomes an owner-visible item |
| S11 | Hermes lifecycle service + result enum | `hermes_profile_lifecycle.rs` — `HermesProfileLifecycleResult` | Archive/restore/permanent-delete reuse the named-result philosophy and the guarded CLI/dir paths |
| S12 | Lifecycle IPC commands | `desktop/src-tauri/src/commands/hermes_profiles.rs:11,17,29`, registered `lib.rs:795-797` | Three new commands register alongside |
| S13 | TS invoke wrappers | `desktop/src/shared/api/hermesProfiles.ts:42,48,56` | Archive/restore/permanent-delete wrappers |
| S14 | Offboard choice UI | `HermesProfileOffboardFields.tsx` (`data-testid="hermes-profile-offboard-{keep,delete}"`) | Radio set gains the archive option; delete branch re-homes |
| S15 | Persona delete action | `usePersonaActions.ts:297-317` | The irreversible call site being replaced |
| S16 | Name validation + `default` hard-reject | `hermes_profile.rs:13,19` | Every new destructive path routes through it |
| S17 | Post-install readiness re-evaluation + bounce | `commands/agent_discovery.rs:295-330`, `:425-455` (`should_restart_after_install`) | Closest existing machinery for "re-check readiness after the fact" — the model for mid-turn re-evaluation |
| S18 | Restricted-permission backup helpers | `desktop/src-tauri/src/util.rs:86,242,275` | Reference for archive file permissions; **not** a reusable archive area (see OQ-1) |

## Design decisions

### DD-1 — `auth-unknown` is NOT a `Requirement`

The issue lists five states as a flat set. They are not flat in the
runtime. `Requirement` (S1) implies `AgentReadiness::NotReady` (S2),
which makes `runtime.rs` (S4) spawn the agent into **setup-listener
mode** instead of a working session. Modelling `auth-unknown` as a
`Requirement` would put every healthy Hermes agent into setup mode —
Hermes v0.20.0 has *no* headless auth probe (spike 0010), so
`auth-unknown` is the permanent state of every correctly configured
profile.

Readiness therefore splits into two channels:

- **Blocking** — `missing`, `broken-config`, `binary-missing` → new
  `Requirement` variants → existing nudge + setup-mode path.
- **Non-blocking advisory** — `ready`, `auth-unknown` → a separate
  advisory field on the projection, displayed but never gating spawn.

The five names in the issue survive intact at the display layer; only
their transport differs. Recorded because it is a deviation of mechanism
from the issue's implied single pipe.

### DD-2 — Carrier correction: per-agent snapshot, not `AcpRuntimeCatalogEntry`

The issue's design constraint names
`AcpRuntimeCatalogEntry` / `lib/agentConfigCore.ts` as the pipeline.
Per `desktop/src/features/agents/AGENTS.md` rule 1, the runtime catalog
holds **harness-scoped capability facts** (what a runtime *can* do) and
`agentConfigCore.ts` projects **field descriptors**. Profile readiness is
neither — it is a per-agent, per-machine, time-varying fact about one
bound profile. Putting it in the catalog would make a harness-level table
carry agent-level state, which is what rule 1 forbids.

The plan honors the constraint's *intent* — no frontend rival table, no
`runtime.id` checks in components, named reasons from Rust — by routing
readiness through the existing per-agent `Requirement` pipeline (S1→S8)
and the runtime status projection. The catalog stays untouched.
This is an explicit, justified deviation, not a dropped requirement.

### DD-3 — Preflight is an extension, not a new mechanism

S4 already evaluates readiness at spawn and diverts to setup mode. The
real gap in the issue's framing is **breakage that appears after a
healthy spawn** (profile deleted mid-turn, binary removed). Phase 03
therefore adds (a) attention/Needs-You routing for the existing
divert, and (b) re-evaluation on turn boundaries modelled on S17. It
does not build a second preflight.

### DD-4 — Running-agent guard is refuse-while-running, not stop-then-archive

The issue asserts "existing graceful stop machinery: SIGTERM → wait →
SIGKILL fan-out". **That machinery does not exist.**
`managed_agents/runtime/stop.rs:40,120,153` uses `Child::kill()`
(SIGKILL, immediate). The only `SIGTERM` in `managed_agents` is
`discovery.rs:927`, inside the *auth-probe* 10s timeout — unrelated to
agent stop.

The guard is therefore implemented as its intended contract — a
precondition that **refuses** a destructive action while any runtime
pair is alive, surfacing the reason and a Stop affordance — and never as
"archive stops the agent for you". Building graceful stop is out of
scope for #119; noted as a follow-up candidate.

### DD-5 — Archive location is unknown; it is a spike, not an assumption

The issue references "the existing backups area
(`~/Library/Application Support/NuncioCrew Backups/` pattern)". A
repo-wide search finds **no such area**. `util.rs` (S18) only writes
`.bak.*` siblings for keychain/store files. The path is aspirational.
Phase 01 resolves it; no later phase may hardcode it first. See OQ-1.

### DD-6 — Generic-ACP check (D-025)

| Mechanism | Generic or Hermes-specific |
| --------- | -------------------------- |
| `Requirement` variants + nudge transport (S1, S5, S6, S7) | **Generic** — any ACP engine emits requirements today |
| Spawn preflight + setup-mode divert (S4) | **Generic** — untouched contract |
| Attention / Needs-You routing (S10) | **Generic** — keyed on requirement presence, not engine |
| Card readiness badge (S9) | **Generic surface**, Hermes-populated for now |
| `config.yaml` parse, `hermes --version` probe, profile dir layout | **Hermes-specific — explicitly labelled.** Confined to `readiness/hermes.rs` (S3), behind the generic `Requirement` boundary |
| Archive / restore / permanent-delete of a profile directory | **Hermes-specific — explicitly labelled.** Crew-owned files only; the concept of "profile" has no generic ACP equivalent |

No component branches on `runtime.id`. A future engine that ships its own
health facts emits `Requirement`s through the same seams with zero
frontend change.

### DD-7 — RED is a gate step inside each phase, never its own PR

D-008 requires RED contract tests before implementation. A phase whose
PR contains only failing tests cannot merge (`main` stays green,
`UPSTREAM-SYNC.md` § Thin-fork rules). Each implementation phase
therefore begins by writing the failing tests, records the observed
failure output in the PR body, then implements to green in the same PR.

### DD-8 — STATE.md is updated by every shipping phase

Issue #117's anti-drift rule: any PR changing shipped state updates
`docs/crew/STATE.md` in the same PR. That is phases 02-07, not only the
docs phase. Phase 07 owns the *narrative* consolidation, not the
per-phase obligation.

## Thin-fork budget

Upstream-file edits require explicit justification (`UPSTREAM-SYNC.md`
§ Thin-fork rules). Classification verified with
`git cat-file -e upstream/main:<path>`.

**Crew-owned (additive, no budget cost):**
`hermes_profile_lifecycle.rs`, `readiness/hermes.rs`,
`hermes_profile.rs`, `commands/hermes_profiles.rs`,
`hermesProfiles.ts`, `HermesProfileOffboardFields.tsx`,
`HermesProfileOrphanRepairRow.tsx`, plus every new file this plan adds.

**Upstream files this plan edits:**

| File | Phase | Justification | Expected diff |
| ---- | ----- | ------------- | ------------- |
| `managed_agents/readiness.rs` | 02 | New `Requirement` variants must live in the enum they extend; a Crew-side parallel enum is exactly the "copied upstream implementation" class UPSTREAM-SYNC forbids. Additive variants only — no restructuring. | ~+25 lines (2 variants + match arms) |
| `managed_agents/runtime_types.rs` | 02 | One additive field on the runtime status struct carrying the named state (replaces reading `local_setup` alone). | ~+4 lines |
| `managed_agents/runtime.rs` | 03 | Attention routing hook at the existing setup-payload branch (S4). Smallest possible hook; logic lives in Crew files. | ~+15 lines |
| `shared/api/types.ts` | 02 | TS mirror of the `runtime_types.rs` field. | ~+3 lines |
| `shared/lib/configNudge.ts` | 02 | TS mirror of the new `Requirement` variants — same file already mirrors the hermes orphan variant at `:69`. | ~+18 lines |
| `shared/ui/config-nudge-attachment.tsx` | 05 | Two new render arms delegating to Crew-owned row components. | ~+20 lines |
| `features/agents/managedAgentRuntimeStatus.ts` | 05 | Replace the `!runtime.localSetup` boolean read (`:12`) with the named state. | ~+20 / -4 lines |
| `features/agents/ui/AgentStatusBadge.tsx` | 05 | Badge variant for the readiness state. | ~+12 lines |
| `features/agents/ui/ManagedAgentRow.tsx` | 05 | Render the badge on the card. | ~+8 lines |
| `features/agents/ui/usePersonaActions.ts` | 06 | The irreversible delete call site (`:297-317`) is here; it must change. Replace the branch with a call into a Crew-owned hook. | ~+15 / -20 lines |
| `commands/hermes_profiles.rs` registration in `lib.rs` | 04 | Three `invoke_handler` lines beside `:795-797`. | ~+3 lines |
| `features/agents/ui/PersonaDeleteDialog.tsx` | 06 | Mount the Crew-owned archive fields; keep the dialog shell upstream. | ~+10 lines |

Total upstream delta ≈ **+153 / -24 lines across 12 files**, every edit a
hook or a mirror, with substantive logic in Crew-owned files. No upstream
file is restyled, reorganized, or copied.

**File-size ratchet:** `desktop/scripts/check-file-sizes.mjs`
(`MAX_LINES = 1000`). `readiness.rs` is at 1734 lines but is
`src-tauri/src` — already over and grandfathered by the script's scope;
confirm in phase 02 that the additive variants do not trip a new
failure. If any touched file approaches the limit, **extract Crew
deltas into Crew-owned files (D-022)** — never raise the limit, never
add an override.

## DoD → phase mapping

Every checkbox in the issue's Definition of Done maps to at least one
phase.

| # | DoD checkbox | Phases |
| - | ------------ | ------ |
| 1 | Five named readiness states visible on agent card + edit dialog, flowing through the canonical catalog pipeline | 01 (signal validity), **02** (model + evaluator + projection), **05** (card + dialog). Carrier deviation per DD-2. |
| 2 | Spawn preflight fails fast into attention/Needs-You with actionable copy (no silent stalls) | 01 (re-evaluation trigger spike), **03** |
| 3 | Offboarding archives profiles (manifest, cache-excluded, size shown); restore + re-bind end-to-end; permanent delete only on archives behind type-name confirmation | 01 (archive mechanics + location), **04** (backend), **06** (UI) |
| 4 | Running-agent guard enforced for destructive profile actions | **04** (backend refusal, authoritative), **06** (UI disable + reason). Per DD-4. |
| 5 | Honest `auth-unknown` state documented with link to the Hermes probe ask (spike 0010) | **02** (advisory field), **05** (display), **07** (docs) |
| 6 | Contract tests + fault-injection + Playwright evidence on the PR; HERMES.md / STATE.md / DECISIONS.md updated in-PR | RED gate in **02-06** (DD-7); **07** consolidates fault-injection, Playwright, and docs. STATE.md per-PR in 02-07 (DD-8). |

## Phases

| # | Phase | Effort | Depends on |
| - | ----- | ------ | ---------- |
| 01 | [Spike 0015 — readiness signals and archive mechanics](phase-01-spike-readiness-and-archive.md) | S (0.5-1 d) | — |
| 02 | [Readiness model, evaluator, and projection](phase-02-readiness-model-backend.md) | M (2-3 d) | 01 |
| 03 | [Spawn preflight and attention routing](phase-03-preflight-attention-routing.md) | M (2 d) | 02 |
| 04 | [Archive, restore, permanent delete, running-agent guard](phase-04-archive-restore-backend.md) | L (3-4 d) | 01 |
| 05 | [Readiness surfacing — card and dialog](phase-05-readiness-surfacing-ui.md) | M (2 d) | 02 |
| 06 | [Offboarding archive, restore, and permanent-delete UI](phase-06-offboarding-archive-ui.md) | L (3 d) | 04, 05 |
| 07 | [Verification, fault injection, and docs](phase-07-verification-and-docs.md) | M (2 d) | 03, 06 |

Phases 03/05 and 04 are independent after 01/02 and may run in parallel
if file ownership is respected (03 owns `runtime.rs`; 05 owns the UI
files; 04 owns the Rust command layer).

Delivery order note: 02 → 05 ships the readiness half end-to-end before
04 → 06 ships the archive half; each half is independently shippable and
each PR leaves `main` green.

## Open questions

| ID | Question | Blocks | Default if unanswered |
| -- | -------- | ------ | --------------------- |
| OQ-1 | Where do archives live? No "NuncioCrew Backups" area exists (DD-5). Under the app data dir, or a user-visible `~/Documents`-adjacent path? | 04, 06 | Phase 01 proposes app-data-dir with restricted permissions per S18 and records the choice; not decided by fiat here |
| OQ-2 | Archive format: `tar.gz` (new dependency) vs plain directory copy (no dependency, larger, simpler restore)? | 04 | Phase 01 measures a real profile; default to whichever meets "archives stay small" without adding a crate |
| OQ-3 | Which `config.yaml` fields make a profile `broken-config`? Unparsable YAML is unambiguous; "missing required model fields" needs the exact key set Hermes v0.20.0 requires. | 02 | Phase 01 reads a healthy profile and enumerates; conservative default is unparsable-only, so a false `broken-config` never blocks a working agent |
| OQ-4 | Does the mid-turn re-evaluation trigger on turn boundaries, on a filesystem watch, or on a timer? | 03 | Phase 01 evaluates cost; turn-boundary is the cheap default |
| OQ-5 | Is archive semantics a DECISIONS.md entry? The issue recommends yes, brief. | 07 | Yes — draft **D-028** in phase 07 (D-027 is the highest existing entry) |

None of these block starting phase 01; all are resolved by the end of
phase 01 except OQ-5.

## Risks

| Risk | Mitigation |
| ---- | ---------- |
| `auth-unknown` displayed as a warning trains the owner to ignore badges | Display as neutral/informational, never as an error; copy links the spike-0010 ask so it reads as a known limit, not a fault |
| Binary probe (`hermes --version`) on every readiness read costs latency | Cache with a short TTL and invalidate on install events (S17 already re-evaluates post-install) |
| Config parse produces false `broken-config`, blocking a working agent | OQ-3 conservative default; the state must be recoverable from the nudge without CLI |
| Archive silently excludes something the owner needed | Manifest records the exclusion list applied; phase 01 fixes the list from a real profile, not a guess |
| Restore clobbers a live profile | Collision check refuses; name validation + `default` reject via S16 on every path |
| Upstream sync conflicts on the 12 touched files | Every edit is a hook or mirror; conflict policy in UPSTREAM-SYNC.md § Conflict policy applies — reapply the smallest hook |

## Validation and red-team

### Validate — **pass**

| Check | Result |
| ----- | ------ |
| Every DoD checkbox maps to ≥1 phase | Pass — 6/6, table above |
| Every phase has frontmatter (`phase`, `title`, `status`, `priority`, `effort`, `dependencies`) | Pass |
| Dependencies acyclic and satisfiable | Pass — 01 → {02, 04}; 02 → {03, 05}; {04, 05} → 06; {03, 06} → 07 |
| Every feature names an existing Buzz seam | Pass — S1-S18, all with `path:line` |
| Upstream edits justified with expected diff size | Pass — 12 files, § Thin-fork budget |
| D-020 honored (PRs to `Nuncio-hq/crew`) | Pass — stated in every phase's PR step |
| D-025 generic-ACP check performed | Pass — DD-6 table, Hermes-specific parts labelled |
| D-008 spike → RED → implement | Pass — phase 01 is the spike; RED gate in 02-06 per DD-7 |
| Non-goals not silently violated | Pass — no auth probe, no scheduled backups, no rename detection, no create/occupancy/owner-only changes |
| Issue requirements dropped | None. One mechanism deviation (DD-2) recorded with rationale, not dropped |
| Plan contains no implementation | Pass — planning artifacts only |

### Red-team — 9 findings, 8 applied, 1 rejected

| # | Finding | Disposition |
| - | ------- | ----------- |
| R1 | Modelling `auth-unknown` as a `Requirement` would put every healthy Hermes agent into setup-listener mode — a total outage of the Hermes runtime | **Applied** → DD-1 two-channel split |
| R2 | The issue's stated carrier (`AcpRuntimeCatalogEntry` / `agentConfigCore.ts`) contradicts `features/agents/AGENTS.md` rule 1; following it literally builds the rival table the same constraint forbids | **Applied** → DD-2 deviation with rationale and preserved intent |
| R3 | The issue asserts graceful stop machinery (SIGTERM → wait → SIGKILL) that does not exist; a plan assuming it would ship a guard that cannot honor its own copy | **Applied** → DD-4, verified at `runtime/stop.rs:40,120,153` and `discovery.rs:927` |
| R4 | The "existing backups area" does not exist anywhere in the repo; hardcoding the path would create a phantom dependency | **Applied** → DD-5 + OQ-1, resolved in phase 01 |
| R5 | A RED-tests-only phase cannot merge without breaking `main` green | **Applied** → DD-7, RED as in-phase gate |
| R6 | Treating STATE.md as phase 07's job violates #117 for phases 02-06 | **Applied** → DD-8, per-PR obligation |
| R7 | A too-eager `broken-config` heuristic is worse than the boolean it replaces — it blocks working agents on a guess | **Applied** → OQ-3 conservative default + recoverable-from-nudge requirement |
| R8 | Archive without a running-agent guard on the *backend* lets a Playwright-invisible path corrupt a live profile | **Applied** → guard is authoritative in phase 04 (backend), UI disable in 06 is advisory only |
| R9 | Phase 04 archive work should block on phase 02 so both halves share one readiness read | **Rejected.** Archive operates on directories and runtime-pair liveness, not on readiness state; coupling them serializes two independent halves for no shared contract and delays the first shippable slice. Recorded rather than applied. |

### Deviations from the issue (explicit, none silent)

1. **DD-2** — readiness rides the per-agent snapshot + `Requirement`
   pipeline rather than `AcpRuntimeCatalogEntry` / `agentConfigCore.ts`.
   Intent preserved; mechanism corrected against
   `features/agents/AGENTS.md` rule 1.
2. **DD-1** — `auth-unknown` is advisory, not a `Requirement`. All five
   names still reach the display layer.
3. **DD-4** — the running-agent guard refuses while running rather than
   stopping the agent for the owner, because the graceful-stop machinery
   the issue cites does not exist.

## Constraints in force for every phase

- **D-020:** every PR targets `Nuncio-hq/crew`. Never `block/buzz`, even
  for upstream-owned files. Branch from Crew `main`, merge through
  `NuncioCrew Gate`.
- Branch names describe product areas, not phase numbers
  (`UPSTREAM-SYNC.md` § Feature branches) — e.g.
  `agents/profile-readiness`, `agents/profile-archive`.
- `just ci` green before every PR; `git commit -s` on every commit (DCO).
- No `unsafe`; no new `unwrap()`/`expect()` in production paths.
- Desktop text sizing: rem tokens only, never px (`AGENTS.md`).
- Screenshots via `scripts/post-screenshots.sh`; distinct-state
  `shasum -a 256` gate before posting.
