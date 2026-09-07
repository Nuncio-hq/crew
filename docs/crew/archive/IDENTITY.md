# NuncioCrew Identity (for agents)

Read this before treating the repository as stock Buzz.

## What this repository is

| Name | Meaning |
|------|---------|
| **NuncioCrew** / **Crew** | This product fork. GitHub repo `Nuncio-hq/crew`. Desktop app / product dir `NuncioCrew`. |
| **Buzz** | Upstream platform and protocol (`block/buzz`). Relay, Nostr kinds, ACP harness, most crate and path names. |
| **Upstream** | Remote `upstream` → `https://github.com/block/buzz.git` (fetch only). Never push here. |
| **Origin** | Remote `origin` → `https://github.com/Nuncio-hq/crew.git` (fetch and push). |

Crew is a **thin GitHub fork** of Buzz. Most of the tree is still Buzz source.
Crew-specific product rules live under [`docs/crew/`](README.md).

## Naming rules (do not mix these up)

1. **Say NuncioCrew / Crew** when talking about this fork's product, GitHub
   repo, CI (`NuncioCrew CI` / `NuncioCrew Gate`), releases, or local data dir
   (`…/Application Support/NuncioCrew/…`).
2. **Say Buzz** when talking about the upstream platform, protocol, shared
   crates (`buzz-*`), Nostr kinds, or behavior that is intentionally unchanged
   from `block/buzz`.
3. **Do not** write PR titles, issue text, or agent plans as if this checkout
   were `block/buzz`. Issues and PRs go to `Nuncio-hq/crew`.
4. **Do not** "fix" upstream docs by renaming every "Buzz" to "Crew". That
   makes upstream merges painful. Point agents here instead.

## Where agents must look first

1. This file (`docs/crew/IDENTITY.md`).
2. [`docs/crew/README.md`](README.md) — Crew reading order and workflow.
3. [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md) and
   [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md) — product
   north star and how to work with this founder (plain language).
4. Root [`AGENTS.md`](../../AGENTS.md) — upstream Buzz contributor guide
   (still accurate for crates, quality gates, and patterns).
5. [`STATE.md`](STATE.md) / [`DECISIONS.md`](DECISIONS.md) — current fork state.

If Crew docs and upstream docs conflict, **stop and surface the conflict**.
Do not silently prefer upstream "Buzz" wording for fork product decisions.

## Product directory vs protocol name

macOS Application Support paths agents hit while debugging (do not conflate):

| Path | Belongs to |
|------|------------|
| `~/Library/Application Support/NuncioCrew/` | Crew node-tools + runtimes (Tauri `productName`) |
| `~/Library/Application Support/Buzz/` | Older shared / stock Buzz tree — **leave in place** from Crew automation |
| `~/Library/Application Support/com.nuncio.crew/` | Crew identity + agents (bundle id) |
| `~/Library/Application Support/xyz.block.buzz.app/` | Stock Buzz / local Crew build using upstream bundle id |

Desktop managed Node/npm trees use Tauri `productName`:

| App | Data directory segment |
|-----|------------------------|
| Stock Buzz | `Buzz` |
| NuncioCrew | `NuncioCrew` |

Example (macOS):

```text
~/Library/Application Support/NuncioCrew/node-tools/<platform>/
~/Library/Application Support/NuncioCrew/runtimes/node/…
~/Library/Application Support/com.nuncio.crew/…
```

Older NuncioCrew builds used the literal `Buzz/` segment. Legacy reclaim only
cleans `NuncioCrew/node-tools` (unscoped). Do **not** delete
`~/Library/Application Support/Buzz/…` from NuncioCrew automation — that tree
may belong to a stock Buzz install on the same machine. NuncioCrew-only users
may remove leftover `Buzz/` trees manually.

## Remotes and branches

```text
upstream  https://github.com/block/buzz.git      fetch only
origin    https://github.com/Nuncio-hq/crew.git  fetch and push
```

- Feature work: short-lived branches on `origin`, PRs into Crew `main`.
- Upstream pulls: `sync/upstream-YYYY-MM-DD` per [`UPSTREAM-SYNC.md`](UPSTREAM-SYNC.md).
- Baseline pin: [`upstream-buzz.json`](upstream-buzz.json).

## CI and release (Crew, not stock Buzz)

| Workflow | Role |
|----------|------|
| `NuncioCrew CI` / required check `NuncioCrew Gate` | Normal PR merge gate (macOS Apple Silicon lean matrix) |
| `NuncioCrew Upstream Sync` | Manual full compatibility when syncing upstream |
| `NuncioCrew Release` | Manual signed/notarized macOS release |

Inherited Buzz workflows under `.github/workflows/` may still exist in the tree
but are disabled at the GitHub repo level. Do not re-enable them casually.
See [`CI.md`](CI.md) and [`RELEASING.md`](RELEASING.md).

## Thin-fork edit budget

Prefer new files under Crew namespaces (`docs/crew/`, additive desktop routes,
Crew workflows). Keep edits to upstream files small and justified. Details:
[`UPSTREAM-SYNC.md`](UPSTREAM-SYNC.md), decision D-001 in
[`DECISIONS.md`](DECISIONS.md).
