# Testing Strategy

## Purpose

Tests are design instruments. They should reveal incorrect assumptions and
edge cases before implementation commits the architecture.

**Client acceptance is a separate bar.** Technical green (`just ci`, e2e)
proves agent trust. Founder Accept requires Gate C (story + try script +
evidence + honest limit in the thread) — see issue #234 / D-070 and
[`templates/CLIENT-ACCEPTANCE.md`](templates/CLIENT-ACCEPTANCE.md). Do not
treat this file’s technical checklist as founder Accept.

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

## Test layers

| Layer             | What it proves                                 | Typical Crew use                                  |
| ----------------- | ---------------------------------------------- | ------------------------------------------------- |
| Pure unit         | Deterministic policy and projection            | Board transitions, WIP calculation, event parsing |
| Component         | React behavior around a stable contract        | Board columns, card detail, path selection        |
| Relay integration | Signed-event storage and subscription behavior | Board reconstruction, Project metadata            |
| ACP contract      | Provider and tool behavior                     | Context, permissions, workspace paths, resume     |
| Desktop E2E       | User-visible workflow in Tauri mock bridge     | Drag transition, Need Input priority, card popup  |
| Live smoke        | Real local relay and installed providers       | Final compatibility confidence                    |

Use the lowest layer that proves the contract, then add boundary tests where
cross-component behavior is the risk.

## Board edge-case checklist

Before implementing board orchestration, test:

- fourth card cannot enter `Working`;
- transition to `Need Input` immediately releases capacity;
- `Need Input` is ordered ahead of other manager attention;
- resolved input returns to the correct next state;
- duplicate transition events do not double-count capacity;
- out-of-order delivery converges to the same state;
- concurrent clients attempting the final slot resolve deterministically;
- reopening a `Done` card follows an explicit transition;
- missing or deleted Project references render safely;
- reconnect and cold start reconstruct the same board;
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
