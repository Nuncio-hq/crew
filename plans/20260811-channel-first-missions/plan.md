# Plan — Channel-first missions: promote a thread into durable agent work in place

- **Issue:** [Nuncio-hq/crew#102](https://github.com/Nuncio-hq/crew/issues/102)
- **Status:** Draft — **planning only, nothing implemented**
- **Branch of record:** `devin/1786410238-plan-102-channel-first-missions`
- **Code baseline:** `origin/main` @ `35af74019`
- **Shipped-state baseline:** `STATE.md` on `origin/docs/state-truth-and-gate-audit`
  (PR #124), **not** `main`'s copy
- **Decision numbers taken by this PR:** **none.** This plan escalates decisions
  instead of recording them (see [Founder decisions required](#founder-decisions-required)).
- **Slices:** 5 (`slice-00` … `slice-04`)

## Audit gate

| Field | Result |
| --- | --- |
| Classification | Epic — product model + desktop UI + (possibly) one tag schema |
| Scout findings | **Real, heavily precedented, and smaller than the issue thinks.** Every state the issue wants except *promotion intent itself* already exists as a durable relay event or a documented projection |
| Already implemented? | No. There is **no mission aggregate** anywhere in the tree. `projectThreadMissionControl.ts` is a name collision, not a mission record — it is a set of pure selectors over in-memory stores |
| Duplicate? | No. #116/#121/#119/#109/#125/#110 are adjacent seams, not this |
| Under-specified? | **In one load-bearing place, yes** — the issue assumes promotion can be additive-durable without saying what carries it, and assumes a promoted ordinary-channel thread can get a worktree. Both are escalated, not guessed |
| **Decision** | **Proceed to plan.** Three founder decisions gate slice 1; slice 0 is runnable today |

The issue was treated as specification, not law. Two of its instructions
collide with repo law or with code reality; both are escalated below rather
than silently executed or silently dropped.

## Objective, in founder language

Oscar talks in a channel like normal. Some of those conversations turn out to
be *real work*: they need an isolated checkout, they pause for his answer, they
produce something he must accept, and they must still be there tomorrow.

Today that work exists, but the *fact that this thread is work* lives only in
Oscar's head and in a few in-memory caches that expire after 30 minutes to 4
hours. A Mission is the smallest durable way to say **"this thread is work"** —
attached to the thread, inside the channel, with no second conversation, no
board, and no task database.

## The one honest finding that shapes everything

> Everything a Mission displays already exists durably, **except the fact that
> it is a Mission.**

Evidence, `path:line`, on the baseline:

| Mission fact the issue asks for | Already durable? | Where |
| --- | --- | --- |
| Thread identity (root) | **Yes** — NIP-10 markered `e` tags | `crates/buzz-sdk/src/builders.rs:179-205`; parse `crates/buzz-acp/src/queue.rs:1082-1130`; desktop `desktop/src/features/messages/lib/threading.ts:1-14` |
| Channel scope | **Yes** — `h` tag | `crates/buzz-sdk/src/builders.rs:218-239` |
| Assignment | **Yes** — `p` mention (+ roles, #116) | `desktop/src/shared/lib/resolveMentionNames.ts:1-8`; `desktop/src/features/notifications/lib/shouldNotify.ts:14-16,85-86` |
| Human decision pending / answered / resolved | **Yes** — kinds `46040/46041/46042` | publish `crates/buzz-acp/src/elicitation.rs:853-956,1027-1046`; replay `:673-710`; desktop parse `desktop/src/features/channels/lib/userInput.ts:150-322` |
| Terminal result | **Yes** — kind `46043` receipt | publish `crates/buzz-acp/src/pool.rs:5544-5589`; consume `desktop/src/features/agents/agentReceiptStore.ts:182-244`; render `desktop/src/features/messages/ui/AgentReceiptMessageBody.tsx:13-65` |
| Owner acceptance / rejection | **Yes** — NIP-25 kind `7` (shipping for receipts, extended by #121/PR #128) | `desktop/src/features/messages/ui/MessageRow.tsx:407-408` |
| Project / repository context | **Yes** — kinds `30617` / `30621` | `crates/buzz-core/src/kind.rs:621-649` |
| PR / CI outcome | **Yes** — kinds `1630-1633` | `crates/buzz-core/src/kind.rs:632-640` |
| Agent readiness | **Yes** (in flight, #119/PR #134) | `desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:20-39` |
| **"This thread is a Mission"** | **NO — nothing carries it** | — |
| **"This Mission is cancelled / reopened"** | **NO — nothing carries it** | — |
| **Mission goal / title distinct from the root message** | **NO** | — |

And the things a naive implementation would lean on are **not** durable and
will silently lie:

| Tempting source | Why it must not be Mission authority |
| --- | --- |
| `conversationOutcomeLedger.ts:11-51` | module-level `Map`, **4-hour TTL**, 512-entry LRU. A Mission marked `completed` from it un-completes itself after lunch |
| `needsYouStore.ts:12,35-50` | in-memory Maps/Sets, **30-minute TTL** |
| `activeAgentTurnsStore.ts:83-106` | module-level Maps fed by ephemeral observer frames (kind `24200`) |
| `projectThreadWorkspaceStore.ts:5-57,100-183` | bounded in-memory projection, cap 256 |
| `channelAgentPresence.ts:97-177` | derived from the two above |

So the correct architecture is not "add a mission store". It is:

```text
durable relay events (already exist)  +  ONE new durable promotion marker
        └────────────────────► pure Mission projection ──► channel UI
                                        (no writes, no TTL, no authority)
```

## 1. What "promote in place" must mean on existing contracts

### 1.1 The projection (no new state)

`missionState(threadRoot)` is a **pure function** of durable events, with this
priority order — highest wins, matching the shipped precedent in
`channelAgentPresence.ts:123-177` (`needs-you` > `working` > `done-recent`):

| Derived state | Durable evidence |
| --- | --- |
| `needs_input` | an unresolved `46040` for the root (no matching `46042`) |
| `working` | live active-turn telemetry for the root's `conversationId` — **display-only, never persisted as truth** |
| `ready_for_review` | a `46043` receipt on the root with no owner kind-`7` verdict yet |
| `completed` | owner ✅ kind-`7` on the newest receipt (the shipped accept path, extended by #121) |
| `failed` | receipt carrying a failure notice, or `1632`/failure Git status |
| `planned` | promoted, none of the above |
| `cancelled` / `reopened` | see 1.2 |

Nothing here needs a new event. `ready_for_review`, `completed` and
`needs_input` **must** derive from `46043` + kind `7` + `46040/46042` and never
from the outcome ledger — that is the difference between a Mission that
survives a restart and one that quietly forgets.

### 1.2 The gap: promotion, cancel, reopen

Three transitions are **human intent**, and intent is not inferable:

- promote (with a goal/title),
- cancel,
- reopen.

The issue forbids silent auto-promotion ("Promotion must be explicit in the
first release"), so promotion cannot be inferred from "a worktree appeared".
This is the one place where new durable state is genuinely unavoidable, and it
is escalated as **[D-1](#d-1--what-carries-promotion-intent)** rather than
assumed.

**This plan's recommendation (pending D-1):** a **tagged ordinary message**, not
a new kind.

```text
kind 9 (KIND_STREAM_MESSAGE), posted into the thread with the normal
h + NIP-10 e tags, authored by the community owner, carrying:

  ["crew-mission", "promote" | "cancel" | "reopen"]
  ["crew-mission-goal", "<short title>"]     # only on "promote"
```

Why this shape:

- **Precedent, already merged-adjacent:** #121/PR #128 does exactly this with
  `["crew-evidence", "<kind>"]` on kind 9
  (`crates/buzz-cli/src/commands/evidence.rs:5-34`,
  `desktop/src/features/messages/lib/evidenceTag.ts:1-19`), and upstream itself
  ships a non-NIP tag on kind 9 (`FAILURE_NOTICE_TAG`,
  `crates/buzz-sdk/src/builders.rs:250`).
- **No new event kind, no relay change, no upstream Rust edit** — satisfies
  D-025 (build on Buzz contracts) and keeps the thin fork thin.
- **Other clients degrade to a readable sentence.** Mobile, upstream Buzz, and
  any non-Crew client show "Promoted to Mission: notification preferences" as
  an ordinary message. Nothing breaks, nothing is invisible.
- **Free authorization and ordering.** The event is signed; the projection
  applies the same strict validation the receipt store already applies
  (`agentReceiptStore.ts:107-175`: verify `h`, `e` ancestry, author) — first
  valid owner-authored `promote` wins, later duplicates ignored. That is
  promotion idempotency for free, with no client-side dedupe table.
- **It is not board state.** It carries no lane, column, order, priority,
  assignee or due date — see the D-037 boundary in §3.

### 1.3 What Mission does *not* own

Deliberately absent from the marker, because each already has an owner:

| Not in the Mission | Owner |
| --- | --- |
| who does the work | `p` mention + roles/routing (#116) |
| the repository / worktree | Project `30617`/`30621` + thread workspace metadata |
| the question and the answer | `46040/46041/46042` |
| the result | `46043` |
| accept / reject | NIP-25 kind `7` (#121) |
| whether the agent can work at all | readiness (#119) |

A Mission is a **noun-level fact plus a lens**. If a field can be derived, it
must be derived.

## 2. Slices

`spike → RED contract → implement → verify` per slice, per
`docs/crew/DEVELOPMENT-WORKFLOW.md` gates 1-7.

| # | Slice | Founder-visible outcome | Gated on |
| --- | --- | --- | --- |
| 00 | Spike — reality check | (none; produces PASS/FAIL evidence) | nothing — **runnable today** |
| 01 | **Minimum shippable:** promote in place, strip survives restart | "I marked this thread as work and it is still marked tomorrow" | D-1, D-2, slice 00 |
| 02 | Live state + inline decision | "The strip tells me it's running, and when it needs me I answer without leaving the channel" | 01 |
| 03 | Review in the channel | "It's done, the receipt is here, I accept it, and it stays accepted" | 02, #121 landed |
| 04 | Cancel / reopen / retry recovery | "I can stop this and pick it up later" | 03 |

Phase files: [`slice-00`](slice-00-spike.md), [`slice-01`](slice-01-promote-in-place.md),
[`slice-02`](slice-02-live-state-and-decision.md), [`slice-03`](slice-03-review-in-channel.md),
[`slice-04`](slice-04-cancel-reopen-recovery.md).

### Slice 00 — spike questions, up front

These four questions can change the architecture. **None may be assumed.**

| # | Question | Pass criterion | If it fails |
| --- | --- | --- | --- |
| **Q1** | Does an unknown `crew-mission` tag on kind 9 survive publish → relay → cold reconnect replay with tags intact? | tag round-trips byte-identical after reconnect | the tag design is dead; re-plan onto a receipt-style body convention. #121's phase-01 spike covers the same risk for `crew-evidence` — **reuse its result if it has run, do not re-run blindly** |
| **Q2** | Can a promoted thread in an **ordinary** (non-Project) channel obtain an isolated worktree today? | worktree provisioned | **Expected FAIL.** `crates/buzz-acp/src/pool.rs:3019-3078` + `thread_workspace.rs:244-270` require trusted `buzz://project-workspace?` metadata with an absolute repo path, owner-authored, fail-closed (`thread_workspace.rs:141-148` "not the Project owner"). Feeds **[D-3](#d-3--where-can-a-thread-be-promoted)** |
| **Q3** | Is a receipt actually published in the founder's real configuration? | a `46043` appears on the relay for a completed turn | **Expected FAIL by default:** `agent_receipts_enabled: false` (`crates/buzz-acp/src/config.rs:1495`). If receipts are off, `ready_for_review` is unreachable and slice 03 is blocked on a config change, not on UI work |
| **Q4** | Does the founder's owner pubkey reliably identify "the owner" for a promotion in an ordinary channel, the same way `AgentReceiptMessageBody` gates review? | owner check resolves in a non-Project channel | authorization rule for D-1 must change (e.g. thread-root author instead of community owner) |

Q2 and Q3 are the ones that most plausibly turn this epic from "UI work" into
"config + scope work". They are cheap and must run first.

### Slice 01 — the minimum a founder can feel

**Ship:** a `Promote to Mission` action in the existing thread action menu; an
owner-signed marker message; a compact one-line Mission strip on the thread
root; the pure projection with fixtures.

**Do not ship:** the suggestion card, worktree changes, execution changes,
Inbox/Projects changes, cancel/reopen.

**Gate (the whole point of the slice):** promote → quit the app → relaunch →
open the channel → **the strip is still there, reconstructed from the relay
alone**, with no local cache primed. If that cannot be demonstrated, the state
model is wrong and slices 02-04 must not start.

RED contracts first (see `slice-01`): projection fixtures for every allowed
state, duplicate-promote idempotency, non-owner marker rejected, unknown tag
value ignored, out-of-order arrival, `cancel` before `promote` ignored.

### Slice 02 — live state and the decision, in place

Strip shows `working` / `needs_input`; the existing durable user-input request
renders inline in the thread; answering inline resumes the same ACP session.
The projection consumes `46040/46042` for `needs_input`; active turns are
allowed **only** as a display hint for `working`, explicitly documented as
non-durable.

**Gate:** an agent turn pauses for an answer, the app restarts, the question is
still there (it is a relay event), the answer resumes the same session.

### Slice 03 — review in the channel

`ready_for_review` derives from a durable `46043`. Accept reuses the **existing**
owner ✅ reaction path — no new accept action, no second review affordance.
Receipt history is preserved; a later receipt supersedes without deleting.

**Gate:** receipt → restart → strip says `ready_for_review`; accept → restart →
strip says `completed`.

### Slice 04 — cancel, reopen, recovery

`cancel` / `reopen` markers; explicit stop; retry after failure; duplicate and
out-of-order marker handling; reconnect.

### Explicit non-goals

Carried from the issue, plus three added by this plan:

- No Mission-first home, board, Kanban, or any column/lane surface (**D-037(1)(2)**).
- No work-overview / cross-channel aggregation surface — that is D-037(3)'s
  future track and is **not** this epic (added).
- No `Since you left` catch-up (issue phase 4 → separate issue).
- No new event kind, no relay change, no `buzz-db` change (added; hard).
- No auto-promotion, no promotion heuristics, no suggestion card in slice 01.
- No parallel task database, no React-owned authoritative mission store.
- No rewrite of Inbox or Projects IA; they consume, they do not own.
- No priority, ordering, due date, or assignee field on a Mission (added; this
  is the D-037 tripwire — see §3).
- No mobile scope, no multi-agent concurrent worktree mutation.

## 3. Composition — and what it must not duplicate

### D-037 (#122 / PR #140) — the constraint this plan works hardest to respect

D-037(2) defers **board event kind and tag schema**. This plan proposes a *tag*.
That tension is real and is stated rather than hidden.

The boundary this plan holds: **D-037(2) defers board schema — lanes, columns,
slot caps, ordering, card position. The Mission marker carries none of those.**
It carries one enum and one title, scoped to a thread root, and it renders only
inside the thread it belongs to. The moment anyone adds `priority`, `order`,
`column`, or a cross-channel list view, they are building the deferred board and
must go back to the founder. That tripwire is written into the non-goals above
and into `slice-01`'s review checklist.

D-037(3)'s work-overview lens is a **consumer** of what this epic makes durable,
not a part of it. If the lens is prioritized later, `missionState()` is exactly
the read-only projection it would aggregate. Nothing here builds it.

### #116 / PR #120 — roles and routing

Roles ride kind `10100` with `["crew-role", role]`
(`desktop/src-tauri/src/commands/crew_role_publish.rs:54-76`); routing is an ACP
session seam (`routing_channel_id` / `routing_channels`,
`crates/buzz-acp/src/pool.rs:66,113,142-207,976-989`).

**Mission must not carry an assignee.** Who works is still `@mention` +
role/routing. A Mission is *what*, not *who*. The only permitted interaction:
the promotion affordance may **read** role/routing to pre-fill the mention it
suggests — never to record one.

### #121 / PR #128 — evidence and acceptance

Acceptance is already NIP-25 kind `7` on the receipt. Mission `completed`
**reads** that reaction. It must not add an "Accept Mission" button — that would
be a third meaning for ✅ (their open D-2 already flags two). Evidence cards
render as themselves inside the thread; the Mission strip does not re-summarize
them.

### #119 / PR #134 — readiness

Readiness states (`ready`, `missing`, `broken_config`, `binary_missing`,
`auth_unknown`, `hermes_profile_readiness.rs:20-39`) are about the *agent*, not
the work. Promotion may surface a blocking readiness state as a preflight
warning; the Mission never stores it.

### #109/#125/#110 — Projects surfaces and E2E reality

PR #129 shows the Projects surface needs an explicit relay-connection wait and a
disclosure expansion before its outcome surfaces assert
(`expandProjectPlumbing`); PR #131 shows the mock bridge only recently honours
`filter.ids`. **Reading:** the Projects surfaces are real but their E2E is
young. Slice 01 therefore ships **channel-only** and touches no Projects spec.
Projects consume the same projection in a later slice, after #109/#125 land.

### Files this epic must be careful with

`desktop/src/features/messages/ui/MessageRow.tsx` is **980 / 1000** lines
against the ratchet (`desktop/scripts/check-file-sizes.mjs:8`), and D-022
forbids raising `MAX_LINES`. #121 is already spending ≤8 of those lines. The
Mission strip and promotion affordance must live in **new Crew-owned
components**, not in `MessageRow.tsx`.

## 4. Red-team pass

Adversarial review of this plan. **11 findings — 8 applied, 3 rejected.**

| # | Finding | Disposition |
| --- | --- | --- |
| RT-1 | "Prefer a projection over existing events" is a comforting phrase that hides the one thing that genuinely cannot be projected: human promotion intent. A plan could ship "pure projection!" and then quietly add a store | **Applied** — §1.2 names the gap explicitly and escalates it as D-1 instead of resolving it |
| RT-2 | `completed` derived from `conversationOutcomeLedger` would silently expire after 4 hours; `needs_input` from `needsYouStore` after 30 minutes. Both are the obvious wiring and both are wrong | **Applied** — §1.1 forbids both by name; RED contract in slice-01 asserts a Mission reconstructed with those stores empty |
| RT-3 | A Mission strip with a state chip *is* a board cell. Add ordering and priority "for convenience" and D-037(2) is violated without anyone deciding to violate it | **Applied** — explicit tripwire non-goal + the §3 boundary statement |
| RT-4 | The issue's DoD step 5 ("existing Project/ACP tooling provisions one isolated worktree") is **not achievable for an ordinary channel thread** — provisioning is fail-closed on trusted Project workspace metadata | **Applied** — spike Q2 + escalated as D-3. The checkbox is neither dropped nor faked |
| RT-5 | DoD steps 9-12 depend on receipts, which are **off by default** (`config.rs:1495`). A plan that assumes receipts flow would burn a whole slice before discovering it | **Applied** — spike Q3 runs before slice 01 |
| RT-6 | Anyone in a channel can publish a `crew-mission` tag. Without an author check, any member (or a compromised agent) can promote or cancel the founder's work | **Applied** — owner-authored + strict `h`/`e`-ancestry validation, mirroring `agentReceiptStore.ts:107-175`; RED contract rejects a non-owner marker |
| RT-7 | Repeated clicks publish duplicate markers; a naive projection creates two Missions or flaps | **Applied** — first valid `promote` wins, later duplicates ignored; deterministic ordering by `(created_at, event id)`; RED contract for out-of-order and duplicate arrival |
| RT-8 | Every promotion posts a visible message into the thread — the founder may experience this as chat spam, and there is no "quiet" variant | **Applied as a stated trade-off, not solved** — surfaced in D-1 option A's cons so the founder chooses with eyes open |
| RT-9 | "Just add a case in `MessageRow.tsx`" trips the 980/1000 ratchet and tempts a D-022 violation | **Applied** — §3 mandates new Crew-owned components |
| RT-10 | Add a `30xxx` addressable Mission record: replaceable, clean edits, natural LWW | **Rejected** — a new kind is exactly the "new authoritative state" the issue and D-025 push back on, it is board-schema-adjacent under D-037(2), and it locks a shape in before slice 01 has taught us what a Mission needs. Revisit only if spike Q1 fails |
| RT-11 | Skip promotion entirely: infer a Mission from "a worktree exists" or "a receipt exists". Zero new state, zero decisions | **Rejected** — it contradicts the issue's explicit-promotion rule, cannot express `planned` (promoted, not started) or `cancelled`, and makes the founder's intent a side effect of tooling. Recorded because it is the cheapest option and the founder may still want it — it is D-1 option C |
| RT-12 | Ship the suggestion card in slice 01 so the feature "feels smart" | **Rejected** — a threshold heuristic (issue § "Mission promotion threshold") is an unproven product bet; getting it wrong trains the founder to ignore the card. It costs nothing to defer to slice 02+ once real promotions exist to learn from |

**Blocking findings remaining:** none technical. D-1, D-2 and D-3 are founder
decisions and gate slice 01.

## Founder decisions required

Reported separately; recorded here so the plan is self-contained. **This PR
takes no D-number.**

### D-1 — What carries promotion intent

- **A (recommended):** tagged ordinary message, `["crew-mission","promote"]` on
  kind 9. *Pro:* no new kind, precedent in #121, degrades readably everywhere,
  free authorization/ordering. *Con:* a visible message in the thread each time
  (RT-8); a Crew tag schema while D-037(2) defers board schema (boundary in §3).
- **B:** new addressable kind (`30xxx`). *Pro:* clean replace/edit semantics.
  *Con:* new authoritative state, upstream `kind.rs` edit, board-schema-adjacent,
  locks the shape in early.
- **C:** no promotion marker at all — infer Missions from worktree/receipt
  existence. *Pro:* zero new state, zero decisions. *Con:* contradicts the
  issue's explicit-promotion rule, cannot express `planned` or `cancelled`.

### D-2 — Does the Mission marker count as the deferred board schema

D-037(2) says no board tag schema until a board surface is prioritized. Option A
is a tag schema, but a thread-scoped one with no lane/order/priority. **Ask:**
confirm the §3 boundary (thread-scoped enum + title is *not* board schema), or
treat D-037(2) as blocking and take option C.

### D-3 — Where can a thread be promoted

Worktrees are fail-closed on trusted Project workspace metadata. So either:

- **A (recommended):** slice 01 allows promotion **anywhere**, and a Mission
  outside a Project simply has no worktree — durable state and decisions still
  work, the founder just gets no isolated checkout.
- **B:** promotion is **Project-channel-only** until a later slice. Honest, but
  much narrower than the issue reads.
- **C:** promotion **publishes** Project workspace metadata for ordinary
  channels. Widest, but it opens the trusted-metadata/local-path security path
  (`thread_workspace.rs:244-270`) and needs its own review.

## Verification (per slice)

- Desktop unit: colocated `*.test.mjs` beside the projection, fixture-driven,
  asserting reconstruction with all in-memory stores empty.
- Desktop E2E: `desktop/tests/e2e/`, `installMockBridge`
  (`desktop/tests/helpers/bridge.ts:911-930`) and `waitForMockLiveSubscription`
  (predicate poll, `desktop/tests/e2e/mentions.spec.ts:163-180`) — never fixed sleeps.
- Screenshots via the existing headless harness only, hash-checked for
  uniqueness before posting. **Never computer-use.**
- Gates: `pnpm --filter buzz check`, `pnpm --filter buzz typecheck`,
  `pnpm --filter buzz test`, `just test-unit`; Rust gates if any slice touches
  `buzz-acp`/`buzz-core` (slice 01 should touch neither).
- Anti-drift: any slice that changes shipped behavior updates
  `docs/crew/STATE.md` in the same PR (D-031).

## Rollback

Additive throughout. Reverting the desktop code leaves already-published marker
messages rendering as ordinary text messages in the thread — readable, harmless,
no migration, no data loss. Nothing is written to the relay that another client
must understand.

## Constraints on execution

- All PRs target `Nuncio-hq/crew`. Never `block/buzz` (D-020).
- `git commit -s` (DCO). `spike → RED → implement → verify` per slice
  (D-008, AGENT-WORKING-AGREEMENT MUST NOT #8).
- Take the next genuinely free D-number at implementation time; D-028…D-039 are
  claimed by in-flight PRs as of 2026-08-11.
