# Spike 0055 — Call-by-name via mention → ACP wake (#230)

- **Status:** PASS
- **Date:** 2026-08-19
- **Issue:** [#230](https://github.com/Nuncio-hq/crew/issues/230)
- **Commit:** `c795b02add75ec3c14967d6de232d2084c283ec8` (`main` tip at spike time; branch `cursor/call-by-name-230-4d3a`)

## Question

Can CoS wake a sleeping specialist **by name** using the existing Buzz
mention → ACP wake path (#169), or do we need a new MCP / `buzz agents call`
wrapper (and possibly a new kind) before CoS can aim a wake at one thread?

## Decision affected

Issue #230 first slice: docs-only teach-CoS (`A`), thin
`buzz agents call` CLI (`B`), or MCP `call_agent` (`C`). Also whether a new
Nostr kind is required (product rule 3: prefer existing mention → wake).

## Hypothesis

Same-owner sibling agents already fire turns under the default
`respond_to=owner` author gate. A room message with a `p` tag for the
specialist is flushable work; lazy pool `Listening → Waking → Ready`
(#169) starts the engine. No new kind is required. A thin CLI wrapper is
still the right first product surface because plain `--mention` is easy to
miss as a deliberate “call” and hard to report consistently in the room.

## Scope

- Code-path read of `buzz-acp` pool lifecycle + mention filter + author gate
- CLI: `buzz messages send --mention`, `buzz agents` subcommand inventory
- Desktop sleeping badge / lifecycle projection
- Fail-closed paths: unknown name, non-member, wrong community / relay

## Exclusions

- Live two-agent relay smoke (CoS + sleeping Dev) — deferred to follow-up
  test contract
- Founder Inbox Wake / Continue UX (explicit non-goal of #230)
- Call-by-role API; Mission machine (#102 / #151)
- Implementing `buzz agents call` or MCP `call_agent` in this spike

## Pass criteria

Observable from the tree (no new production code):

1. Sleeping agents wake on **`p`-tag mention**, not body text alone
   (`require_mention` / `event_mentions_agent`).
2. Lazy pool starts from `Listening` when flushable work arrives
   (`PoolLifecycle::start_wake_if_due`).
3. An agent under ACP can already emit that mention via
   `buzz messages send … --mention` (or resolvable `@Name`) with
   room-visible content — **no new kind**.
4. Same-owner siblings are accepted by the inbound author gate under
   default `OwnerOnly` (so CoS → specialist is not blocked as “external”).
5. Fail-closed: unresolved/ambiguous name, non-member pubkey, and
   cross-community (wrong relay / not a channel member) stop before a
   successful silent wake.
6. Work is aimed per **conversation/thread** identity inside a channel
   (sibling thread does not receive this event’s turn).

## Fail criteria

Any of:

- Wake requires a founder-only control event or a new kind.
- Agent-authored mentions cannot fire turns (author gate rejects all
  non-owner authors, including siblings).
- Mentions are body-only with no `p` tag path agents can emit.
- There is no channel/thread-scoped queue identity (every wake collapses
  all threads into one turn without separation).

## Environment

- Repo: `Nuncio-hq/crew` on branch `cursor/call-by-name-230-4d3a`
- OS: Linux 6.12.94+ x86_64
- Method: static code + existing unit contracts (`pool_lifecycle`,
  `conversation`, CLI mention preflight). No live relay process in this
  spike.
- Auth class: NIP-OA same-owner siblings (managed agents under one
  founder); no secrets recorded.

## Method

1. Trace #169 sleep/wake: `pool_lifecycle` + main-loop `lazy_wake_work_pending`.
2. Trace mention acceptance: `filter::match_event` `require_mention`,
   `event_mentions_agent`, subscribe mode `Mentions`.
3. Trace author gate: `author_allowed` / `is_owner_or_sibling`.
4. Inventory CLI `agents` vs `messages send --mention`.
5. Trace conversation IDs for thread aiming.
6. Trace CLI fail-closed mention resolution + membership.

## Results

### 1) Sleeping → wake on mention (#169)

Lazy pool emits `listening` at start; flushable queued work starts a wake:

```8:13:crates/buzz-acp/src/pool_lifecycle.rs
//! Transitions (issue #169):
//! ```text
//! Listening ──(work)──► Waking ──► Ready ──(idle ≥ T, safe)──► Draining ──► Listening
//!                                ▲                                          │
//!                                └───────────────(new work)────────────────┘
//! ```
```

```77:108:crates/buzz-acp/src/pool_lifecycle.rs
    /// Start the first wake, or a due retry, when buffered work exists.
    pub(crate) fn start_wake_if_due(
        &mut self,
        has_pending_work: bool,
        now: Instant,
    ) -> Option<u32> {
        if !has_pending_work {
            return None;
        }
        // ...
        if let Some(attempt) = next_attempt {
            *self = Self::Waking { attempt };
        }
        next_attempt
    }
```

Desktop projects that state as **Sleeping · wakes on mention**:

```10:22:desktop/src/features/agents/managedAgentRuntimeStatus.ts
export const MANAGED_AGENT_SLEEPING_BADGE_LABEL = "Sleeping · wakes on mention";
// ...
    case "listening":
      return "Sleeping";
```

Default subscribe mode requires a self-`p` tag:

```2110:2129:crates/buzz-acp/src/lib.rs
    let rules: Vec<SubscriptionRule> = match config.subscribe_mode {
        SubscribeMode::Mentions => {
            vec![SubscriptionRule {
                name: "mentions".into(),
                // ...
                require_mention: !config.no_mention_filter,
                prompt_tag: Some("@mention".into()),
            }]
        }
```

```387:398:crates/buzz-acp/src/filter.rs
        if rule.require_mention {
            let mentioned = event.tags.iter().any(|tag| {
                let s = tag.as_slice();
                s.first().map(|k| k.as_str()) == Some("p")
                    && s.get(1).map(|v| v.as_str()) == Some(agent_pubkey_hex)
            });
            if !mentioned {
                continue;
            }
        }
```

```3788:3792:crates/buzz-acp/src/lib.rs
fn event_mentions_agent(event: &nostr::Event, agent_pubkey_hex: &str) -> bool {
    event.tags.iter().any(|t| {
        t.as_slice().first().map(|s| s.as_str()) == Some("p")
            && t.as_slice().get(1).map(|s| s.as_str()) == Some(agent_pubkey_hex)
    })
}
```

### 2) Existing CLI — mention yes; `agents call` no

`messages send` already takes repeatable `--mention` and resolves `@Name`
against channel members:

```452:454:crates/buzz-cli/src/lib.rs
        /// Pubkey to mention (hex or npub; repeatable). …
        #[arg(long = "mention")]
        mentions: Vec<String>,
```

```604:613:crates/buzz-cli/src/commands/messages.rs
    let missing = missing_members(&mention_pubkeys, &member_pubkeys);
    if !missing.is_empty() {
        return Err(CliError::Usage(
            serde_json::json!({
                "message": "mentioned pubkeys are not channel members; add them explicitly before retrying",
                "missing_member_pubkeys": missing,
```

```135:146:crates/buzz-cli/src/commands/messages.rs
            [] => {
                return Err(CliError::Usage(format!(
                    "mention '@{name}' does not match a current channel member; retry with --mention <pubkey>"
                )))
            }
            // ...
                    "mention '@{name}' is ambiguous; candidates: {}. Retry with --mention <pubkey>",
```

`buzz agents` today is only draft/archive surfaces — **no `call`**:

```2263:2271:crates/buzz-cli/src/lib.rs
        assert_eq!(
            names(&cmd, "agents"),
            vec![
                "archive",
                "archived",
                "draft-create",
                "draft-update",
                "unarchive"
            ]
        );
```

Base prompt already teaches CoS-class agents to use `--mention` for
delivery evidence (`crates/buzz-acp/src/base_prompt.md` Mentions section).
There is **no** `call_agent` tool in `buzz-dev-mcp` (wiki / desktop / shell
faces only).

### 3) Can CoS aim a wake without a new kind?

Yes. Under ACP, CoS runs `buzz` with its own key. Publishing an ordinary
channel message (kind 9 / stream message) that `p`-tags the specialist is
the wake. Sibling authors skip the human mis-click hold and are allowed
through the author gate:

```242:278:crates/buzz-acp/src/lib.rs
/// Both `OwnerOnly` and `Allowlist` accept the owner and same-owner siblings;
/// `Allowlist` additionally accepts the explicit external pubkey list.
async fn author_allowed(
    respond_to: &RespondTo,
    allowlist: &HashSet<String>,
    author: &str,
    is_dm: bool,
    // ...
) -> bool {
    // ...
    match respond_to {
        RespondTo::Anyone => true,
        RespondTo::Nobody => false,
        RespondTo::OwnerOnly => is_owner_or_sibling(author, owner_cache, rest_client).await,
        RespondTo::Allowlist => {
            allowlist.contains(author)
                || is_owner_or_sibling(author, owner_cache, rest_client).await
        }
    }
}
```

```3073:3085:crates/buzz-acp/src/lib.rs
                            // Sibling agents do not mis-click — skip the hold.
                            let hold_exempt = owner_cache.get() != Some(author_hex.as_str())
                                && is_owner_or_sibling(
                                    &author_hex,
                                    &owner_cache,
                                    &ctx.rest_client,
                                )
                                .await;
                            let accepted = queue.push(QueuedEvent {
                                channel_id: inbound_conversation_id,
```

Thread aiming: channel events get a **per-root conversation id**; multiple
threads in one channel are independent scheduler slots:

```5:19:crates/buzz-acp/src/conversation.rs
/// Channel messages use their NIP-10 root event, or their own event ID when
/// starting a new thread. This lets multiple threads in one channel occupy
/// independent pool slots without changing the queue and session-state APIs
/// that already accept UUIDs.
pub fn id_for_event(channel_id: Uuid, event: &Event, is_dm: bool) -> Uuid {
    // ...
    deterministic_id(channel_id, &root)
}
```

A call that `--reply-to` the target thread (or starts a top-level message)
queues work under that conversation id only. The specialist’s **process**
may wake globally from `Listening`, but the turn content/session routing
is that conversation — sibling threads do not receive this event.

Optional `--handoff` already exists on `messages send` for manager-chain
work creation (`crew-handoff`); #230’s first slice does **not** require it
for a pure wake-by-name call.

### 4) Fail-closed cases

| Case | Behavior today |
|------|----------------|
| Unknown / ambiguous `@Name` without `--mention` | CLI `Usage` error; no publish |
| `--mention` of non-member | CLI `Usage` with `missing_member_pubkeys`; no publish |
| Wrong community (other relay) | CoS ACP/`buzz` is bound to `BUZZ_RELAY_URL`; target harness on another community never sees the event. On this relay, non-membership fails preflight. |
| Target `respond_to=nobody` | Author gate drops; no turn |
| Cross-owner agent under `OwnerOnly` | Not a sibling → dropped |
| Unresolved channel type | Treated as DM for author gate (fail closed) |

No parallel “call bus” or new kind is required for these closes.

## Edge cases observed

- **Body `@Name` without `p` tag does not wake.** Delivery is tag-gated.
  Agents must use resolvable names or `--mention`.
- **Engine wake is process-wide; turn aim is conversation-scoped.** Prove
  “sibling thread untouched” at the queue/session layer, not by claiming
  the child process stays asleep.
- **Partial display names** — base prompt warns they fail; call-by-name
  product should resolve against member profiles the same way CLI does
  (exact / unique match).
- **`respond_to` misconfig** on a specialist is an operator footgun, not a
  missing call API.
- Issue text references spikes 0052/0053; those files are **not** in this
  tree yet. This spike does not depend on them for the wake path conclusion.

## Limitations

- No live CoS→Dev relay smoke in this environment (would need two managed
  pairs, lazy pool, and a sleeping specialist).
- Did not re-run `pool_lifecycle_state` or CLI unit tests here; contracts
  already exist and pin the cited transitions.
- Name→agent discovery beyond channel membership (org roster / portfolio)
  was not required to answer the wake question.

## Verdict

**PASS.** CoS can wake a sleeping same-owner specialist by name using
existing `buzz messages send` mention → `p` tag → ACP lazy-pool wake, with
thread aim via conversation ids and fail-closed membership/name checks.
**No new Nostr kind and no MCP tool are required for the wake mechanism.**

**Smallest first slice recommendation: B** — thin `buzz agents call`
that posts a **room-visible** call message and attaches the target
`--mention` (reuse `cmd_send_message`; optional `--reply-to` for thread
aim). Prefer B over A because deliberate “call” is easy to miss amid
narrative `@` rules and raw `--mention` choreography; prefer B over C
because agents already live on `buzz` under ACP and a CLI wrapper stays
thin-fork / inspectable without a new MCP face.

## Follow-up test contract

Must be RED before implementing `buzz agents call` (or teaching A as the
only path):

1. **CLI unit / integration:** `buzz agents call --channel <id> --agent Dev`
   (or `--pubkey`) publishes kind-9 (or channel default) with content that
   names the call, `mention_pubkeys` contains exactly Dev’s hex, and exit 0.
2. **Fail closed:** unknown name → exit 1; non-member pubkey → exit 1 with
   `missing_member_pubkeys`; no event on relay.
3. **ACP / pool (lazy):** specialist in `Listening`; CoS (sibling) call on
   thread T1 → specialist lifecycle `waking`→`ready` and one turn whose
   `[Context]` / queue batch is conversation id for T1; T2 has no new
   flushable batch from that event (`conversation::id_for_event` separation).
4. **Author gate:** same call from a non-sibling external pubkey under
   specialist `OwnerOnly` does **not** start a turn (drop / no wake from
   that event).
5. **Wrong community:** call against a channel/agent only on relay B while
   CLI is aimed at relay A fails closed (membership or network), never
   silent success.

Optional later (not first slice): MCP `call_agent` as a thin alias of the
same CLI; `--handoff` composition when assignment must auto-create work
(spike 0053 / D-053 territory).

## Cleanup

Docs only. No temporary processes, fixtures, or production code changes.
