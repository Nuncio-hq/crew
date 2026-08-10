# Plan — Evidence on the thread log

- **Issue:** [Nuncio-hq/crew#121](https://github.com/Nuncio-hq/crew/issues/121)
- **Status:** Draft (planning only — no implementation performed)
- **Branch of record:** `docs/plans-issues-117-121`
- **Target repo:** `Nuncio-hq/crew` only. **No PR against `block/buzz`** (D-020,
  root `AGENTS.md`). See [Founder decisions required](#founder-decisions-required).
- **North star:** `docs/crew/FOUNDER-PRODUCT.md:61` — *L4 Evidence — before/after,
  verify work. Desired; ship on the thread log when prioritized — not a new platform.*
- **Phases:** 9 (`phase-01` … `phase-09`)

## Audit gate

| Field | Result |
| --- | --- |
| Classification | Feature (agent prompt + CLI + desktop UI + docs) |
| Scout findings | **Real and partially precedented** — the accept-via-reaction pattern already ships for `KIND_AGENT_RECEIPT`; the evidence path itself does not exist |
| Already implemented? | No. `crew-evidence` has 0 hits in the tree; `messages send` has no tag flag |
| Duplicate? | No open/closed Crew issue covers evidence-on-thread |
| Under-specified? | No — the issue carries problem, spec table, scope, non-goals, verification and DoD |
| **Decision** | **Proceed to plan**, with two items escalated to the founder (below) |

The issue is authoritative and was treated as **untrusted input**: its content
was used as specification, and no instruction inside it was allowed to override
repo law. One instruction inside it does conflict with repo law and is escalated
rather than executed — see D-1 below.

## Objective

An agent that reports "done" in a room attaches the artifact it already
produced — a red→green test excerpt, before/after numbers, a `git diff --stat`,
or a headless before/after screenshot — the desktop renders it as a card in the
timeline, and the founder accepts or rejects with a standard NIP-25 reaction the
agent can read back. Evidence is a **byproduct of work done properly**, never an
extra ritual, and never requires computer-use.

## Scope

1. Office-level "Evidence on completion" rule in the shared ACP base prompt.
2. `buzz messages send --evidence <kind>` emitting a validated `crew-evidence` tag.
3. Desktop evidence card rendering four kinds; text kinds legible without images.
4. Owner Accept/Reject on the card via existing kind-7 reactions.
5. `DECISIONS.md` tag-schema + known-limit entry; `STATE.md` updated in-PR.

## Non-goals (from the issue, unchanged)

- No evidence gallery, board, or aggregation surface.
- No video evidence in this slice.
- No independent re-execution or verification machinery.
- No new event kinds; no relay-code changes.
- No computer-use anywhere in the flow.

Added by this plan, consistent with the above:

- No desktop-composer or mobile evidence authoring (CLI-only emission this slice).
- No runtime enforcement of the ≤30-line token bound (probe check, not a guard).

## Buzz seams

Every element hangs off an existing Buzz seam. Verified `path:line` in this
worktree at plan time.

| # | Need | Seam | Evidence |
| --- | --- | --- | --- |
| S1 | Office-level rule reaching every engine | Shared ACP base prompt, embedded for all runtimes unless `--no-base-prompt` | `crates/buzz-acp/src/base_prompt.md:46` (`## Communication Patterns`); delivery `crates/buzz-acp/src/lib.rs:1942` `include_str!("base_prompt.md")` |
| S2 | Custom tag on an ordinary message kind | Upstream already ships a non-NIP tag on kind 9 | `crates/buzz-sdk/src/builders.rs:250` `FAILURE_NOTICE_TAG` |
| S3 | Append a tag without touching `buzz-sdk` | Post-build `EventBuilder::tags` append | `crates/buzz-cli/src/client.rs:590` `builder.tags([tag.clone()])` |
| S4 | CLI flag surface | `MessagesCmd::Send` clap variant | `crates/buzz-cli/src/lib.rs:398`; handler `crates/buzz-cli/src/commands/messages.rs:574` `cmd_send_message`, builder match `:664` |
| S5 | Image evidence transport | Existing Blossom upload → NIP-92 `imeta` | `crates/buzz-cli/src/commands/messages.rs:616-629` (`--file` → `build_imeta_tag`) |
| S6 | Tag reaching the desktop renderer | Timeline mapper already forwards raw tags | `desktop/src/features/messages/lib/formatTimelineMessages.ts:520` → `desktop/src/features/messages/types.ts:50` `tags?: string[][]` |
| S7 | Per-kind body rendering | `renderBody()` switch, Crew already owns a branch here | `desktop/src/features/messages/ui/MessageRow.tsx:368` (switch), `:400` (`KIND_AGENT_RECEIPT`), `:415` (`default:` — where kind 9 lands) |
| S8 | Accept via reaction, already shipping | Agent-receipt card sends ✅ through the normal reaction handler | `desktop/src/features/messages/ui/MessageRow.tsx:408` `onReviewed={() => handleReactionSelect("✅")}`; state read at `desktop/src/features/messages/ui/AgentReceiptMessageBody.tsx:45-50` |
| S9 | Reject that carries a reason | Same card wires "request changes" to the reply composer | `desktop/src/features/messages/ui/MessageRow.tsx:407` `onRequestChanges={onReply ? () => onReply(message) : undefined}` |
| S10 | Owner identity for gating actions | `profiles[pubkey].ownerPubkey === currentPubkey` | `desktop/src/features/messages/ui/AgentReceiptMessageBody.tsx` owner check |
| S11 | Agent reads the verdict back | **Already exists — no CLI work** | `buzz reactions get --event <id>`, `crates/buzz-cli/src/lib.rs:746-774` |
| S12 | E2E injection of a tagged message | Mock bridge accepts arbitrary tags | `desktop/src/testing/e2eBridge.ts:1153` `extraTags?: string[][]` |

**No seam is missing.** S11 means DoD checkbox 4's "agent-readable" half is
satisfied by shipped code and needs verification, not implementation.

## Thin-fork accounting

Line counts measured against `upstream/main` at plan time.

| File | Owner | Crew delta today | This slice | Justification |
| --- | --- | --- | --- | --- |
| `crates/buzz-acp/src/base_prompt.md` | upstream | **0** (147 = 147) | **+ ≤18** (one section) | **First Crew edit to this file.** Office-level behavioral rule belongs in the office-level prompt; a per-agent Layer-3 path cannot cover the floor. Self-contained Markdown section — conflict cost is minutes, comparable to the accepted route/nav budget (`docs/crew/UPSTREAM-SYNC.md:26`) |
| `crates/buzz-acp/src/lib.rs` | upstream | +829 | **+ ~8** (one prompt-assertion test) | Follows the upstream test pattern at `upstream/main:crates/buzz-acp/src/lib.rs:3944` |
| `crates/buzz-cli/src/lib.rs` | upstream | +50 | **+ ~6** (one clap arg) | Established add-a-flag playbook; already a Crew-edited file |
| `crates/buzz-cli/src/commands/messages.rs` | upstream | **0** (1375 = 1375) | **+ ~14** | **First Crew edit.** Kept minimal by putting kind validation in a Crew-owned module and appending the tag post-build (S3) |
| `crates/buzz-sdk/src/builders.rs` | upstream | +407 | **0** | Avoided via S3 |
| `crates/buzz-core/src/kind.rs` | upstream | +21 | **0** | No new kinds (issue non-goal) |
| relay / `buzz-db` / `buzz-relay` | upstream | — | **0** | No relay changes (issue non-goal) |
| `desktop/…/MessageRow.tsx` | upstream-derived | +24 (**980 / 1000**) | **≤ 8** | See R-1: only ~20 lines of ratchet headroom; D-022 forbids raising `MAX_LINES` (`desktop/scripts/check-file-sizes.mjs:8`) |
| `docs/crew/UPSTREAM-SYNC.md` | Crew | — | new section | The issue says "record the edit in UPSTREAM-SYNC.md's upstream-file-edit list" — **that list does not exist yet** (see R-7); phase 02 creates it |
| New Crew-owned desktop + Rust files | Crew | — | new | Where all real logic lives |

Total upstream-file footprint: **5 files, ~46 added lines**, of which two files
gain their first Crew delta. Everything else is additive Crew-owned code.

## Generic-ACP check (D-025)

| Element | Generic? | Reasoning |
| --- | --- | --- |
| Prompt rule | **Yes** | Lives in the shared base prompt embedded for every runtime (S1); no engine branch |
| `crew-evidence` tag | **Yes** | Emitted by `buzz messages send`, available to any engine that can run the CLI; other clients ignore an unknown tag |
| Desktop card | **Yes** | Keys off the tag value, never off the author's runtime or profile |
| Accept/Reject | **Yes** | Standard NIP-25 kind-7; readable by any engine via `buzz reactions get` |
| Image capture | **Engine-agnostic, repo-tool-dependent** | `just desktop-screenshot` needs a Crew checkout and shell access, not Hermes |

**Hermes-only behavior in this slice: none.** Nothing in the design assumes
profile memory or profile-owned model config (FOUNDER-PRODUCT rule 4).

Honest limit to state in docs: engines or surfaces that publish messages
*without* the CLI (desktop composer, mobile) cannot emit the tag this slice.

## Proposed tag schema (becomes DECISIONS.md D-028)

```text
["crew-evidence", "<kind>"]
kind ∈ { test-run | metrics | before-after-visual | diff-stat }
```

- Rides the existing message kinds; **no new event kind**.
- First occurrence wins; later duplicates ignored.
- CLI validates the value; **the renderer does not trust it** — an unrecognized
  value falls back to the ordinary message body (forward-compatible, and safe
  because tag content is agent-authored).
- `KIND_AGENT_RECEIPT` (46043) keeps its own card and **ignores** `crew-evidence`
  (see R-2).

## Phases

| # | Title | Effort | Depends on | Gate |
| --- | --- | --- | --- | --- |
| 01 | Spike — unknown `crew-evidence` tag round-trip | S | — | Gate 1 |
| 02 | Office prompt rule + thin-fork accounting | M | 01 | Gate 4 |
| 03 | RED contract tests (CLI, desktop, reactions) | M | 01 | Gate 3 |
| 04 | CLI `--evidence <kind>` | S | 03 | Gate 5 |
| 05 | Desktop evidence card (four kinds) | L | 03 | Gate 5 |
| 06 | Owner Accept/Reject via NIP-25 reactions | M | 05 | Gate 5 |
| 07 | Upstream generic half — **BLOCKED on founder decision** | S | 02 | — |
| 08 | DECISIONS.md schema + known limit, STATE.md anti-drift | S | 04, 05, 06 | Gate 6 |
| 09 | Live probes + Playwright evidence on the PR | M | 04, 05, 06, 08 | Gate 6 |

Gate names refer to `docs/crew/DEVELOPMENT-WORKFLOW.md`
(Spike → RED tests → edge cases → approved plan → smallest implementation →
GREEN → review/docs). **Phases 04-06 must not start before phase 03 is RED and
this plan is approved** (AGENT-WORKING-AGREEMENT MUST NOT #8).

## Definition of Done → phase map

| # | DoD checkbox | Phase(s) |
| --- | --- | --- |
| 1 | base_prompt.md section + UPSTREAM-SYNC.md accounting in the same PR; upstream PR for the generic half (link recorded, not blocking) | **02** (section + accounting) · **07** (upstream half — blocked, see D-1) |
| 2 | `buzz messages send --evidence <kind>` emits the validated tag | **03** (RED) · **04** (GREEN) |
| 3 | Desktop renders all four kinds; text kinds legible without images | **03** (RED) · **05** (GREEN) |
| 4 | Owner Accept/Reject end-to-end: send, persist, render, agent-readable | **03** (RED) · **06** (send/persist/render) · **09** (agent-readable via shipped `buzz reactions get`) |
| 5 | Known-limit statement and tag schema in DECISIONS.md; STATE.md updated in-PR | **08** |
| 6 | Live probes + Playwright evidence attached to the PR | **09** |

Every checkbox maps to at least one phase. No issue requirement was dropped.

## Founder decisions required

### D-1 — The upstream PR in DoD checkbox 1 conflicts with repo law

- **The issue asks (item 1, "Parallel, non-blocking"):** *"open an upstream PR to
  `block/buzz` proposing the generic half."*
- **Repo law says:** D-020 — *"no pull request will be opened against
  `block/buzz` for this feature (or, by default, for any Crew work)"*; root
  `AGENTS.md` — *"Do not propose, draft, or open pull requests against
  `block/buzz`; the upstream remote's push URL is disabled on purpose"*;
  `docs/crew/UPSTREAM-SYNC.md:17` — *"The local upstream push URL is deliberately
  disabled. Never push to `block/buzz`."*
- **This plan does not execute the upstream PR.** Phase 07 is written as
  **BLOCKED** and delivers only a Crew-owned draft of the generic, Crew-free
  section text, so the option stays open at zero cost.
- **Precedent for this exact collision:**
  `plans/20260805-1330-hermes-first-class-runtime/phase-01-upstream-tier1-pr.md`
  was retargeted to Crew with the note that the upstream-targeted version is
  historical.
- **Ask:** keep D-020 (phase 07 stays a draft artifact), or record a scoped
  exception in `DECISIONS.md` authorizing this one upstream contribution?

### D-2 — ✅ now means two things on a Crew message

`AgentReceiptMessageBody` already treats an owner ✅ as "reviewed" on kind 46043.
This plan reuses ✅ as "accept" on evidence-tagged kind 9. They never collide on
one event (different kinds, and 46043 ignores the tag — R-2), but the founder is
learning one glyph with two nearby meanings.

- **Plan's default:** reuse ✅/❌ as the issue specifies ("no new semantics").
- **Ask:** confirm, or pick a distinct pair for evidence?

## Risks

| # | Risk | Mitigation | Owner phase |
| --- | --- | --- | --- |
| R-1 | `MessageRow.tsx` is at 980/1000 lines; a naive card branch trips the ratchet and tempts someone to raise `MAX_LINES` (violating D-022) | Hard budget of ≤8 added lines; route through the Crew-owned `MessageRowDefaultBody`; if the budget cannot be met, extract Crew deltas out of `MessageRow.tsx` — never raise the limit | 05 |
| R-2 | Two competing review affordances if a receipt also carries the tag | kind 46043 keeps the receipt card and ignores `crew-evidence`; the evidence card is kind-9-only. Contract test asserts it | 03, 05 |
| R-3 | Fabricated evidence — self-report, not proof | Accepted and documented, per the issue's "Known limit"; recorded in `DECISIONS.md`, not only in the issue | 08 |
| R-4 | Prompt bloat: base_prompt is 147 lines and every turn of every agent pays it; a fat section contradicts the issue's own token-frugality principle | Hard cap ≤18 lines; compress the 7-row table to a compact list; assert the cap in the prompt test | 02 |
| R-5 | The ≤30-line token bound is unenforceable at runtime | Stated as a probe check, never as a guard. No claim of enforcement in docs | 02, 09 |
| R-6 | Playwright screenshots for four card kinds come out byte-identical | `locator.screenshot()` per card + `shasum -a 256` uniqueness gate before posting (root `AGENTS.md`) | 09 |
| R-7 | The issue assumes an UPSTREAM-SYNC.md "upstream-file-edit list" that **does not exist** | Phase 02 creates the section and seeds it with the files this slice touches | 02 |
| R-8 | Unknown tag could be stripped by the relay, silently breaking the whole design | Phase 01 spike proves round-trip before any production code | 01 |
| R-9 | Crew e2e smoke is flaky under load; a red shard could be misread as an evidence-card regression | Attribute each failure individually; never gate the merge on one scoped run | 09 |

## Rollback / abort

- **Phase 01 FAIL** (tag does not survive round-trip): stop. The wire design is
  invalid; re-plan around a receipt-style body convention instead of a tag. Do
  not start phases 02-09.
- **Post-merge revert:** the feature is additive. Reverting the CLI flag and the
  desktop branch leaves already-published evidence messages rendering as ordinary
  messages, and existing ✅/❌ reactions intact. No migration, no data loss.
- **Partial ship:** phases 02+04 (prompt + CLI) are shippable without 05/06 —
  evidence lands in the log as plain text and stays readable. 05/06 are not
  shippable without 04.

## Validate pass

Run against this plan on 2026-08-10. **Result: PASS.**

| Check | Result |
| --- | --- |
| Every DoD checkbox maps to ≥1 phase | Pass — 6/6, table above |
| No issue requirement silently dropped or contradicted | Pass — the one contradiction (upstream PR) is escalated as D-1, not dropped |
| Every phase names a verified seam | Pass — S1-S12, all with `path:line` |
| Upstream-file edits justified with expected diff size | Pass — accounting table, ~46 lines across 5 files |
| Generic-ACP check performed (D-025) | Pass — no Hermes-only behavior; limits stated |
| Workflow gate order respected | Pass — spike → RED → implementation; 04-06 depend on 03 |
| Non-goals preserved | Pass — no new kinds, no relay edits, no gallery, no computer-use, no video |
| Anti-drift (#117): shipping PR updates STATE.md | Pass — phase 08, and restated in every implementation phase |
| PR target | Pass — `Nuncio-hq/crew` only; no upstream PR executed |
| Phase files carry required frontmatter | Pass — `phase`, `title`, `status`, `priority`, `effort`, `dependencies` |
| No implementation performed during planning | Pass — files on disk are plan artifacts only |

## Red-team pass

Adversarial review of this plan on 2026-08-10. **9 findings; 8 applied, 1
escalated.**

| # | Finding | Disposition |
| --- | --- | --- |
| RT-1 | "Just add a case to `MessageRow.tsx`" ships a ratchet violation; the tempting fix is raising `MAX_LINES`, which D-022 forbids outright | **Applied** — R-1 + explicit ≤8-line budget and extraction fallback in phase 05 |
| RT-2 | The plan originally reused ✅ without noticing `AgentReceiptMessageBody` already binds ✅ to "reviewed" | **Applied** — R-2 (kind isolation + contract test) and escalated as D-2 |
| RT-3 | A 7-row table plus 3 rules plus tooling pointers is ~30 prompt lines on *every* turn for *every* agent — the feature would violate its own token-frugality principle | **Applied** — R-4, hard ≤18-line cap asserted by test in phase 02 |
| RT-4 | "Validated tag" could be read as validating that evidence *exists*; it only validates the enum | **Applied** — stated in phase 04 and in the schema section |
| RT-5 | The issue's UPSTREAM-SYNC.md "upstream-file-edit list" does not exist — a phase written against it would fail on contact | **Applied** — R-7; phase 02 creates the section |
| RT-6 | `buzz reactions get` already exists, so a naive plan would add a redundant CLI command for DoD 4 | **Applied** — S11; phase 09 verifies rather than implements |
| RT-7 | Rejection without a reason is a dead end for the agent | **Applied** — phase 06 wires ❌ to the reply composer, mirroring `MessageRow.tsx:407` |
| RT-8 | Editing `base_prompt.md` creates a permanent conflict surface in a file with zero Crew delta today | **Applied** — accounted in the thin-fork table with a resolve hint; the section is self-contained Markdown |
| RT-9 | DoD checkbox 1 cannot be fully satisfied under D-020 | **Escalated, not applied** — D-1. The plan will not silently drop the checkbox nor silently open an upstream PR |

**Blocking findings remaining: none.** D-1 and D-2 are founder decisions that
gate phase 07 and confirm phase 06's glyph choice respectively; phases 01-05 and
08-09 can proceed without them.

## Constraints on execution

- All PRs target `Nuncio-hq/crew`. Never `block/buzz` (D-020).
- Any PR that changes shipped state updates `docs/crew/STATE.md` in the same PR (#117).
- `git commit -s` on every commit (DCO); `just ci` green before PR.
- Desktop Tauri fmt must be run from the main checkout, not this worktree.
