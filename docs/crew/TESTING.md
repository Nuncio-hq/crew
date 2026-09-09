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

## ACP AUTH correlation (#338, first slice)

`cargo test -p buzz-acp --lib auth_correlation_tests` drives the actual
`do_connect` handshake using ephemeral loopback WebSockets. It covers unrelated
positive/negative OK frames before and after the challenge, exact AUTH denial,
retryable `error:` dependency denial, AUTH-close, and buffered generic/channel
CLOSED frames. Unrelated acknowledgements must neither authenticate an attempt
nor reject credentials; only the exact sent AUTH ID can settle the handshake.
Existing subscription handling consumes the buffered frames afterward.

These fixtures do not prove bounded reconnect health episodes, local Desktop
transport projection, in-flight receipt recovery during exhaustion, or installed
staging acceptance. Those remain #338 gates; real staging requires #348.

## CompanyOS grouped evidence (#344)

Related issues may reuse one unchanged build and owned real-data run; keep an
explicit issue-to-case mapping and post evidence on each corresponding issue.
Record candidate SHA/build, source revision, runtime/model/profile, test data
identity, handler effects/readback and recovery. Preserve active employee
sessions and private data. Stage 0 reference screenshots are explicitly
simulated; final implementation evidence launches the real candidate flow.
Use `scripts/post-screenshots.sh` on the resulting PR and copy its immutable
image URLs with concise captions into the issue. Required final CI, independent
exact-head review and installed release acceptance remain separate gates.
