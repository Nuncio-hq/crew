# Testing Strategy

## Purpose

Tests are design instruments. They should reveal incorrect assumptions and
edge cases before implementation commits the architecture.

**Client acceptance is a separate bar.** Technical green (`just ci`, e2e)
provides scoped evidence. Founder Accept requires Gate C (story + try script +
evidence + honest limit in the thread) — see issue #234 / D-070 and
[`templates/CLIENT-ACCEPTANCE.md`](templates/CLIENT-ACCEPTANCE.md). Do not
treat this file’s technical checklist as founder Accept.

For CompanyOS's first coding delivery loop (D-076), the responsible lead
must connect acceptance criteria to evidence from the delivered result.
Verification skills should specify the setup, action, observable pass/fail
criteria, recovery from failure, and evidence to return. Exercise the actual
user workflow where relevant; use comparable measurements for performance
claims. A mock-only check must be labeled mock-only. Non-code work uses checks
appropriate to its sources and requirements. Screenshots are useful for visual
claims, not a universal proof of correctness.

## Test environments and real-data staging (D-077)

The founder's daily relay is hosted on `oscar-dev-server`, reachable from
the Mac over Tailscale. It is live data even though the host is named
dev-server. Deployment details and current verification are in
[`ARCHITECTURE.md`](ARCHITECTURE.md) and [`STATE.md`](STATE.md).

**Agreed topology, not yet provisioned:** run a separate staging stack on
the same dev-server; connect the local Crew build and test agents to it over
Tailscale. Staging must have its own relay endpoint, PostgreSQL, Redis,
media storage, Git storage where needed, volumes, and network configuration.
Set CPU/RAM limits so load tests do not starve the daily relay. Record the
verified staging endpoint here when provisioned; do not guess one or fall
back to the daily relay if staging is unavailable.

Choose the environment for the check:

- Unit tests and mock E2E use small deterministic fixtures. Isolated local
  relay tests remain valid when real account history is unnecessary.
- Real-data workflows and bounded performance tests use staging restored
  from a snapshot, not the daily database.
- Checks on the daily relay are narrow and read-only by default. Any write
  test needs explicit authorization for that live scope; a general request
  to test a feature does not authorize live writes, resets, or migrations.

### Snapshot and reset policy

Take a consistent PostgreSQL snapshot (for example, `pg_dump`) and capture
the relevant media/Git data separately. Record snapshot time, source relay
revision/configuration and database schema version, plus any missing assets.
Restore into isolated staging, applying branch schema changes only there.
Preserve signed Nostr events; do not rewrite their content, pubkeys, or tags
in SQL to remap identities or URLs. Configure staging routing and access
separately, and verify media links do not silently target writable live storage.

Use one fixed baseline during a reproduce → fix → retest cycle. Reset staging
from that baseline as needed; refresh from the daily system only when a new
baseline is deliberately requested. Never synchronize test writes back to
the daily system. Keep snapshots outside Git and task/PR attachments.

### Agent and desktop configuration

Start from the installed NuncioCrew configuration, not a blank agent setup
or a stock Buzz directory. The observed store is
`~/Library/Application Support/com.nuncio.crew/agents/`; verify the active
app-data path before copying. Preserve relevant personas, model/provider,
timeouts and role settings, then make a separate test configuration:

- Set the relay destination for the desktop **and every test agent**;
  imported records may retain older relay URLs.
- Use separate desktop app-data, Hermes test profiles, writable workspaces
  and runtime session state. Copy required memory/skills by value rather
  than sharing the founder's live writable profile.
- Exclude PIDs and active-session state; disable imported autostart,
  schedules and external hooks until explicitly enabled for the test.
- Configure staging identity/membership deliberately. Do not blindly copy
  live auth tokens, private keys, or connector credentials. Enable only the
  tool access needed; a staging relay does not sandbox shell, email or GitHub.

Before launching agents or a write test, verify the effective relay URL,
database/storage destination, workspace and enabled external tools. Record
these non-secret destinations and the snapshot identifier with results in
the task/PR. Stop on a live destination mismatch.

### Existing harness limitations

[`docker-compose.harness.yml`](../../docker-compose.harness.yml) is an
existing isolated fixture stack, not the agreed snapshot staging deployment.
[`start-isolated-test-relay.sh`](../../scripts/start-isolated-test-relay.sh)
drops the isolated schema and seeds fixtures on every launch; it also uses
relaxed development auth. Add and verify snapshot-aware setup before using
it for real-data staging. Do not point its reset path at the daily database.
The default Compose file uses fixed container/volume/network names, so
changing only the Compose project name does not isolate that stack.
`buzz-adopt-prod-agents.sh` targets stock Buzz paths and copies identity;
it is not a ready-made NuncioCrew staging importer.

## TDD loop

For each observable contract:

1. Write one focused test.
2. Run it and confirm the intended RED.
3. Add design-changing edge cases.
4. Implement the smallest behavior.
5. Run to GREEN.
6. Refactor while keeping the suite green.
7. Run affected integration and end-to-end suites.

Never claim TDD when tests were written after implementation without first
demonstrating that they detect the missing or broken behavior.

## Isolated staging tooling

`just crew-tooling-test` runs the bounded CLI regression suite. The same
command is required by `just check` and the NuncioCrew CI Policy job. These
tests replace external commands; they do not prove a running staging relay or
a desktop workflow.

Copy `scripts/crew-staging.example.json` outside Git and replace every placeholder
from verified resource inventory; the example intentionally cannot run as-is.
The server operator supplies this private version-1 manifest to
`scripts/crew-staging.py`. It records the source container IDs, source schema,
community and asset roots; separate target ports, volume names, network and
container IDs; pinned images and build SHA; migration checksums; resource
budgets; and private credential **paths**. Keep that manifest and all captured
data outside Git. The CLI rejects shared source/target destinations.

```sh
python3 scripts/crew-staging.py plan --config /absolute/private/staging.json
python3 scripts/crew-staging.py snapshot --config /absolute/private/staging.json --baseline initial
python3 scripts/crew-staging.py restore --config /absolute/private/staging.json --baseline initial
python3 scripts/crew-staging.py verify --config /absolute/private/staging.json --baseline initial
```

Provisioning is a separate reviewed operation. `restore` requires already
recorded resource identities; it does not discover a container by name and
assume ownership. Coordinate the PostgreSQL writer before restoring. A separate
fixture database may share the owned PostgreSQL service, but cannot connect to
the snapshot database. Stop/start and restore commands must never target the
daily environment.

A baseline uses a PostgreSQL dump transaction and unchanged pre/post asset,
community, sanitized-user and signed-event sentinels. This is **not a globally
atomic cross-store snapshot**. A changed sentinel leaves an incomplete baseline
that cannot be restored. Capture a new baseline ID after inspection; never
silently refresh an existing ID. Scheduled reminders and expiring channels
currently cause refusal. Credential and executable table data are excluded;
public profile data is retained except `users.okta_user_id`. Signed Nostr event
fields are preserved and compared after restore. Git archives contain the
`repos/` directory and are extracted into the owned Git volume root.

Restore first records unavailable state and stops the target relay, then
recreates only the declared snapshot database, applies hashed migrations, and
checks signed events and persisted assets before starting services. Verification
requires readiness plus a new private-channel message and exact CLI readback.
`SERVER_VERIFIED` deliberately leaves desktop acceptance unverified: capture the
actual isolated app workflow separately and post evidence with
`scripts/post-screenshots.sh`.

The Mac profile export accepts a separately approved ownership document through
`scripts/crew_staging_profiles.py`. Its output is inert runtime/model/provider
choices. It grants no generation permission and copies no employee identities,
credentials, prompts, sessions or environment variables. Native account reuse
requires a separately verified auth reference and refresh-write boundary.

## Test layers

| Layer             | What it proves                                 | Typical Crew use                                  |
| ----------------- | ---------------------------------------------- | ------------------------------------------------- |
| Pure unit         | Deterministic policy and projection            | Event parsing, role checks, retry policy          |
| Component         | React behavior around a stable contract        | Thread detail, input requests, path selection     |
| Relay integration | Signed-event storage and subscription behavior | Thread history, repository metadata               |
| ACP contract      | Provider and tool behavior                     | Context, permissions, workspace paths, resume     |
| Desktop E2E       | User-visible workflow in Tauri mock bridge     | Thread navigation, Need you, live-job desk         |
| Live smoke        | Real local relay and installed providers       | Final compatibility confidence                    |

Use the lowest layer that proves the contract, then add boundary tests where
cross-component behavior is the risk.

## Work coordination edge-case checklist

For the affected coordination seam, test these against its agreed contract.
The old board and fixed three-card limit were superseded by D-037; D-076
does not introduce replacement queue semantics:

- runtime capacity limits are respected without losing queued work;
- waiting for input follows the agreed resource-release policy;
- work requiring a founder decision remains discoverable;
- resolved input returns to the correct next state;
- duplicate transition events do not double-count capacity;
- out-of-order delivery converges to the same state;
- concurrent clients attempting the final slot resolve deterministically;
- reopening a `Done` card follows an explicit transition;
- missing or deleted Project references render safely;
- reconnect and cold start reconstruct the same work state;
- optimistic UI rolls back when relay publication fails.

## Project-location syntax and relay checklist

The current no-Rust slice tests:

- moved and deleted paths do not alter Project identity;
- multiple location tags coexist;
- existing clone metadata remains unchanged;
- path with spaces and non-ASCII characters round-trips;
- a local path is not sent to an unintended relay;
- Windows path forms are either supported or rejected clearly.

Spike 0006 and the exact-reader implementation now exercise directory
existence, Git checkout detection, native containment, path mismatch, and
symlink fail-closed behavior through Buzz's existing native snapshot command.
Permission-denied and missing paths share the truthful `Local unavailable`
state; they do not fall back to another repository.

## Folder-first Add Project contract

Run the focused policy, relay orchestration, and UI integration contracts:

```text
cd desktop
node --import ./test-loader.mjs --experimental-strip-types \
  --test src/features/projects/project-add-local-workspace-*.test.mjs
```

The contracts require:

- folder basename becomes the default Project name;
- spaces and Unicode remain unchanged in the location tag;
- invalid or relative paths fail before a channel or relay write;
- duplicate `(owner, d)` fails before channel creation;
- failed publication exposes a full `(owner, d)` channel retry token;
- an ACKed event recovered by exact read-back completes the retry;
- malformed Crew location/channel metadata fails closed;
- a linked path never binds to a same-named checkout under Buzz's repos root;
- duplicate preflight uses an exact relay coordinate query;
- no `clone` tag is fabricated;
- local path and canonical Project channel survive the read model;
- a local-only Project cannot fall through to clone/terminal actions;
- empty and populated Projects views use the same Repository callback;
- the standalone Local workspace strip is absent.

Manual native smoke must also confirm picker cancel causes no visible channel
or Project, because the Node contract runner cannot drive the macOS directory
dialog.

## Exact local workspace reader contract

Run the exact resolver together with the Add Project integration contracts:

```text
cd desktop
node --import ./test-loader.mjs --experimental-strip-types \
  --test src/features/projects/project-exact-local-workspace-contract.test.mjs \
  src/features/projects/project-add-local-workspace-*.test.mjs
```

The contracts require:

- folder basename, not Project `d`, selects the repository;
- spaces and Unicode remain addressable;
- the native returned path must match the selected path;
- null, error, mismatch, and non-Git results remain unavailable;
- linked paths never fall back to configured or remote repositories;
- ordinary Buzz checkouts retain clone-origin matching;
- source labels distinguish checking, ready, unavailable, and missing;
- linked Projects expose no clone, fetch, sync, Terminal, or configured-root
  commit-diff path;
- linked or invalid Projects cannot merge a pull request through a retained
  clone location.

## Provider matrix

Workspace behavior must be checked independently for:

- Codex through its Buzz ACP adapter and `buzz-dev-mcp`;
- Claude Code through its ACP adapter;
- Cursor through native `cursor-agent acp`;
- Devin through native `devin acp` when Devin is in scope.

Record:

- provider and adapter versions;
- authentication class, without secrets;
- `session/new.cwd`;
- Project path;
- permission mode;
- tools actually used;
- created filesystem evidence;
- warnings that did not affect the result;
- cleanup.

Do not infer one provider's sandbox or permission behavior from another.

## Event-model checklist

For new relay event schemas, test:

- signing and verification;
- required and optional tags;
- unknown extra tags;
- parameterized replacement behavior, if used;
- channel scoping and membership;
- duplicate publication;
- replay;
- timestamp ties and conflict resolution;
- cold reconstruction;
- compatibility with clients that ignore Crew tags;
- kind collision against current Buzz and relevant NIPs.

## Test integrity

Forbidden shortcuts:

- deleting a failing test without replacing its contract;
- broadening an assertion until incorrect behavior passes;
- mocking the exact boundary the spike or test is meant to verify;
- swallowing provider or relay errors;
- relying on sleeps when deterministic synchronization is available;
- reporting a sandbox/environment failure as a product pass;
- reporting a targeted pass as if the full repository were green.

Every completion report must distinguish:

- focused tests run;
- broader suites run;
- suites not run;
- environment blockers;
- remaining risk.

## Release contract

Run the local flavor and manual release contracts:

```text
cd desktop
node --import ./test-loader.mjs --experimental-strip-types \
  --test src/testing/nuncio-crew-local-build-contract.test.mjs \
  src/testing/nuncio-crew-release-contract.test.mjs
```

They require:

- local builds visibly say `Local`, retain the Buzz local identifier, and
  cannot inherit updater configuration;
- the Nuncio release workflow has only `workflow_dispatch`;
- release inputs use an exact tag, channel, and 40-character commit on `main`;
- dev and stable version formats fail closed;
- stable users never read a dev manifest;
- stable publication advances both stable and dev manifests;
- distributed builds use the Nuncio identifier and Nuncio GitHub URLs;
- the Tauri updater manifest contains only the signed Apple Silicon artifact;
- committed Buzz manifests remain pinned to the recorded upstream version.

Static contracts do not prove Apple signing, notarization, GitHub publication,
installation, or updating. Those require the real manual workflow and the
end-to-end checklist in [`RELEASING.md`](RELEASING.md).

## CI contract

Run the additive Crew workflow and final-gate policy contracts:

```text
node --test desktop/src/testing/nuncio-crew-ci-contract.test.mjs
```

They require:

- one stable `NuncioCrew Gate`;
- automatic work limited to desktop, macOS ARM64, and relevant Project relay
  behavior;
- failed, cancelled, or missing dependencies to block the gate;
- deliberately irrelevant conditional jobs to be accepted as skipped;
- no signing credentials or publication permissions in PR CI;
- heavyweight upstream compatibility to remain manual-only.

The required Project Relay job also runs `just wiki-contract`, which exercises
the native snapshot builder/verifier and the relay's pure conditional
publication, Wiki validation, and repository-state contracts. The PostgreSQL
lane discovers the `conditional_publication_postgres_tests` family, including
the deletion-versus-decision tests. These are separate from the older
Project-link fixture tests excluded from that lane.

## Project local workspace verification

The normal desktop suite keeps the real-relay test skipped:

```text
cd desktop
pnpm test
```

With an isolated local Buzz relay already running, execute the boundary test
explicitly:

```text
cd desktop
CREW_LIVE_RELAY_URL=ws://127.0.0.1:3000 \
  node --import ./test-loader.mjs --experimental-strip-types \
  --test src/features/projects/project-local-workspace-live-relay.test.mjs
```

The live test uses a generated ephemeral keypair and unique `d` tag. It
publishes a kind `30617`, links one path, reconnects for a cold read, relinks a
Unicode path, and resolves the latest path into Project-channel agent context.
Never point this test at a shared or production relay.

## Desktop channel-membership projection (#337)

The Desktop membership badge consumes the ACP `channel_membership` observer
signal through the production live observer ingress. Its projection is bounded
per normalized agent and generation, orders frames by sequence, and keeps a
generation's start timestamp immutable. The reader requires the native runtime
row for the same agent and canonical community relay, the exact `startNonce`, a
connected transport, and a runtime that is not retired. Missing, invalid, stale,
closed-watch, or transport-error input remains `unknown`; only an explicit
current count of zero displays “No channels.” Both zero and unknown preserve
the existing Add/Restart controls. Retired-generation replay fences are
bounded and recyclable, so a valid fresh native nonce still recovers after
long-running restart churn.

Run the focused production-ingress suite with:

```text
cd desktop
node --import ./test-loader.mjs --experimental-strip-types \
  --test src/features/agents/lib/channelMembershipState.test.mjs
```

The suite covers unknown → zero → nonzero recovery, same-count generation
notifications, sequence and start-timestamp fences, stale and evicted
generations, community reset, normalized agent keys, and disconnected or
retired runtime status. It calls `_testProcessLiveObserverEvents`, which is
bound to `processLiveObserverEvents` in `observerRelayStore.ts`; direct calls
to `applyCrewLiveFrameSideEffects` are not evidence of the live path.

The corresponding ACP subscription tests exercise the startup FIFO barrier,
membership-watch rejection/replay, parked channel retries, and stale CLOSED
after removal through the actual background command and WebSocket handlers:

```text
cargo test -p buzz-acp --lib channel_membership_signal
cargo test -p buzz-acp --lib relay::subscription_recovery_tests
```

The mock membership case lives in `agent-availability.spec.ts`, registered in
the `integration` Playwright project but explicitly using `installMockBridge`.
It verifies badge transitions, frame-before-runtime ordering, native transport
and community fences, and keyboard access to the existing recovery controls:

```text
cd desktop
pnpm build:e2e
BUZZ_E2E_PORT=4337 pnpm exec playwright test --project=integration \
  tests/e2e/agent-availability.spec.ts --grep 'membership stays unknown'
```

The native status constructor exposes the active generation's `startNonce` in
`ManagedAgentRuntimeStatus`. Its focused Tauri test is
`managed_agents::runtime_commands::tests::status_construction_exposes_the_active_runtime_start_nonce`;
run it only with the sidecar prerequisite and an available Rust build slot:

```text
just _ensure-sidecar-stubs
cargo test --manifest-path desktop/src-tauri/Cargo.toml \
  managed_agents::runtime_commands::tests::status_construction_exposes_the_active_runtime_start_nonce
```

These fixture and mock-bridge checks do not establish installed Hermes staging,
relay health, or receipt acceptance. Those remain separate #338/#348 gates.

## Wiki Ask unavailable placeholder

The production `WikiAskBox` stays visible while Ask is unavailable. Its focused
regression suite proves the accessible unavailable status, editable question and
mode controls, and no answer, draft write, or channel navigation through pointer,
keyboard, or IME submission paths:

```text
cd desktop
node --import ./test-loader.mjs --experimental-strip-types --test \
  src/features/wiki/ui/WikiAskBox.unavailable.test.mjs
```

This is a fail-closed UI guard; #366/#367's private runtime, history, durable
draft, and explicit dispatch evidence remain pending.

## Wiki Protocol B live acceptance

The independent Wiki protocol harness is also skipped by the normal desktop
suite. Run it only with an explicitly disposable relay origin, owner key, and
repository identifier. The relay must advertise and enable
`crew-conditional-publication-v1`:

```text
cd desktop
CREW_PROTOCOL_B_ORIGIN=ws://127.0.0.1:3000 \
CREW_PROTOCOL_B_OWNER_SECRET_KEY=<disposable-32-byte-lowercase-hex> \
CREW_PROTOCOL_B_REPO_D=crew-protocol-b-<unique-suffix> \
node --import ./test-loader.mjs --experimental-strip-types \
  --test src/features/wiki/wiki-protocol-b-live-relay.test.mjs
```

`CREW_PROTOCOL_B_OWNER` is optional; when supplied, it must match the public
key derived from the disposable secret. For the cross-client acceptance,
first generate through native A, then set
`CREW_PROTOCOL_B_EXPECTED_NATIVE_HEAD_ID` to its verified live `_toc` event ID.
B must cold-read that exact graph before making any Wiki write. Omitting this
variable selects a self-contained protocol fixture: B creates its own repository
anchor and signed graph, which does not establish native-A interoperability.

After the initial read, B publishes two immutable dependency sets and races
two `_toc` heads with the same expected revision. It requires exactly one CAS
winner, one conflict, an exact replay of the winner, and rejection of the
losing head replay. Reads verify the signed IDs and canonical digests, then
reread the head to fence the graph to one revision. The harness bounds each
signed event to 192 KiB, each query response to 1 MiB, the descriptor to 64 KiB,
and the full graph to 64 MiB and 256 pages; page queries use batches of four.
This is an independent protocol client, not a second native app. Native
restart/recovery proof remains a separate acceptance requirement. Never point
the harness at a shared or production relay; it intentionally leaves its
disposable repository data in place.

## Wiki journal v3 recovery

Cancellation acceptance must cover an uncertain head send followed by Cancel,
process restart and read-only Reconcile. None may implicitly submit again.
Explicit Resume publication must either confirm a late matching head without
sending, or reuse the exact saved signed graph and original CAS precondition
after a durable resume transition. A failed transition must send nothing; a
changed live head must resolve as superseded. Verify that typed immutable
retirement remains Regenerate-only: Cancel and Resume must preserve its typed
proof and unresolved claim. Reconciled cancellation stays terminal.

Schema v3 and permanent-dependency Regenerate are accepted designs whose runtime
evidence remains pending in #362. Release acceptance must exercise both
forward migrations from an existing v1 journal, preserving all owners,
communities, payloads, revisions and claims. Reopening v3 must be idempotent;
an older reader must refuse it visibly. Injected migration, quota and commit
failures must preserve the exact predecessor. Test every trim/removal path with
a pinned direct predecessor, including A→B→C and unrelated scoped operations.

Exercise permanent dependency refusal before and after a TOC attempt, and both
orders of an already-admitted old send versus the new CAS. An old send winning
first must leave a working recovery action. Generic refusal from an older
relay, a missing query result and transport failure must never become deletion
proof. A new UUID must produce new immutable addresses even for unchanged source.

Head and precondition retirement (accepted design; implementation and runtime
evidence pending in #362) extends the same matrix. The native guarded-read
fences are covered separately from the proof decision: identity generation,
durable revision and worker lease are each moved against a real captured
scope, a real journal row and a real transport, before the first request and
while each of the two reads is held, so a pre-send refusal and a post-response
refusal are independently observable. Native A restart and independent signed
protocol B acceptance remain pending and separately allocated. On real PostgreSQL: an
accepted-then-deleted head must classify as retired while its exact live replay
still ACKs as a duplicate first; an accepted-then-deleted non-absent
`expected-revision` with no live head must classify as a retired precondition;
an unknown expected revision, one retired only at another coordinate, and a
restored live head must all keep the generic classification; a never-accepted
head under `expected-revision: absent` must stay admissible, proving absence is
not retirement. Refused writes must insert nothing. Natively: each malformed,
foreign, generic, non-400, failed-query and still-live condition must stay
Unknown with a retryable claim; a validated proof must settle `Superseded` and
reconciled in one guarded CAS, send nothing afterwards, survive journal reopen,
release the resource claim for a fresh Generate, and preserve the signed graph,
progress and head-attempt metadata. A conflict reconciliation must carry any
existing typed dependency retirement into the new proof, visible through both
the durable record and the public job projection. Reconcile and automatic
restart must still send nothing.

After migration, binary rollback alone is unsupported. The forward-recovery
procedure is to quit every app instance using this journal, preserve its full
directory, verify a v3-capable build and reopen the same journal. Set the paths
below from the exact affected app's native app-data directory and verified app
bundle; never substitute another staging profile or an old database snapshot.

```sh
: "${CREW_APP_DATA:?Set the verified affected app-data directory}"
: "${CREW_VERIFIED_V3_APP:?Set the verified v3-capable app bundle}"
umask 077
CREW_RECOVERY_DB="$CREW_APP_DATA/owner-operations/recovery.db"
CREW_RECOVERY_COPY="$(mktemp -d "${TMPDIR:-/tmp}/crew-recovery-v3.XXXXXX")"
ditto "$CREW_APP_DATA/owner-operations" "$CREW_RECOVERY_COPY/owner-operations"
sqlite3 -readonly "$CREW_RECOVERY_DB" 'PRAGMA user_version; PRAGMA integrity_check;'
open "$CREW_VERIFIED_V3_APP"
```

Run those commands only after app shutdown, with `CREW_APP_DATA` and
`CREW_VERIFIED_V3_APP` already set to verified absolute paths. Preserve any
SQLite sidecar in the directory copy. Expect schema `3` and integrity `ok`;
otherwise retain the files and investigate with a compatible forward fix.
Reconcile or retry the durable job through the app and confirm its exact signed
IDs against a fresh protocol-client read. Do not delete the journal, reset its
schema version or restore a v1 copy over new operations. No downgrade converter
is provided. The stopped-app copy, compatible reopen and recovery action must
be demonstrated in the isolated acceptance environment before release.

To validate a graph produced by the native A worker, set
`CREW_PROTOCOL_B_EXPECTED_NATIVE_HEAD_ID` to the expected 64-character head ID.
In this mode the harness publishes no initial Wiki fixture: it reconnects,
reads that existing repository anchor and complete graph, then races two new
heads against the exact native revision. Self-contained mode publishes its
disposable kind `30617` repository anchor before the Wiki graph. Every read
enforces the 192 KiB event, 256-page, four-event/1 MiB query, and stable-head
limits.

## ACP transport recovery (#338)

`cargo test -p buzz-acp --lib auth_correlation_tests` drives the actual
`do_connect` handshake using ephemeral loopback WebSockets. It covers unrelated
positive/negative OK frames before and after the challenge, exact AUTH denial,
retryable `error:` dependency denial, AUTH-close, and buffered generic/channel
CLOSED frames. Unrelated acknowledgements must neither authenticate an attempt
nor reject credentials; only the exact sent AUTH ID can settle the handshake.
Existing subscription handling consumes the buffered frames afterward.

The managed retry contract is exercised through the production startup,
autonomous reconnect, wait, handshake-buffer and subscription-replay seams in
`transport-health-tests.rs`. Run `cargo test -p buzz-acp --lib relay::` for the
combined suite. The same health episode covers six attempts and 300 seconds,
including DNS, connection time, subscription replay and the final command drain.
Slow probes use a fresh 270–330-second delay and a fresh 300-second bounded
connect/replay/drain window; only complete recovery resets health. A deadline
interrupting a command retains its exact signed observer event and deferred
subscription intent for the existing replay/ACK path.

Mode scope follows the #338 coordinator interpretation: both
`CREW_ACP_TRANSPORT_STATUS_PATH` and `CREW_ACP_TRANSPORT_START_NONCE` missing means
legacy retry behavior and no status I/O; a complete valid native pair enables
managed policy; partial or invalid configuration fails explicitly. This is not
a new founder product decision. Desktop reserves both keys and supplies them
after custom environment entries; setup/adopted processes without a native
nonce remain unknown. On platforms without the secure Unix storage backend,
Desktop leaves both keys absent so the existing harness start remains usable;
transport projection stays unknown.

`cargo test -p buzz-core transport_status --lib` covers generation, monotonic
sequence, freshness and final-record rules. ACP status tests cover atomic 0600
writes, fixed safe diagnostics, renewal, startup failure flushing and a real
staging-write failure that leaves the old connected file readable: its lease
must expire to unknown, and renewal must later publish the retained new state.
`cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib
managed_agents::transport_status` covers secure reads, owner/nonce/epoch fences,
late completion after leave or shutdown, and bounded retired diagnostics.

The per-pair transport directory has a hard cap of 192 entries plus its empty
shared lock file. Native preflight runs before spawn, reserves all three
per-generation names, and performs bounded flat-directory hygiene under the
same lock as ACP writes. It preserves the required previous generation across
failed startup and one recent historical record (24-hour age target). Valid
private single-link records with exact pair/nonce binding may be removed even
when nonterminal; that grants no live-status authority. Unknown/malformed files
are retained and counted; unsafe or saturated storage fails visibly before
spawn. Every write rechecks capacity, including an old writer recreating an
unlinked record. Repeated replacement, failed replacement, shared-lock,
symlink/hardlink/FIFO/mode/size, and saturation cases bind these helpers.

The Members menu keeps process and connection status separate. A live failed
transport retains Stop Agent alongside Retry by restarting; a failed previous
process is labeled Last process. A failed retry must leave a recovery action
available. The `channels.spec.ts` mock-bridge case binds the rendered
transport status and retry click to the same pair's stop/start commands, while
the `community-rail.spec.ts` leave case binds removal to the native eligibility
invalidation command. These cases cannot prove native process behavior,
installed staging, or in-flight turn/receipt acceptance. Those remain #338
gates; real staging requires #348.

## CompanyOS grouped evidence (#344)

For the #349 shell, `companyos-shell-navigation.spec.ts` checks stable Projects
and Workflows despite saved preview opt-outs, workspace-menu navigation,
Company Wiki, Settings, and Project expansion versus row navigation. Existing
channel navigation specs use `openWorkspaceChannel` to exercise Browse
channels through the real menu. These mock-bridge cases establish UI routing,
not installed runtime or Project-operation recovery acceptance.

The #350 role editor unit lane binds reopen, durable discard and recovery
selection to the native seams. It proves that a discarded reconciled
superseded journal is absent after reopen, unrelated owner-operation rows stay
intact, and an unresolved or partial row wins deterministically over older
superseded history. Removal failures retain the draft and recovery action; the
registered `companyos-channel-roles.spec.ts` covers the corresponding
review/replacement UI path.

For #361, run the native `project_git_workspace_probe` tests separately from
the root Rust workspace. The probe must preserve the exact selected path,
distinguish a Git root from a subdirectory, and propagate inaccessible-path or
probe failures. Its Git subprocesses share one bounded deadline. Relay atomic
creation/recovery tests must use isolated owned databases and an explicitly
enabled relay; D-080's concurrent-writer and deployment checks remain separate
from these local probe tests.

The native Project workspace journal has a focused unit lane:

```text
cargo test -j 1 --locked --manifest-path desktop/src-tauri/Cargo.toml \
  -p buzz-desktop --lib project_change_link -- --test-threads=1
```

It covers the durable v1 channel link, v2 repository attachment, v3 unlink,
and v4 exact workspace path link. The shared driver cases cover lost-ack retry
identity; v4-specific cases cover tag-order/path preservation and pre-persist
intent validation. This lane does not prove the relay's conditional capability
advertisement or installed picker behavior.

The managed-agent deletion coordinator has a focused native lane:

```text
cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml \
  -p buzz-desktop --lib managed_agent_delete -- --test-threads=1
```

It binds the production deletion journal's payload validator, channel cleanup
operation UUIDs, exact-record fence, key/tombstone ordering, UTF-8 error cap,
review-state recovery rules, global claim/list behavior, and renderer-terminal
mutation rejection. A passing local lane does not prove installed process
termination or relay readback; those remain runtime acceptance gates.

Related issues may reuse one unchanged build and owned real-data run; keep an
explicit issue-to-case mapping and post evidence on each corresponding issue.
Record candidate SHA/build, source revision, runtime/model/profile, test data
identity, handler effects/readback and recovery. Preserve active employee
sessions and private data. Stage 0 reference screenshots are explicitly
simulated; final implementation evidence launches the real candidate flow.
Use `scripts/post-screenshots.sh` on the resulting PR and copy its immutable
image URLs with concise captions into the issue. Required final CI, independent
exact-head review and installed release acceptance remain separate gates.

## Recap capability source proof (#351)

The default-off native recap slice is tested through its production modules:
`recap_capability`, `recap_adapter`, `recap_state`, `recap_ownership`, and
`discovery::bounded_command`. The native `AppHandle` loader belongs to the full
Tauri build gate; a small exact-module Cargo harness alone does not certify that
integration. Run `just ci` before the PR and require the immutable head's
NuncioCrew Gate. See [the runtime limits](ARCHITECTURE.md#bounded-inventory-limits-2026-09-10)
for the current unsupported inventory.

Test boundaries include explicit model/profile admission, identity invalidation,
fixed argv with a fake tool sentinel, native final-result/model parsing, private
unlinked stdin and size limits, durable process-pending state, copied/symlinked
ownership records, root-generation replacement, UID/build/profile/exclusion
mismatch, and rejection of generation/auth claims in an ownership-only manifest.
The bounded process tests cover aggregate discovery versus independent recap
budgets, cancellation before and after spawn, zero deadlines, EPERM retry only
after observed root reap, and cleanup failures. The existing escaped-descendant
test deliberately proves the limit of Unix process groups and cleans its own
fixture; it must never be reported as whole-tree containment.

Keep RED, mutation and restored-GREEN evidence separate. Removing the fixed
`--tools` argument, prompt cap, pending-process cleanup guard, or EPERM reap
condition must fail the corresponding production-seam regression. Synthetic
argv/output tests and authorized design reviews do not establish provider auth,
effective generation model, native tool isolation or a working recap. Real
staging generation remains blocked until one exact runtime combination proves
all those properties and #348 supplies a separate runtime-ready grant. Evidence
screenshots must label a source/tooling summary as such; they cannot substitute
for the designated staging runtime acceptance required by #351/#356.

The historical 2026-09-09 executed inventory used resolved native executables,
not the user's updating wrapper. The Claude image matches the current
Architecture row; the historical Codex 0.153.4 image used SHA-256
`b973d440acac501fd2594a43e7ca9ce41e0a65b9dfb28d0d7a7837c99e1261e3`, which
differs from the current static row and is not current certification. Recorded
arguments (excluding that executable) were:

- Claude: `["--version"]` and `["--safe-mode", "--setting-sources", "", "--help"]`.
  Cleared environment allowlist: `HOME`, `PATH`, `TMPDIR`, `CLAUDE_CONFIG_DIR`,
  `CLAUDE_CODE_SAFE_MODE`, `DISABLE_AUTOUPDATER`.
- Codex: `["--version"]`, `["--help"]`, `["exec", "--help"]`,
  `["app-server", "--help"]`, `["app-server", "generate-json-schema", "--help"]`,
  and `["app-server", "generate-json-schema", "--experimental", "--out", "/tmp/crew-351-evidence/codex-schema"]`.
  Cleared environment allowlist: `HOME`, `PATH`, `TMPDIR`, `CODEX_HOME`.
- Hermes: no executable arguments were run; inspection followed the installed
  CLI, `run_agent.AIAgent`, and `hermes_cli/oneshot.py` source imports only.

HOME/config/temp paths above pointed to disposable inventory roots. Help/version
capture used a 10-second deadline and 64 KiB per stream. The initial Claude
inventory retained `process_group_cleanup_denied` despite exit 0; it is not a
clean containment result. Subsequent owned synthetic process fixtures established
the unreaped-root EPERM case and the checked reap/retry behavior. No repeat
Claude call was used to replace that failed cleanup record with a success.

For #354 / G-THREAD-1 v3, bind regressions to the production pane store, scoped
forge subject, current-generation dispatch and drawer Escape gate. Prove exact
thread return restores selection without opening/launching; account/removal
clears scope but query failure does not fabricate removal; explicit empty and
invalidated plans remain distinct from retained history; narrow Escape preserves
draft/scroll and returns focus without closing the outer thread. Observe zero
native activation commands from selection/restoration, while existing hide and
sim-visible-false cleanup still executes. Preserve channel-to-channel remount
compatibility. Mock checks do not satisfy native #348/#357 evidence.

The #354 control foundation's `crew_thread_cancel_tests` Rust filter calls the
real cancel handler with `AgentPool`, `EventQueue` and `ObserverHandle`. It covers
stale targets with queued/replacement work, wrong channel/conversation, malformed
explicit target presence, a closed signal receiver, accepted-only lease release,
exact success and absent-turn legacy cancellation. The injected release callback
observes the handler's side-effect decision; it does not prove native OS cleanup.
Run `owner_control_command_tests` for adjacent queue/control compatibility.

`useChannelUserInput.actions.test.mjs` mounts the real hook with existing durable
hydration and authorization logic, stubbing relay/identity/IPC boundaries. It
covers same-tick duplicate publication, current failure/retry, stale relay/viewer
callbacks, resolved-request replay, immediate ownership revocation and concurrent
question completion. `userInputAnswerGate.test.mjs` proves bounded admission has
a visible error and recovers after authoritative reconciliation. These are scoped
UI publication checks; the existing elicitation durable-claim tests establish the
separate native claim contract. Selected-run controls additionally use the composed Activity tests below. Strict
Steer and native workflow evidence remain required before #354 is complete.

The first #354 slice adds production-bound `threadToolPaneSelection`,
`threadToolPaneInvalidation`, `threadToolPanePresentation`,
`threadPaneChatPreservation`, `threadNativeActivation`, `toolPaneKeyboard`, and
`ThreadAgentTranscript` Node suites under `desktop/src/features/tool-pane/`.
They exercise the real store, confirmed refresh seam, components and shared Radix
dialog, stubbing native IPC and unrelated dependencies. `toolPaneKeyboard` also
proves that thread Browser/Sim shortcuts enter the focus host before selecting a
tab. `ThreadAgentTranscript` binds the shared channel archive-paging prop and
proves agent switching does not start another loader. The composed
`threadArchiveOwnerComposition` suite executes the real conversation body, Activity
peek, declared-plans hook, and information tab. It counts the archive-hook boundary
from initial mount through Activity open, agent switch, and close, and requires one
stable paging object and one uninterrupted hydration lifecycle. `threadPlansPlacement`
binds the conversation body; `threadTranscriptLiveness` covers optional
exact-conversation live projection.
Existing declared-plan and anchored-scroll suites remain the parser/generation
and paused-reader contracts. The registered `companyos-thread-workspace.spec.ts`
adds mock-bridge workflow coverage; running it requires the owned E2E build slot.
Its Tools entry cases cover both side-thread and focus-thread modes. Deferred
native promises cover old Browser bounds/hide and simulator visibility failures
after a newer presentation, plus visible current-attempt failures and retry.
These presentation checks complement the second-slice controls tests below;
none replace final `just ci` or real installed staging screenshots.


#354 scoped Stop native source tests live in
`commands/scoped_observer_control_tests.rs`. They bind the actual command helper,
native owner capture, shared transport, and bounded loopback HTTP receiver.
The required cases are zero connections on stale scope/malformed target and
identity import ABA during admission; captured event/NIP-98 owner and decrypted
exact target on acceptance; unknown on mismatched ACK/refusal/disconnect; and
preserved accepted outcome when identity changes after send. Native execution
remains pending its allocated build slot; source registration is not RED/GREEN
or installed-runtime evidence. The shared transport's existing tests also remain
required and unchanged.


`ThreadSelectedRunControls.test.mjs` binds the real component and correlated
outcome helper, including same-tick duplicate claims, ownership refresh versus
revocation, exact turn/request results, stale completion, and unknown delivery.
`ThreadActivityRunControls.test.mjs` composes the actual Activity tab, run picker,
selected control and outcome helper with external store/native boundaries mocked.
It requires explicit choice, exact native token/target publication, no successor
retarget, and fail-closed native owner mismatch. Node pass counts and falsifiable
baseline evidence belong in the task; these mocks do not exercise native IPC.
