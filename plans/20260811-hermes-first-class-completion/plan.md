# Hermes first-class operations — completion scope audit (issue #104)

- **Status:** Scope verdict — Q1/Q6 decided; Q2–Q5 remain open (no implementation)
- **Date:** 2026-08-11
- **Issue:** [#104](https://github.com/Nuncio-hq/crew/issues/104)
  (tracking epic, Phases 01–06)
- **Parent feature:** [`docs/crew/features/0001-hermes-first-class-runtime.md`](../../docs/crew/features/0001-hermes-first-class-runtime.md)
  (contracts C-01…C-17)
- **Parent plan:** [`../20260805-1330-hermes-first-class-runtime/plan.md`](../20260805-1330-hermes-first-class-runtime/plan.md)
- **Decisions:** D-019, D-020, D-023, D-024, D-025 apply. **This plan takes no
  D-number** — a plan does not record decisions; the D-019 amendment belongs
  in #118's decision record.
- **Baseline read:** `origin/main` `35af74019`, plus the open branches for
  #134 (`agents/profile-lifecycle-hardening`), #120
  (`feat/issue-116-agent-roles`), #123 (`docs/plans-issues-117-121`), #124
  (`docs/state-truth-and-gate-audit`), #128
  (`devin/1786360062-evidence-thread-log`). Nothing is merged; branch content
  was read directly, not inferred from issue or PR text.

---

## 1. Verdict in one paragraph

**#104 as written is stale and should be reduced, not planned as an epic.**
Phase 01 shipped and is merged (#106 + #107). Phase 03's and Phase 04's
mechanisms are *shipped on `main`* — the durable question/answer substrate,
owner-only answering, `Needs you` vs `Working` projection, permission
separation, and deterministic thread worktrees all exist with tests; what is
missing is **live-Hermes certification evidence**, not code. Phase 02 and
Phase 05 are **substantially in flight in #134**, which lands a named
readiness enum, agent-card surfacing, spawn preflight, and
archive/restore/permanent-delete. Only **Phase 06 (read-only capability view)
is genuinely unbuilt**, and half of its idea is already claimed by #118's
planned capability descriptor.

The honest remainder is roughly **one small implementation slice, two
verification records, and one blocked-on-upstream item** — not six phases.
Agreed disposition in §7: close #104 and replace it with three narrow issues.

---

## 2. Reconciliation — feature 0001 contracts C-01…C-17

Verdict key: **shipped** = on `main` with code and test; **shipped
(untested)** = code on `main`, no contract test found; **in-flight-by-#N**;
**remaining**; **blocked** = cannot be built until an external dependency
lands.

| # | Contract | Verdict | Evidence |
| - | -------- | ------- | -------- |
| C-01 | Catalog truth | shipped | `desktop/src-tauri/src/managed_agents/discovery.rs:310-311`; normalization tests `crates/buzz-acp/src/config.rs:1586-1606` |
| C-02 | Profile-bound spawn | shipped | `desktop/src-tauri/src/managed_agents/hermes_profile.rs:94-119`; live evidence [`verification/0006`](../../docs/crew/verification/0006-hermes-slice1-live-roundtrip.md) |
| C-03 | Missing profile | shipped | `desktop/src-tauri/src/managed_agents/readiness/hermes.rs:30-32`, test `:137-172` |
| C-04 | No model field | **obsolete (superseded by #118)** | runtime metadata `desktop/src-tauri/src/managed_agents/discovery.rs:1256-1275`; founder decided that #118's approved model write-through supersedes the original no-model-field intent, so C-04 as written is obsolete |
| C-05 | No model injection (fields) | shipped (untested) | `desktop/src-tauri/src/managed_agents/runtime.rs:781`, helper `hermes_profile.rs:82-90` |
| C-06 | No model injection (env maps) | shipped (untested) | same last-write guard `runtime.rs:781`; no per-layer assertion found |
| C-07 | Profile-side model change | shipped | `crates/buzz-acp/src/pool.rs:948-1051`; live `!rotate` evidence in verification 0006 |
| C-08 | Memory isolation | shipped (untested) | binding injects a named profile home; no isolation probe test found |
| C-09 | Skill inheritance | shipped (untested) | same; no test found |
| C-10 | Duplicate binding | shipped | `desktop/src-tauri/src/managed_agents/hermes_profile.rs:140-208`, tests from `:291` |
| C-11 | Parallelism guard | shipped (untested) | parallelism passthrough `crates/buzz-acp/src/lib.rs:1705`; the D-019 cap of 1 is documented, not enforced by a test |
| C-12 | Unauthenticated profile | **blocked** | deliberately deferred `desktop/src-tauri/src/managed_agents/readiness/hermes.rs:10-11`; spike 0010 verdict in feature `:457-464`. #134 adds an honest `AuthUnknown` state, not a probe |
| C-13 | Offboarding keep | shipped (untested) | `desktop/src-tauri/src/commands/hermes_profiles.rs:15-34`; keep is default in the offboard UI |
| C-14 | Offboarding delete | shipped | `desktop/src-tauri/src/managed_agents/hermes_profile_lifecycle.rs:258-263` (`-y` + directory-absence verification) |
| C-15 | Non-Hermes unaffected | shipped | `crates/buzz-acp/src/config.rs:735-743`, assertion `:1705-1723` |
| C-16 | Upstream sync safety | **obsolete** | D-020 moved tier-1 promotion into Crew (`docs/crew/DECISIONS.md:308-328`); there is no upstream entry to collide with |
| C-17 | Env guard | shipped | `crates/buzz-acp/src/config.rs:735-743`; child-env tests `crates/buzz-acp/src/acp.rs:3251-3277` |

**Reading:** the C-list is done. The only live entries are C-12 (blocked
upstream) and a tail of *untested* contracts (C-04/05/06/08/09/11/13). C-16 is
obsolete by D-020.

---

## 3. Reconciliation — issue #104 Phases 01–06

### Phase 01 — trusted-autonomy / local boundary → **SHIPPED (merged)**

| Acceptance criterion | Verdict | Evidence |
| -------------------- | ------- | -------- |
| Owner-only + local Hermes agent runs with full autonomy | shipped | #106; `hermes_profile.rs:60-70`, test `:354-376` |
| Backend rejects `respond-to anyone` | shipped | `hermes_profile.rs:63`, test `:360` |
| Backend rejects remote/provider deployment | shipped | `hermes_profile.rs:69`, test `:369` |
| Same profile usable in two communities | shipped | one installation-wide record owns runtime pairs per community — `desktop/src/features/agents/lib/hermesProfileBinding.ts:265` |
| UI shows where else the profile is used + shared state | shipped | `desktop/src/features/agents/ui/HermesProfileBindingFields.tsx:78`, `:106-118` |
| Same-relay duplicate binding still fails | shipped | `hermes_profile.rs:183-208` |
| No permission approval UI added | shipped | `desktop/src/features/agents/needsYouStore.ts:26-33` keeps ACP permissions off the Needs You path |

Issue #104's own comment thread already records Phase 01 merged via #106
(`a74a18fc3`) with follow-up #107. `docs/crew/HERMES.md:203-204` says the same.
**Phase 01 needs nothing.**

### Phase 02 — Hermes Doctor → **mostly in-flight-by-#134; a thin remainder**

| Acceptance criterion | Verdict | Evidence |
| -------------------- | ------- | -------- |
| Missing binary / missing profile / broken config / auth-unknown are distinct states | in-flight-by-#134 | enum `Ready`/`Missing`/`BrokenConfig`/`BinaryMissing`/`AuthUnknown` — `desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:20-39` |
| …*incompatible version*, *missing MCP*, *ACP failure* as distinct states | **remaining** | #134 probes `--version` (`hermes_profile_readiness.rs:59-80`) but has no minimum-version gate, no `hermes acp --check`, no `buzz-dev-mcp` probe |
| Never "ready" solely because the directory exists | in-flight-by-#134 | reads `config.yaml` before claiming readiness — `hermes_profile_readiness.rs:118-149` |
| Capability facts originate in Rust | in-flight-by-#134 | Rust evaluator `hermes_profile_readiness.rs:87-149`; presenter `desktop/src/features/agents/hermesProfileReadinessPresenter.ts:1-56` |
| `Check again` refreshes the full result | partial-in-#134 | readiness recomputes on status read (`docs/crew/HERMES.md:167-172`, #134) and generic settings has refresh plumbing (`desktop/src/features/settings/ui/HarnessesSettingsPanel.tsx:90-133`, #134); no Hermes Doctor refresh contract |
| Diagnostics contain no credentials / raw env | **remaining** | no redaction/allowlist contract found anywhere |
| Update discovery (`hermes update --check`), never mutating | **remaining** | not found |
| Truthful auth | **blocked** | no headless probe in Hermes v0.20.0 (spike 0010). #134's honest `AuthUnknown` is the correct interim answer |

### Phase 03 — Needs You for decisions → **SHIPPED mechanism; certification remaining**

| Acceptance criterion | Verdict | Evidence |
| -------------------- | ------- | -------- |
| All question types intact (single-select, multi-select, free text) | shipped | `crates/buzz-acp/src/elicitation.rs:1543-1617`, reconstruction `:1647-1705`, tests `:2710-2738` |
| Answering resumes the same turn | shipped | `crates/buzz-acp/src/acp.rs:1586-1647`; durable delivery test `elicitation.rs:2067-2125` |
| Decline/cancel has a terminal outcome | shipped | `acp.rs:1595-1614`, `:1628-1633`; resolution `elicitation.rs:1465-1482`, `:1202-1209` |
| Reconnect restores a pending request exactly once | partial | exactly-once orphan recovery/cancel is proven (`elicitation.rs:1106-1139`, `:1248-1270`, restart test `:2330-2408`); *restoration of a still-answerable request across a live reconnect* is not separately certified |
| A non-owner cannot answer | shipped | `elicitation.rs:1426-1457`, test `:1855-1862`; UI authority `desktop/src/features/agents/userInputAttentionProjection.test.mjs:286-333` |
| `Needs you` ≠ `Working` | shipped | `desktop/src/features/agents/agentAttention.ts:30-40`, `:188-192`, tests `agentAttention.test.mjs:145-172`; channel presence `channelAgentPresence.ts:123-155` |
| No permission request routed here | shipped | `needsYouStore.ts:26-33`, tests `needsYouStore.test.mjs:100-180` |
| **A live Hermes `clarify` round-trip** | **remaining** | every proof above is against the harness/fake-agent or the mock bridge. `docs/crew/verification/` holds records 0001–0006 only; 0006 certifies Slice 1 spawn/model, not elicitation |

Phase 03 is therefore **not an implementation phase any more** — it is one
verification record (plus, optionally, the reconnect-restore case).

### Phase 04 — Project Runner → **SHIPPED substrate; Hermes certification remaining**

| Acceptance criterion | Verdict | Evidence |
| -------------------- | ------- | -------- |
| Deterministic worktree as `session/new.cwd` | shipped (engine-generic) | `docs/crew/STATE.md:118-125`; `crates/buzz-acp/src/thread_workspace.rs` |
| Changes land in the worktree, never the source checkout | shipped (engine-generic) | `docs/crew/STATE.md:124-127` |
| Retry does not create a second branch/worktree | shipped (engine-generic) | `docs/crew/STATE.md:140-153` |
| Restart reattaches the existing clean worktree | shipped (engine-generic) | `docs/crew/STATE.md:140-147` |
| Stop/cancel releases active-turn/worktree state | shipped (engine-generic) | `docs/crew/STATE.md:223-225` |
| Workspace failure blocks the turn with actionable copy | shipped (engine-generic) | `docs/crew/STATE.md:219-222` |
| Durable result links branch/commit/PR or explicit no-code result | **remaining, and overlapping #121/#128** | #128 adds durable evidence events and an Evidence Card (`crates/buzz-cli/src/commands/evidence.rs:1-64`, `desktop/src/features/messages/ui/EvidenceCard.tsx`) but no Project-runner result projection; `crates/buzz-acp/src/base_prompt.md:32-34` is prompt guidance only |
| **Hermes-specific certification** | **remaining** | no Hermes-specific worktree test or verification record found |

### Phase 05 — profile custody → **in-flight-by-#134; two open questions**

| Acceptance criterion | Verdict | Evidence |
| -------------------- | ------- | -------- |
| Export is an explicit manager action | in-flight-by-#134 | `desktop/src-tauri/src/commands/hermes_profiles.rs:48-99` |
| Failed export does not delete profile/agent | in-flight-by-#134 | copy→verify→remove with failure-preservation tests — `hermes_profile_archive.rs:1149-1157` |
| Warns the archive holds sensitive state | partial-in-#134 | offboard copy `desktop/src/features/agents/ui/HermesProfileOffboardFields.tsx:95-120`; no explicit "may contain credentials" warning |
| Re-hire attaches the restored profile without duplicating | in-flight-by-#134 | restore + collision handling `hermes_profile_archive.rs:764-871`; refuses while the agent runs `commands/hermes_profiles.rs:81-86,147-153,203-208` (D-035) |
| Import of an *externally supplied* archive | **remaining (may be obsolete)** | #134 restores only Crew-created archives; no import command found |
| Restored profile passes readiness *before* binding | **remaining** | no readiness gate in the restore path |
| Nothing uploaded | shipped by construction | archive is local copy logic, not `hermes profile export/import` (`docs/crew/HERMES.md:150-165`, #134) |

### Phase 06 — read-only effective capability view → **GENUINELY REMAINING**

Nothing found anywhere for skills count, computer-use availability,
configured-MCP list/status, plugin tools, or gateway/cron/webhook presence.
`HERMES_ACP_SKIP_CONFIGURED_MCP=1` is applied (`crates/buzz-acp/src/config.rs:735-743`)
but never surfaced as a diagnostic fact. The only adjacent work is #118's
**planned** capability descriptor `{ modelSource, personaDoc, layer3 }`
(`plans/20260810-hermes-profile-editing/plan.md:69-96`, #123) — plan-only,
explicitly "not implemented, not approved", and covering model/persona rather
than tools.

---

## 4. The genuine remainder

| Ref | Work | Size | Depends on |
| --- | ---- | ---- | ---------- |
| **R1** | [Doctor completion](phase-01-doctor-completion.md) — version-compat gate, `hermes acp --check`, `buzz-dev-mcp` probe, explicit re-check action, diagnostics redaction contract, read-only update discovery | one small slice on top of #134 | #134 merged |
| **R2** | [Live-Hermes certification](phase-02-live-certification.md) — verification records for the elicitation round-trip (Phase 03) and a Project-thread coding task (Phase 04) against a real `hermes -p <p> acp` | two verification records, no production code | #126 (channel question card) merged |
| **R3** | [Effective capability view](phase-03-capability-view.md) — read-only skills/tools/MCP/entrypoint summary | one slice, **gated on spike S-B** | S-B verdict; #118 direction |
| **R4** | Auth truthfulness (C-12 / Phase 02) | **blocked** on the Hermes-side probe ask (feature §7.3) | external |
| **R5** | External archive import + readiness-before-bind (Phase 05 tail) | small, **may be dropped** | founder decision Q4 |

Everything else in #104 is shipped, in flight, or obsolete.

## Phase files

- [`phase-01-doctor-completion.md`](phase-01-doctor-completion.md) — R1 Hermes Doctor completion
- [`phase-02-live-certification.md`](phase-02-live-certification.md) — R2 live-Hermes verification records
- [`phase-03-capability-view.md`](phase-03-capability-view.md) — R3 conditional effective capability view

### Spike questions that must be answered before any of R1/R3 is implemented

Per D-008 no production code starts before these are conclusive.

| Spike | Question | Decides |
| ----- | -------- | ------- |
| **S-A** | Does `hermes --version` emit a stable parseable version, and does `hermes acp --check` return truthful per-profile exit codes on a broken profile? | Whether R1 can have an `incompatible` state and an ACP-dependency check at all, or degrades to "not verified" |
| **S-B** | Does Hermes v0.20.x expose any stable JSON contract listing skills, tools, computer-use availability, configured MCP servers, and gateway/cron/webhook presence for a named profile? | Whether R3 is buildable, or must stay blocked like C-12. **If S-B fails, Phase 06 is dropped, not faked** |
| **S-C** | Does `hermes update --check` exist and is it side-effect-free? | Whether update discovery ships in R1 or is cut |
| **S-D** | What reproducible prompt/skill makes a real `hermes -p <p> acp` process emit `elicitation/create`? | Whether R2's Phase 03 record can be reproducible rather than anecdotal |
| **S-E** | Does `hermes profile export/import` produce archives interchangeable with #134's Crew-owned archive format? | Whether R5 is a thin wrapper or a second format Crew must own |

### Non-goals for the remainder (explicit)

- No permission approval inbox; ACP permissions keep `allow_once` (D-024).
- No auth badge, and no inferring auth from human-readable text, until the
  Hermes probe ask lands. `AuthUnknown` stays honest.
- No editors for skills, memories, credentials, plugins, cron, gateways,
  webhooks, or raw `config.yaml`. R3 shows counts and statuses only.
- No Hermes Dashboard/Kanban clone, no cloud profile sync, no remote
  profile-bound agents (D-024).
- No new Nostr kind, no Hermes-only protocol (D-025); R2 produces documents,
  not surfaces.
- No re-litigation of the model-presentation rule — settled in favour of #118; implementation and the D-019 amendment belong to #118.
- No global lock on cross-community profile reuse.

---

## 5. Overlaps and conflicts found while auditing

1. **#104 vs #118 — model configuration.** #104's non-goals forbid "model /
   provider configuration in Crew"; #118 is founder-locked to *model
   write-through from Crew* and explicitly supersedes the presentation half of
   D-019. **Resolved in favour of #118.** The profile remains the source of
   truth and Crew stores no competing model copy; #118 owns the implementation,
   and its PR must record the D-019 amendment in `DECISIONS.md`.
2. **Phase 06 vs #118's capability descriptor.** #118 plans
   `{ modelSource, personaDoc, layer3 }`; Phase 06 wants a tools/skills/MCP
   summary on the same card. Two owners, one surface. Founder decision **Q2**.
3. **Phase 04 vs #121/#128.** "Durable result links to branch/commit/PR" is
   the evidence-thread-log's problem, not the Hermes runner's. Keeping it in
   #104 would duplicate #128.
4. **STATE.md drift, three ways.** `main` and #124 both still list Slice 2/3/4
   as Hermes "next gates" (`docs/crew/STATE.md:265-267`, #124) although those
   slices merged; #134's STATE claims readiness/archive as shipped
   (`docs/crew/STATE.md:239-276`, #134); #120's claims binding/lifecycle
   shipped (`:239-267`, #120). This plan does **not** edit STATE.md — it
   changes no shipped state, and three open PRs are already editing that file.
   The correction belongs on #124 (docs-truth owner) and is filed as
   finding F-4 below.
5. **#134 owns Phase 02 and Phase 05 without saying so.** Issue #119's text
   never references #104; a reader of #104 would plan work #134 already did.

---

## 6. Open product decisions (founder only — Q2–Q5 remain undecided)

| # | Question | Options | Trade-off |
| - | -------- | ------- | --------- |
| **Q2** | Who owns the profile card's capability summary? | (a) fold Phase 06 into #118 as one card; (b) keep R3 as its own slice after #118 lands; (c) drop Phase 06 | (a) one coherent surface, but grows an unmerged plan; (b) clean ownership, two passes over one component; (c) cheapest — the manager keeps using `hermes -p X` to see skills |
| **Q3** | Is a live-Hermes certification record (R2) required before #104 can close, or is the existing harness/mock coverage enough? | (a) require both records; (b) require only the Phase 03 elicitation record; (c) close on existing coverage | The repo's own workflow prizes real-boundary evidence; (c) is faster but leaves "works with a real Hermes" as an assumption |
| **Q4** | Phase 05 tail: import of an externally supplied archive, and readiness-check-before-bind? | (a) both; (b) readiness gate only; (c) neither — #134's restore is enough | External import is the "moved machines" story; nobody has asked for it yet, and it is a second archive format to own |
| **Q5** | If spike S-B finds no stable Hermes JSON contract for skills/tools/MCP, what happens to Phase 06? | (a) drop it; (b) ship a degraded card showing only what Crew already knows (MCP guard, bound profile, role); (c) file a Hermes-side ask and wait | (b) risks the "Crew invents parity" failure #104 explicitly forbids |

---

## 6a. Decided (founder, 2026-08-11)

| Question | Decision | Consequence |
| --- | --- | --- |
| **Q1 — model configuration** | **#118 wins.** Crew-side model write-through is approved; #104's no-model/provider-configuration non-goal and D-019's presentation clause are superseded. | D-019's substance remains: the Hermes profile owns the model and Crew stores no competing copy. #118 owns the implementation. Its PR must record the D-019 amendment in `DECISIONS.md`; this plan takes no D-number. |
| **Q6 — disposition** | **Close #104 once #134 merges, then replace it with three narrow issues.** | The three issues are R1 Doctor completion, R2 live-Hermes certification, and R3 capability view. |

---

## 7. Agreed disposition (founder-decided)

**#104 is mostly covered and will close once #134 merges, with the remainder
tracked as three narrow issues.** Concretely:

1. **Close #104** once #134 merges, with a comment mapping each phase to what
   covered it (Phase 01 → #106/#107; Phase 02/05 → #119/#134; Phase 03/04 →
   already on `main`; Phase 06 → new issue).
2. Open **three narrow issues** in its place:
   - *Hermes Doctor completion* (R1) — after spikes S-A and S-C.
   - *Live-Hermes certification records* (R2) — verification-only, no code.
   - *Effective capability view* (R3) — gated on S-B and on Q2.
3. Leave **C-12 / auth truthfulness** filed as the standing Hermes-side ask
   (feature §7.3); it is not Crew work.
4. Fix the STATE.md Hermes drift on **#124**, not here.

What must not survive is the six-phase body, which would send the next agent to
rebuild #106, #134, and `main`.

---

## 8. Red-team pass on this plan

Findings raised against the first draft of this document, and their
disposition.

| # | Finding | Disposition |
| - | ------- | ----------- |
| F-1 | The first evidence sweep marked Phase 01 and most of Phase 03 "not found" because it searched only the paths issue #104 names. Both are on `main`. | **Applied.** Re-ran the sweep across `crates/`, `desktop/`, `docs/`, `desktop/tests` and corrected §3. Recorded here because the same trap will catch the next reader of #104. |
| F-2 | "Shipped" was being inferred from `STATE.md` prose, which is itself drifted. | **Applied.** Every `shipped` verdict in §2–§3 now cites code or a test, not STATE prose; where only STATE supports a claim (Phase 04 rows) it is labelled *engine-generic* and still counted as remaining for Hermes certification. |
| F-3 | Counting engine-generic ACP behavior as Hermes coverage would hide real risk (e.g. worktrees proven with Codex/Claude, not Hermes). | **Applied.** Phase 04 rows are split into substrate (shipped) and Hermes certification (remaining) — this is R2, the main reason the remainder is not empty. |
| F-4 | The plan risks contradicting three open PRs that all edit `STATE.md`. | **Applied.** No STATE.md edit here; drift filed as §5.4 for #124. |
| F-5 | Phase 06 could be planned as buildable when Hermes may expose no contract for it. | **Applied.** R3 is gated on spike S-B, with an explicit "drop, do not fake" instruction and founder question Q5. |
| F-6 | Recommending closure of #104 while #134 is unmerged could lose Phase 02/05 scope if #134 is abandoned. | **Applied.** §7.1 makes closure conditional on #134 merging. |
| F-7 | "Reconnect restores a pending request exactly once" was initially marked shipped from the restart-recovery tests, which actually prove *cancellation* of orphans, not restoration of an answerable request. | **Applied.** Downgraded to *partial* with the distinction spelled out. |
| F-8 | Should this plan take D-028 (or the next free number) for "reduce #104"? | **Rejected.** A plan does not record decisions, and the scope call was the founder's (§6a). D-numbering is also contended right now — #120 holds D-028/029/030, #124 D-031/032, #134 D-035, #128 D-036. |
| F-9 | Proposal to fix the untested contracts (C-04/05/06/08/09/11/13) as part of the remainder. | **Rejected for this plan.** Real gap, but it is test debt on already-shipped behavior, not #104 operations scope; it belongs in its own hygiene issue so it is not used to keep the epic alive. |
| F-10 | Proposal to specify the Phase 04 "durable result links to a PR" work here. | **Rejected.** #121/#128 own the evidence surface; specifying it here would duplicate an open PR (§5.3). |
| F-11 | Estimate the remainder in weeks for the founder. | **Rejected as framed.** Sized in slices/records instead (§4); calendar time here is dominated by waiting on #134 and on the Hermes-side probe, not by build effort. |

---

## 9. Verification for this plan

Docs-only; no code, no `D-` entry, no `STATE.md` change.

```bash
. ./bin/activate-hermit
git diff --check
pnpm --filter buzz check      # unaffected: no desktop source touched
```

Evidence trail for every citation in §2–§3 was collected by reading
`origin/main` at `35af74019` and the five open branches directly.
