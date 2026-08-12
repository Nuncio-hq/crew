# Verification 0011 — E2E bridge relay mutation audit

- **Date:** 2026-08-12 (updated for #172 / #144; original #133 2026-08-11)
- **Issue:** #133; follow-ups #144, #172
- **Branch / commit:** see PRs for #144 and #172 SHAs
- **Plan phase:** relay-backed desktop mutation coverage

## Boundary exercised

The desktop E2E bridge now sends real Nostr mutations when
`getIdentity(config)` returns a relay identity. Relay branches call
`submitSignedEvent` (or `submitSignedEventWithIdentity` for managed-agent
authorship), and do not update the mock message/feed stores or emit a
mock live event. The real WebSocket subscription supplies the relay echo.
Mock mode keeps the existing in-memory behavior.

Decision **D-042**: mutating bridge commands with a real Nostr event behind
them publish through `submitSignedEvent` in relay mode and skip mock-store
bookkeeping.

The single-run evidence path (#133) is:

1. Publish an evidence-tagged kind-9 message as Alice with `POST /events`.
2. Open the message in the headless desktop app as Tyler.
3. Click `evidence-accept`; poll `POST /query` for Tyler's kind-7 `✅` with the
   evidence event's `e` tag.
4. Assert the Accepted card state.
5. Click `evidence-reject`; poll for Tyler's kind-7 `❌`, assert the Rejected
   card state, and assert the reply composer remains visible.

The evidence message and both reaction events use the real local relay. The
agent ownership metadata remains a bridge-config profile injection; see
[Limits](#limits).

Issue **#144** extended the same boundary to the remaining mock-only
mutating commands listed under [(a) Already relay-aware](#a-already-relay-aware)
and confirmed workflow / local-archive classification against the Rust
commands (see [Rust confirmation (#144 / #172)](#rust-confirmation-144--172)).
Issue **#172** then gave the four workflow commands real relay branches
(they had been misclassified as local-only on the bridge).

## CI coverage

The relay-backed desktop specs are covered by the advisory
`Desktop E2E Integration` job in the active
`.github/workflows/nuncio-crew-ci.yml` workflow (`playwright test
--project=integration`, two shards, real Postgres/Redis/MinIO + built
`buzz-relay`, schema/community seed, `scripts/setup-desktop-test-data.sh`,
`BUZZ_RECONCILE_CHANNELS=true`). Status is **advisory** (`continue-on-error`,
excluded from `NuncioCrew Gate`) per D-047 / issue
[#147](https://github.com/Nuncio-hq/crew/issues/147); a green Gate is not
evidence that integration passed. See [CI.md](../CI.md).

History: the `integration` project was an upstream-owned lane from
`block/buzz`'s `.github/workflows/ci.yml`, introduced by upstream commit
[`a1c28f487d`](https://github.com/block/buzz/commit/a1c28f487d2af01d620619d940cf377d21c1a81a),
“Shard desktop Playwright CI jobs (#992)”. Crew disabled its inherited `CI`
workflow deliberately as part of the minimised merge gate; before #147 the
active workflow ran smoke only. For PR #146, head `70bfa70e7`, desktop filters
were true but no integration job appeared in
[31448955605](https://github.com/Nuncio-hq/crew/actions/runs/31448955605).
The clean-main push run
[31362178966](https://github.com/Nuncio-hq/crew/actions/runs/31362178966)
also had no integration job. The lane last appeared under the disabled
workflow at
[30534161705/job/90843624305](https://github.com/Nuncio-hq/crew/actions/runs/30534161705/job/90843624305).

The last completed upstream-style integration run before the Crew lane,
[30533242569](https://github.com/Nuncio-hq/crew/actions/runs/30533242569),
used two `ubuntu-latest` shards (~6m45s / ~6m17s, 20-minute per-shard
timeout). The Crew lane mirrors that shape (30-minute timeout to absorb the
per-shard relay build). `Project Relay` still provisions its own stack inside
`scripts/run-nuncio-crew-project-relay-ci.sh` and does not run Playwright; the
integration job duplicates services by design so it never becomes a hard Gate
dependency. Crew-specific relay-mutation specs:

- `evidence-reactions-relay.spec.ts` (#133)
- `bridge-relay-mutations.spec.ts` (#144 / #172) — archive/unarchive identity,
  update_persona_and_publish, send_managed_agent_channel_message,
  send_channel_user_input_answer, workflow create/update/trigger/delete

### How to run these specs locally

```bash
. ./bin/activate-hermit
docker compose up -d postgres redis minio minio-init
export BUZZ_RECONCILE_CHANNELS=true
just relay
bash scripts/setup-desktop-test-data.sh
pnpm --filter buzz build:e2e
cd desktop
pnpm exec playwright test evidence-reactions-relay.spec.ts bridge-relay-mutations.spec.ts --project=integration
```

## Local relay and test data

The Docker Postgres, Redis, and MinIO services were reused. The relay was
started with:

```bash
. ./bin/activate-hermit && export BUZZ_RECONCILE_CHANNELS=true && just relay
```

The ordinary `just relay` process was healthy but had no kind-39000 discovery
events for the SQL-seeded desktop channels. Reconciliation was therefore
required locally. The relevant startup output was:

```text
"buzz-relay TCP listening","addr":"0.0.0.0:3000","target":"buzz_relay"
{"timestamp":"2026-08-11T01:07:49.819013Z","level":"INFO","message":"reconciled channel discovery events","count":9,"target":"buzz_relay::handlers::side_effects"}
```

The desktop seed was initialized with:

```bash
. ./bin/activate-hermit && ./scripts/setup-desktop-test-data.sh
# Checking database connection...
# Desktop e2e data ready.
```

## Relay-backed evidence spec

The #133 spec is
`desktop/tests/e2e/evidence-reactions-relay.spec.ts`, registered in the
integration project. It was run with:

```bash
cd desktop
pnpm exec playwright test evidence-reactions-relay.spec.ts --project=integration
```

Verbatim result from the passing run:

```text
Running 1 test using 1 worker
  ✓  1 [integration] › tests/e2e/evidence-reactions-relay.spec.ts:88:1 › owner Accept and Reject publish real relay reactions (1.8s)

  1 passed (3.2s)
```

The spec asserts the raw relay events directly: kind `7`, Tyler's pubkey,
exact `✅`/`❌` content, and an `e` tag targeting the published evidence event.

## Relay-backed bridge mutation specs (#144)

The #144 spec is
`desktop/tests/e2e/bridge-relay-mutations.spec.ts` (integration project).
Each test invokes the bridge command path under `mode: "relay"` and polls
`POST /query` for the real event:

| Spec | Command | Kind / author | Tags asserted |
|---|---|---|---|
| archive/unarchive | `archive_identity` / `unarchive_identity` | 9035 / 9036, self | `-`, `p`, optional `reason` |
| persona catalog | `update_persona_and_publish` | 30175, tyler | `d`, content `display_name` |
| managed agent msg | `send_managed_agent_channel_message` | 9, agent (alice) | `h`, `client` marker; agent signing + NIP-OA `x-auth-tag` |
| user-input answer | `send_channel_user_input_answer` | 46041, tyler | `h`, `e`→request, `p`→requesting agent |
| workflows (#172) | `create_workflow` / `update_workflow` / `trigger_workflow` / `delete_workflow` | 30620 / 46020 / 5, tyler | create+update: `d`+`h` + YAML content; trigger: `d`; delete: `a=30620:owner:id` |

## Verification commands

The E2E bundle was rebuilt after the bridge style edits:

```bash
CI=true . ./bin/activate-hermit && CI=true pnpm --filter buzz build:e2e
```

Desktop checks:

```bash
. ./bin/activate-hermit && pnpm --filter buzz check
. ./bin/activate-hermit && pnpm --filter buzz typecheck
```

## Mock-boundary audit

“Relay-aware” means the bridge has a relay branch that publishes the real
event. “Mock-only trap” means relay mode can return a successful-looking result
without reaching the relay. “Local-only” means the bridge-side mutation is
local or an OS/plugin shim (and Rust does not publish a Nostr mutation for
that command — confirmed where noted).

### (a) Already relay-aware

| Command(s) | Evidence |
|---|---|
| `update_profile` | `handleUpdateProfile`; relay kind-0 submission. |
| `create_channel` | relay kind-9004 submission. |
| `open_dm` | signed relay submission. |
| `hide_dm` | relay submission. |
| `update_channel` | kind-9002 submission. |
| `set_channel_topic`, `set_channel_purpose` | relay submissions. |
| `archive_channel`, `unarchive_channel` | relay submissions. |
| `delete_channel` | relay kind-9008 submission. |
| `add_channel_members` | relay kind-9000 submission. |
| `remove_channel_member` | relay kind-9001 submission. |
| `join_channel` | relay kind-9021 submission. |
| `leave_channel` | relay kind-9022 submission. |
| `send_channel_message` | relay submission without mock bookkeeping. |
| `edit_message` | relay kind-40003 submission. |
| `delete_message` | **Fixed in #133.** Relay publishes the Rust-compatible kind-5 `h`+`e` event. |
| `add_reaction` | **Fixed in #133.** Relay publishes exact kind-7 content/tags. |
| `remove_reaction` | **Fixed in #133.** Relay queries the caller's own kind-7 and deletes it with kind 5. |
| `set_canvas` | **Fixed in #133.** Relay publishes kind 40100 with `h`. |
| `send_channel_user_input_answer` | **Fixed in #144.** Relay path looks up the 46040 request, then publishes kind **46041** with `h` + `e` + `p` (requesting agent). Mirrors `desktop/src-tauri/src/commands/user_input.rs`. Spec: `bridge-relay-mutations.spec.ts`. |
| `update_persona_and_publish` | **Fixed in #144.** `publishMockPersonaHead` posts kind **30175** via `submitSignedEvent` (content/tags mirror `persona_event_content` / NIP-AP); mock catalog bookkeeping skipped on success. Also covers `set_persona_shared` through the same helper. Spec: `bridge-relay-mutations.spec.ts`. |
| `archive_identity`, `unarchive_identity` | **Fixed in #144.** Relay posts kind **9035** / **9036** with `-` + `p` (+ optional `reason` / `replaced-by`); owner path attaches live kind:0 NIP-OA `auth` tag when present. Mirrors `identity_archive.rs` / `events::build_*_identity_request`. Spec: `bridge-relay-mutations.spec.ts`. |
| `send_managed_agent_channel_message` | **Fixed in #144.** Relay path signs kind **9** as the managed agent (`MockManagedAgentSeed.privateKeyHex` → real nsec), attaches client markers, and sends NIP-OA `x-auth-tag` from the owner identity (mirrors `managed_agent_submission_auth_tag`). Without a real agent key the command errors visibly rather than silently mocking. Spec: `bridge-relay-mutations.spec.ts`. |
| `create_workflow`, `update_workflow`, `delete_workflow`, `trigger_workflow` | **Fixed in #172.** Relay publishes kind **30620** (`d`+`h`, YAML content), kind **5** (`a=30620:owner:id`), kind **46020** (`d`). Skips mock workflow stores. Mirrors `commands/workflows.rs` + `events::build_workflow_*`. Spec: `bridge-relay-mutations.spec.ts`. |

### (b) Mock-only traps

None remaining from the #133/#144/#172 lists.

### (c) Legitimately local-only or confirmed / misclassified

| Command(s) | Classification after Rust confirmation (#144 / #172) |
|---|---|
| `create_persona`, `update_persona`, `delete_persona`, `set_persona_active` | Local persona catalog persistence; `update_persona` queues local pending state. Publishing is the separate `update_persona_and_publish` / `set_persona_shared` path (now relay-aware). |
| `create_channel_template` | E2E-only local template fixture state. |
| `create_team`, `update_team`, `delete_team`, `install_team_from_directory`, `sync_team_directory`, `confirm_*_snapshot_import` | Local persona/team files and SQLite/import workflows. |
| Managed-agent create/update/start/stop/delete/runtime-pair commands | Local records, process lifecycle, and runtime bookkeeping (agent **message** authorship is relay-aware above). |
| `create_hermes_profile`, `delete_hermes_profile` | Local Hermes profile-manager state. |
| `save_custom_harness`, `delete_custom_harness`, `connect_acp_runtime`, `install_acp_runtime` | Local ACP configuration/process installation. |
| `plugin:process\|restart`, updater, opener, window/resource/plugin commands | OS/plugin/process shims, not relay mutations. |
| Pairing and identity-recovery UI commands | Native pairing flow; relay interaction is outside this mocked command state. |
| `create_save_subscription`, `delete_save_subscription`, `merge_save_subscription_kinds`, `remove_save_subscription_kind`, `archive_events` | **Confirmed local-only against Rust.** `archive/mod.rs`: `create_save_subscription` probes access then upserts SQLite; `archive_events` queries the relay then persists to the local archive DB — neither command publishes a mutation event. Bridge mock state matches that boundary. |
| Sleep prevention, clipboard/download/save, and media picker/upload shims | OS/filesystem/native media shims; separate message commands publish relay events. |

## Rust confirmation (#144 / #172)

Checked against:

| Area | Rust source | Result |
|---|---|---|
| User-input answer | `desktop/src-tauri/src/commands/user_input.rs` | Durable kind 46041 via `build_agent_user_input_answer` + `submit_event`. Bridge now mirrors. |
| Persona publish | `desktop/src-tauri/src/commands/personas/sharing.rs` | Kind 30175 via `submit_signed_event_at_with_keys`. Bridge now mirrors. |
| Identity archive | `desktop/src-tauri/src/commands/identity_archive.rs` + `events.rs` | Kind 9035/9036. Bridge now mirrors. |
| Managed agent message | `desktop/src-tauri/src/commands/messages.rs` | Kind 9 signed as agent + optional `x-auth-tag`. Bridge now mirrors when `privateKeyHex` is seeded. |
| Workflows | `desktop/src-tauri/src/commands/workflows.rs` + `events.rs` | **Fixed in #172.** Kind **30620** definition (`d`+`h`, YAML content), kind **5** delete (`a=30620:owner:id`), kind **46020** trigger (`d`). No kind **30625** in Rust (issue title hypothesis only). Bridge relay branches mirror; return shapes match Rust (`WorkflowSaveWire` / `{ event_id }` for trigger). Spec: `bridge-relay-mutations.spec.ts`. |
| Local archive | `desktop/src-tauri/src/archive/mod.rs` | **No mutation publish**; SQLite + query. Bridge local-only confirmed. |

## Limits

The card's owner metadata is still injected via
`mock.searchProfiles`: the profile identifies Alice as an agent owned by Tyler.
The evidence message is real relay state, and both Accept/Reject clicks invoke
the real relay-aware bridge reaction path. Publishing a valid
owner-authenticated kind-0 profile for these test identities requires
owner-authentication/signing plumbing that the headless harness does not
currently provide for the evidence card path. This record therefore supports
the narrower claim:
**a headless evidence-card click publishes a real kind-7 to a real relay**.
It does not claim agent-profile provenance from relay state for that card.

For managed-agent channel messages (#144), the bridge **does** compute and
send the owner NIP-OA `x-auth-tag` header (empty conditions, matching the
Rust legacy path). Specs seed `managedAgents[].privateKeyHex` so the agent
can sign; without it, relay mode fails closed with an explicit error.

`send_channel_user_input_answer` in relay mode requires the 46040 request to
be queryable on the real relay (the #144 spec publishes it with Alice's
NIP-OA auth header first).
