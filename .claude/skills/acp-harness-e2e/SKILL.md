---
name: acp-harness-e2e
description: Run real-process end-to-end tests of the buzz-acp harness (ACP protocol features such as elicitation/user-input, cancel, timeouts) against a local relay using a scriptable fake ACP agent.
---

# End-to-end testing of `buzz-acp` (ACP harness) features

Use this when a change touches `crates/buzz-acp` and you need *runtime* proof (real wire frames),
not unit tests. Driving a real Claude/Codex agent into an exact protocol state is unreliable — a
tiny fake ACP agent over stdio is faster and deterministic, and the harness cannot tell the
difference.

## Stack

1. Postgres + Redis (Docker, per `TESTING.md` / `Justfile`), then
   `cargo build --release -p buzz-relay -p buzz-acp -p buzz-cli` and run
   `./target/release/buzz-relay`. Start it with
   `RUST_LOG=buzz_relay=debug,info ./target/release/buzz-relay > /tmp/relay.log 2>&1 &` —
   the debug log gives you per-event `EVENT`/`ingest_event`/`Fan-out` lines. `Fan-out … match_count:0`
   is the fastest way to prove that *no subscriber's filter matched* an event.
2. Identities: `buzz keys generate` for owner / agent / a third "stranger" key. Export
   `BUZZ_PRIVATE_KEY` per command; `BUZZ_RELAY_URL=http://localhost:3000` for the CLI and
   `ws://localhost:3000` for the harness. Never echo private keys.
3. Channel: `buzz channels create --name x --visibility open --type stream`
   (`--type` is mandatory), then `buzz channels add-member --channel <id> --pubkey <agent>`.
   `buzz dms open --pubkey <agent>` creates a DM channel.
4. Harness:
   `BUZZ_PRIVATE_KEY=<agent> BUZZ_ACP_AGENT_COMMAND=python3 BUZZ_ACP_AGENT_ARGS=/path/fake_agent.py \
    ./target/release/buzz-acp --agent-owner <owner pubkey hex> --idle-timeout 20 --max-turn-duration 40`
   Boolean env flags need an explicit value (`BUZZ_ACP_NO_MENTION_FILTER=true`, not `=1`);
   the CLI-style `--flag` form works too.

## Fake ACP agent

A ~120-line Python script reading JSON-RPC lines on stdin and writing them on stdout, logging every
frame to a file, is enough. Handle `initialize`, `session/new`, `session/prompt`, `session/cancel`,
and reply `{}` to anything unknown so the harness never hangs. Drive behaviour with an env var
(`ask`, `silent`, …) so one script covers happy path, timeout controls, and cancel. The frame log is
your primary evidence: it shows the exact `clientCapabilities` on `initialize` and the exact
`{"action": …}` result the harness sends back.

## Gotchas that cost real time

- **Conversation id ≠ channel id.** For non-DM channel messages the harness's queue/session key is
  `conversation::id_for_event()` = `sha256("buzz-acp-conversation-v1" || channel || root_event_id)`.
  Any code that publishes a channel-scoped event must map back to the real channel (`h` tag /
  `conversation::routing_channel_id`). If the relay answers
  `OK false "restricted: not a channel member"` for an event whose author *is* a member of an *open*
  channel, suspect that the `h` tag holds a conversation UUID. Confirm with
  `select count(*) from channels where id='<uuid>'`. A DM channel is a useful control, since there
  the conversation id equals the channel id.
- **The harness only receives what its REQ subscribes to.** Subscription kinds are built in
  `config.rs::resolve_channel_filters` / `resolve_dynamic_channel_filter` (per `--subscribe` mode),
  *not* from the rule list in `lib.rs`. Default `Mentions` mode also adds `#p = <agent pubkey>`, so
  any event without a `p` tag mentioning the agent is never delivered. Workarounds while testing:
  `BUZZ_ACP_KINDS=<comma list>` and `BUZZ_ACP_NO_MENTION_FILTER=true`. Be aware that adding a kind to
  the subscription also makes those events trigger new prompt turns.
- Shrink timers with `--idle-timeout 20 --max-turn-duration 40` and always run a *control* case
  (agent that never answers) to prove the timers actually fire before claiming a feature suspends them.
- `!cancel` must be a kind-9 message from the agent owner that mentions the agent; `!shutdown`
  and `!rotate` work the same way.
- Restarting the harness from a tool call: `pkill -f buzz-acp` frequently kills the calling shell
  chain too. Launch with `nohup … &` in its own call and verify with `pgrep -af release/buzz-acp`.
- Each turn is conversation-scoped, so several parked turns can coexist in one channel and
  `buzz user-input list`-style queries will show all of them; restart the harness between scenarios
  for clean, attributable evidence.

## Devin Secrets Needed

None — everything runs against a local relay with locally generated keys.
