# Spike 0015 — Role record shape and projection (`30179` + `10100` crew-role)

- **Status:** PASS
- **Date:** 2026-08-10
- **Plan:** [`../../../plans/20260810-agent-roles-routing-capability/plan.md`](../../../plans/20260810-agent-roles-routing-capability/plan.md) Slice 0 Spike A
- **Issue:** [Nuncio-hq/crew#116](https://github.com/Nuncio-hq/crew/issues/116)

## Question

Can the owner-signed managed-agent record (kind `30179`,
`crates/buzz-core/src/private_managed_agent.rs`) carry a role field whose value
is projected to a public tag on the agent's `KIND_AGENT_PROFILE` (`10100`)
without breaking existing consumers, and does the tag survive relay round-trip
and stay ignorable by non-Crew clients?

## Decision affected

Slice 1 role storage detail: extend `30179` content vs sibling owner-signed
event; public projection tag name/shape on `10100`; whether stock consumers
need a compatibility shim.

## Hypothesis

1. `Payload.extensions` (namespaced keys containing `:`) is the forward-compat
   path for role on `30179` without touching core fields
   (`deny_unknown_fields` rejects unknown top-level members).
2. An unknown `["crew-role","code"]` tag on `10100` is stored and returned by
   the relay; `handle_agent_profile` only reads `content.channel_add_policy`
   and ignores tags.
3. Outer `30179` tags remain exactly `d`/`g`/`prev`/`state` (role must not
   appear there).

## Scope

- Isolated Postgres/Redis/MinIO (`docker compose -p buzz-spike116 -f
  docker-compose.harness.yml`), `buzz-relay` on `:3030` / health `:8088`.
- Disposable probe under `/tmp/spike116/scratch` (not production code).
- Codec path: `buzz_core::private_managed_agent::{build_event,
  validate_and_decrypt}`.
- Live path: `POST /events` + fresh `POST /query` cold read.

## Exclusions

- Did not open stock Buzz / NuncioCrew desktop GUI against this isolated
  community (would risk pointing the installed app at throwaway keys). Stock
  consumer safety is evidenced by the relay side-effect path for `10100` and
  code inspection of `handle_agent_profile`.
- Did not exercise Desktop managed-agent dual-write of `30179` (still inert
  aggregate path for product use; NIP-PMA notes full CAS/privacy deployment
  order). This spike proves the **codec + current generic ingest** boundary.
- No Slice 1 production projection builder.

## Pass criteria

Both records round-trip; role readable from `10100`; stock UI/consumer path
unaffected (side effect still applies `channel_add_policy`; unknown tag
preserved, not stripped/rejected).

## Fail criteria

Any consumer rejects/strips the extension, or role cannot be recovered after
cold read.

## Environment

- Commit: `06107122b` (worktree `feat/issue-116-agent-roles`)
- OS: macOS 26.5.2 arm64
- Relay: `target/release/buzz-relay` against Postgres `localhost:5471`, Redis
  `localhost:6471`, MinIO `localhost:9471`
- Auth class: local dev relay (`BUZZ_REQUIRE_AUTH_TOKEN=false`), X-Pubkey
  submit; no secrets in this record
- Probe binary: `/tmp/spike116/scratch` (throwaway)

## Method

1. `docker compose -p buzz-spike116 -f docker-compose.harness.yml up -d`
2. `buzz-admin migrate` on `DATABASE_URL=postgres://buzz:buzz_dev@localhost:5471/buzz`
3. Start relay with harness S3/ports (tmux session `spike116-relay`)
4. Mint owner + two agent hex keys via `buzz-admin generate-key`
5. Build payload with `extensions["crew:role"] = "code"`, `build_event`, local
   decrypt assert
6. Publish signed `30179` (owner) and `10100` with tags
   `["crew-role","code"]` + content `{"channel_add_policy":"owner_only"}`
   (agent); baseline second agent `10100` without role tag
   (`channel_add_policy=nobody`)
7. Cold `POST /query` for both kinds; decrypt `30179`; inspect tags on `10100`
8. SQL: `users.channel_add_policy` after side effect

Raw evidence archived under
[`assets/0015-role-record-projection/`](assets/0015-role-record-projection/).

## Results

### Codec (`30179` + extensions)

- Local round-trip: **OK**
- Event id: `53564dec3a37810712af70af914c4ed3fd624c145c4def9a10505f90cd77fee5`
- Outer tags only: `d` (agent pubkey), `g=1`, `state=active` (no role tag on
  envelope — required by NIP-PMA tag grammar)
- Decrypted extensions: `{"crew:role":"code"}`
  ([`30179-decrypted-extensions.json`](assets/0015-role-record-projection/30179-decrypted-extensions.json))

### Live relay publish + cold read

| Kind  | Publish                         | Cold read | Role recoverable                                      |
|-------|---------------------------------|-----------|--------------------------------------------------------|
| 30179 | `accepted:true`                 | 1 event   | decrypt → `crew:role=code`                             |
| 10100 | `accepted:true` (with crew-role)| 1 event   | tags include `["crew-role","code"]`; content unchanged |
| 10100 | baseline without tag            | n/a       | accepted; side effect only                             |

Cold-query excerpts:

- `10100` tags after cold read:
  `["crew-role","code"]`, `["alt","agent profile with crew role"]`
  content still `{"channel_add_policy":"owner_only"}`
  ([`10100-cold-query.json`](assets/0015-role-record-projection/10100-cold-query.json))
- `30179` cold decrypt extensions identical to local
  ([`30179-cold-decrypted-extensions.json`](assets/0015-role-record-projection/30179-cold-decrypted-extensions.json))

### Stock consumer unaffected

Relay side effect `handle_agent_profile`
(`crates/buzz-relay/src/handlers/side_effects.rs:1161-1192`) only parses
`content.channel_add_policy`. After publish:

```text
pubkey (agent with crew-role)  → channel_add_policy = owner_only
pubkey (baseline, no tag)      → channel_add_policy = nobody
```

Unknown `crew-role` tag was **not** rejected and did **not** block the stock
side effect. CLI `set-add-policy` path still emits empty tags
(`crates/buzz-cli/src/commands/channels.rs:1035-1041`); additive tags are a
projection concern for Crew clients only.

### Storage shape decision evidence

- Top-level `role` field on `Payload` would fail `deny_unknown_fields` until a
  schema version bump — **not** viable without coordinated codec change.
- Namespaced `extensions["crew:role"]` works today end-to-end on current
  ingest (kind is in `required_scope_for_kind` allowlist). Product still
  follows NIP-PMA deployment order for **private aggregate authority**; for
  Slice 1 day-one the public `10100` tag alone may be enough for prompt
  injection if private dual-write is not yet productized.

## Edge cases observed

- Extension keys **must** contain `:` (`crew:role` OK; bare `role` fails
  `validate_payload` at `private_managed_agent.rs:427-431`).
- Outer envelope rejects unexpected tags (role cannot live on `30179` tags).
- Generic ingest currently **accepts** `30179` even though NIP-PMA draft text
  says relays MUST reject until privacy/CAS gates land
  (`docs/nips/NIP-PMA.md:3-5`, step 1 at line 101). Decision-changing: Slice 1
  must not treat live `30179` accept as full product authority — public
  projection on `10100` remains the safe day-one surface for other clients.

## Limitations

- No stock desktop GUI session against this isolated community.
- No proof of Desktop local-record field round-trip (that is Slice 1 RED).
- Privacy: cold query as owner returned ciphertext; stranger decrypt fails
  closed (codec-tested in unit suite; not re-probed here against relay ACL
  filtering).
- Whether FTS/search indexes `30179` content is out of scope.

## Verdict

**PASS** — `30179` carries role via namespaced `extensions["crew:role"]`;
public `10100` tag `["crew-role","code"]` survives relay round-trip and cold
read; stock `channel_add_policy` side effect still applies; consumers neither
reject nor strip the unknown tag.

## Follow-up test contract (RED before Slice 1)

1. Parse/serialize role from managed-agent record (Desktop) and from
   `extensions["crew:role"]` when reading `30179`.
2. Projection builder emits exactly one `["crew-role", <role>]` on `10100`.
3. Non-founder pubkey role events ignored (authority = owner pubkey).
4. Missing role ⇒ no role section injected (behavior unchanged).
5. Role removal clears projection tag.
6. Unknown extra tags on `10100` preserved across replaceable update.
7. Stock `channel_add_policy` still applied when `crew-role` present.

## Cleanup

- Probe remains disposable under `/tmp/spike116/` (not in repo).
- Evidence copies committed under `docs/crew/spikes/assets/0015-…`.
- Isolated compose project `buzz-spike116` and tmux `spike116-relay` left up
  for Spikes B/C in the same session; tear down after Slice 0 completes:
  `tmux kill-session -t spike116-relay`;
  `docker compose -p buzz-spike116 -f docker-compose.harness.yml down -v`.
