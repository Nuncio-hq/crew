# Verification 0011 — E2E bridge relay mutation audit

- **Date:** 2026-08-11
- **Issue:** #133; follow-up #144
- **Branch / commit:** `devin/issue-133-relay-add-reaction` @ `b4f29025c`
- **Plan phase:** relay-backed desktop mutation coverage

## Boundary exercised

The desktop E2E bridge now sends real Nostr mutations when
`getIdentity(config)` returns a relay identity. Relay branches call
`submitSignedEvent`, and do not update the mock message/feed stores or emit a
mock live event. The real WebSocket subscription supplies the relay echo.
Mock mode keeps the existing in-memory behavior.

The single-run evidence path is:

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

## CI coverage

The new relay-backed desktop spec has **no CI coverage today**. The
`integration` project is an upstream-owned lane from `block/buzz`'s
`.github/workflows/ci.yml`, introduced by upstream commit
[`a1c28f487d`](https://github.com/block/buzz/commit/a1c28f487d2af01d620619d940cf377d21c1a81a),
“Shard desktop Playwright CI jobs (#992)”. Upstream's workflow is still
`active`; Crew disabled its inherited `CI` workflow deliberately as part of
the minimised merge gate. See [CI.md](../CI.md), which records that “Upstream
Sync does not run the inherited integration or cross-platform matrices” and
the cutover step to “disable inherited `CI` and `Docker image` in GitHub
Actions”.

In Crew, workflow `CI` is `disabled_manually` (id `323540365`), while the
active `.github/workflows/nuncio-crew-ci.yml` runs the smoke project only.
For PR #146, head `70bfa70e7`, desktop filters were true but no integration
job appeared in
[31448955605](https://github.com/Nuncio-hq/crew/actions/runs/31448955605).
The clean-main push run
[31362178966](https://github.com/Nuncio-hq/crew/actions/runs/31362178966)
also had no integration job. The lane last appeared under the disabled
workflow at
[30534161705/job/90843624305](https://github.com/Nuncio-hq/crew/actions/runs/30534161705/job/90843624305).

The last completed integration run,
[30533242569](https://github.com/Nuncio-hq/crew/actions/runs/30533242569),
used two `ubuntu-latest` shards. Shard 1 took about 6m45s
([job 90840519406](https://github.com/Nuncio-hq/crew/actions/runs/30533242569/job/90840519406));
shard 2 took about 6m17s
([job 90840519509](https://github.com/Nuncio-hq/crew/actions/runs/30533242569/job/90840519509)).
Each shard has a 20-minute timeout. The job requires Postgres, Redis, MinIO
and `minio-init`, a built `buzz-relay`, schema and community setup,
`scripts/setup-desktop-test-data.sh`, Playwright installation, and
`BUZZ_RECONCILE_CHANNELS=true`.

The active `Project Relay` job provisions its own relay stack inside
`scripts/run-nuncio-crew-project-relay-ci.sh`, but jobs do not share
services; that job neither seeds desktop E2E data nor runs Playwright.
Adding this lane would therefore duplicate the per-job relay, database,
schema, seed, browser, and E2E-build setup. It stays with
[#147](https://github.com/Nuncio-hq/crew/issues/147). Of the 17 configured
integration specs, `evidence-reactions-relay.spec.ts` is the only
Crew-specific one; the rest are inherited Buzz coverage. Every relay-backed
desktop spec is therefore locally verified only.

### How to run this spec locally

```bash
. ./bin/activate-hermit
docker compose up -d postgres redis minio minio-init
export BUZZ_RECONCILE_CHANNELS=true
just relay
bash scripts/setup-desktop-test-data.sh
pnpm --filter buzz build:e2e
cd desktop
pnpm exec playwright test evidence-reactions-relay.spec.ts --project=integration
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

The new spec is
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

## Verification commands

The E2E bundle was rebuilt after the bridge style edits:

```bash
CI=true . ./bin/activate-hermit && CI=true pnpm --filter buzz build:e2e
```

The relevant successful output was:

```text
Scope: all 4 workspace projects
Lockfile is up to date, resolution step is skipped
Done in 1.1s using pnpm v11.4.0
$ tsc && vite build --mode e2e
✓ 4864 modules transformed.
✓ built in 1.76s
```

Desktop checks passed:

```bash
. ./bin/activate-hermit && pnpm --filter buzz check
# Checked 2200 files in 1421ms. No fixes applied.
# $ node ./scripts/check-file-sizes.mjs
# $ node ./scripts/check-px-text.mjs
# $ node ./scripts/check-pubkey-truncation.mjs

. ./bin/activate-hermit && pnpm --filter buzz typecheck
# $ tsc --noEmit
```

The requested smoke command ran 29 tests:

```bash
cd desktop
pnpm exec playwright test --project=smoke \
  evidence-reactions.spec.ts evidence-cards.spec.ts reaction-order.spec.ts \
  reaction-names.spec.ts inbox-reactions.spec.ts custom-emoji.spec.ts \
  empty-edit-delete.spec.ts
```

Its result was:

```text
  ✓  29 [smoke] › tests/e2e/reaction-order.spec.ts:126:1 › a later emoji that accrues more reactors stays to the right of an earlier emoji (4.1s)
  1) [smoke] › tests/e2e/inbox-reactions.spec.ts:36:1 › inbox reaction on a thread-reply mention persists after refetch
  ...
  28 passed (1.1m)
```

The Inbox failure is pre-existing. It was baselined from a separate clean
worktree at `origin/devin/1786360062-evidence-thread-log` (`df2a9995e`):

```bash
git worktree add /tmp/i133-base origin/devin/1786360062-evidence-thread-log
cd /tmp/i133-base
CI=true . ./bin/activate-hermit && CI=true pnpm --filter buzz build:e2e
cd desktop
pnpm exec playwright test inbox-reactions.spec.ts --project=smoke
```

The clean-base run produced the same failure:

```text
  ✘  1 [smoke] › tests/e2e/inbox-reactions.spec.ts:36:1 › inbox reaction on a thread-reply mention persists after refetch (6.1s)

    Error: expect(locator).toBeVisible() failed
    Locator: getByTestId('home-inbox-selected-message').getByLabel('Toggle ❤️ reaction')
    Expected: visible
    Error: element(s) not found
```

The branch failure was therefore not caused by the relay mutation changes.

### Smoke shard-1 baseline

Clean-main run
[31362178966/job/93373095535](https://github.com/Nuncio-hq/crew/actions/runs/31362178966/job/93373095535)
reported these six failures:

```text
6 failed
[smoke] › tests/e2e/channel-activity-popover.spec.ts:274:3 › channel activity hover preview › shows unread channel activity and working agents, then opens the selected thread
[smoke] › tests/e2e/channel-activity-popover.spec.ts:368:3 › channel activity hover preview › removes the dot and preview after the final activity is read
[smoke] › tests/e2e/channel-activity-popover.spec.ts:459:3 › channel activity hover preview › supports row actions and opens an agent's scoped activity
[smoke] › tests/e2e/channel-activity-popover.spec.ts:753:3 › channel activity hover preview › surfaces future replies after the user reacts to a thread root
[smoke] › tests/e2e/channel-agent-presence.spec.ts:100:3 › channel header agent presence › shows needs-you for a 46040 request and opens its real thread
[smoke] › tests/e2e/channels.spec.ts:500:1 › channel question card accepts an answer
```

The PR shard-1 log
[31448955605/job/93649138295](https://github.com/Nuncio-hq/crew/actions/runs/31448955605/job/93649138295)
also reported `channels.spec.ts:1951` and `channels.spec.ts:2108`, but
neither appears in the clean-main failure list above. Both reproduce
identically on the PR #128 parent commit `df2a9995e`, so they are inherited
from that base branch rather than introduced by this change:

```text
Error: expect(locator).toHaveCount failed
Locator: getByTestId('message-timeline-day-group')
Expected: 2
Received: 0
Timeout: 5000ms
```

```text
Error: expect(locator).toContainText failed
Locator: getByTestId('agent-session-thread-panel')
Expected substring: "No ACP activity yet"
Received string: "AaliceActivity · #agents·No updates yet"
Timeout: 5000ms
```

### Smoke shard-3 baseline

The three additional failures from
[PR #146 shard 3](https://github.com/Nuncio-hq/crew/actions/runs/31451647392/job/93657147023)
also reproduce on the PR #128 parent (`df2a9995e`) and on clean
`origin/main` (`35af74019`). They were already present in the earlier PR
shard-3 run
[31448955605/job/93649138303](https://github.com/Nuncio-hq/crew/actions/runs/31448955605/job/93649138303),
so they are upstream-side pre-existing failures, not regressions from the
relay bridge changes:

| Spec | Branch `413afe3fd` | PR #128 parent `df2a9995e` | `origin/main` `35af74019` |
|---|---:|---:|---:|
| `inbox-edit.spec.ts:175` | fail | fail | fail |
| `inbox-edit.spec.ts:325` | fail | fail | fail |
| `messaging.spec.ts:1819` | fail | fail | fail |

The edit assertions consistently showed:

```text
- Attachment reply after editing.
+ 👍❤️😂🎉YYouAug 11, 2026, 2:44 AMAttachment reply before editing.
+ inbox-edit-proof.pdf
```

```text
Expected substring: "My Inbox message after editing."
Received string: "👍❤️😂🎉Nnpub1mock...Aug 11, 2026, 2:44 AMMy Inbox message before editing."
```

The reaction-shaped messaging assertion consistently showed:

```text
Locator: ...getByLabel('Toggle 👍 reaction')
Expected: visible
Error: element(s) not found
```

## Mock-boundary audit

“Relay-aware” means the bridge has a relay branch that publishes the real
event. “Mock-only trap” means relay mode can return a successful-looking result
without reaching the relay. “Local-only” means the bridge-side mutation is
local or an OS/plugin shim. The class-(c) rows marked unconfirmed were inferred
from the bridge side and still need Rust-side confirmation; issue #144 tracks
that confirmation.

### (a) Already relay-aware

| Command(s) | Evidence |
|---|---|
| `update_profile` | `handleUpdateProfile` `desktop/src/testing/e2eBridge.ts:5940-6007`; relay kind-0 submission at `:6006`. |
| `create_channel` | `:6341-6405`; relay kind-9004 submission at `:6404`. |
| `open_dm` | `:6436-6507`; signed relay submission at `:6499`. |
| `hide_dm` | `:6534-6556`; relay submission at `:6552`. |
| `update_channel` | `:6645-6700`; kind-9002 submission at `:6696`. |
| `set_channel_topic`, `set_channel_purpose` | `:6730-6780`; relay submissions at `:6749` and `:6778`. |
| `archive_channel`, `unarchive_channel` | `:6842-6880`; relay submissions at `:6854` and `:6876`. |
| `delete_channel` | `:6886-6907`; relay kind-9008 submission at `:6904`. |
| `add_channel_members` | `:6911-7031`; relay kind-9000 submission at `:7027`. |
| `remove_channel_member` | `:7036-7059`; relay kind-9001 submission at `:7055`. |
| `join_channel` | `:7065-7095`; relay kind-9021 submission at `:7091`. |
| `leave_channel` | `:7098-7121`; relay kind-9022 submission at `:7117`. |
| `send_channel_message` | `:9160-9342`; relay submission at `:9336`, without mock bookkeeping. |
| `edit_message` | `:9459-9505`; relay kind-40003 submission at `:9503`. |

### (b) Mock-only traps

| Command(s) | Status and evidence |
|---|---|
| `delete_message` | **Fixed in this change.** Former mock-only handler `:9425-9455`; relay now publishes the Rust-compatible kind-5 `h`+`e` event. |
| `add_reaction` | **Fixed in this change.** Former mock-only handler `:9528-9576`; relay now publishes exact kind-7 content/tags. |
| `remove_reaction` | **Fixed in this change.** Former mock-only handler `:9579-9614`; relay now queries the caller's own kind-7 and deletes it with kind 5. |
| `set_canvas` | **Fixed in this change.** Dispatcher `:13363-13364` formerly returned a synthetic ID; relay now publishes kind 40100 with `h`. |
| `send_managed_agent_channel_message` | **Not fixed; #144.** `:9351-9420` uses mock agent/store state and emits a mock kind-9. The real event needs an agent identity/signing decision. |
| `send_channel_user_input_answer` | **Not fixed; #144.** Dispatcher `:13023-13028` returns a synthetic accepted response; the Rust command has a durable relay event. |
| `update_persona_and_publish` | **Not fixed; #144.** Dispatcher `:12278-12282` reaches mock persona catalog state; the Rust operation publishes persona catalog kind 30175. |
| `archive_identity`, `unarchive_identity` | **Not fixed; #144.** Dispatcher `:13358-13360` is a UI-only stub; the Rust commands publish NIP-IA archival events. |

### (c) Legitimately local-only or pending confirmation

| Command(s) | Bridge-side classification |
|---|---|
| `create_persona`, `update_persona`, `delete_persona`, `set_persona_active`, `set_persona_shared` | Local persona/catalog persistence; `update_persona` queues local pending state (`:8090-8096`). Rust-side confirmation remains tracked by #144. |
| `create_channel_template` | E2E-only local template fixture state. |
| `create_team`, `update_team`, `delete_team`, `install_team_from_directory`, `sync_team_directory`, `confirm_*_snapshot_import` | Local persona/team files and SQLite/import workflows; Rust-side confirmation remains tracked by #144. |
| Managed-agent create/update/start/stop/delete/runtime-pair commands | Local records, process lifecycle, and runtime bookkeeping; Rust-side confirmation remains tracked by #144. |
| `create_hermes_profile`, `delete_hermes_profile` | Local Hermes profile-manager state. |
| `save_custom_harness`, `delete_custom_harness`, `connect_acp_runtime`, `install_acp_runtime` | Local ACP configuration/process installation. |
| `plugin:process|restart`, updater, opener, window/resource/plugin commands | OS/plugin/process shims, not relay mutations. |
| Pairing and identity-recovery UI commands | Native pairing flow; relay interaction is outside this mocked command state. |
| `create_workflow`, `update_workflow`, `delete_workflow`, `trigger_workflow` | Bridge-local workflow/run records. Whether durable workflow state has a Rust-side relay mutation is unconfirmed; #144 tracks it. |
| `create_save_subscription`, `delete_save_subscription`, `merge_save_subscription_kinds`, `remove_save_subscription_kind`, `archive_events` | Bridge-local save-subscription/archive state; Rust-side confirmation is unconfirmed and tracked by #144. |
| Sleep prevention, clipboard/download/save, and media picker/upload shims | OS/filesystem/native media shims; separate message commands publish relay events. |

## Limits

The card's owner metadata is still injected via
`mock.searchProfiles`: the profile identifies Alice as an agent owned by Tyler.
The evidence message is real relay state, and both Accept/Reject clicks invoke
the real relay-aware bridge reaction path. Publishing a valid
owner-authenticated kind-0 profile for these test identities requires
owner-authentication/signing plumbing that the headless harness does not
currently provide. This record therefore supports the narrower claim:
**a headless evidence-card click publishes a real kind-7 to a real relay**.
It does not claim agent-profile provenance from relay state.

The relay remained running after verification at `http://localhost:3000`.
