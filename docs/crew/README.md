# Crew documentation

This repository is [Nuncio-hq/crew](https://github.com/Nuncio-hq/crew)
(product name **NuncioCrew**, "Crew"), a fork of
[block/buzz](https://github.com/block/buzz). Upstream docs elsewhere in the
tree stay as they are; Crew's own rules live only in this directory.

## Read these, in this order, before working on Crew

1. Root [`AGENTS.md`](../../AGENTS.md) — how to work in this codebase
   (upstream conventions + the Crew banner at the top).
2. [`PRODUCT.md`](PRODUCT.md) — what Crew is and where it is going.
3. [`FORK.md`](FORK.md) — identity, how we sync upstream, the fork-delta
   record and its CI check.

That is the whole reading list. Everything else below is looked up when
needed, not read up front.

## The seven living documents

Agents may **edit** these. Agents may **not** create new top-level documents
in `docs/crew/`; a new `.md` here fails review. If something durable does not
fit one of these seven, that is a signal to change one of them, not to add
an eighth.

| File | Answers | Update style |
| --- | --- | --- |
| [`PRODUCT.md`](PRODUCT.md) | What / why / direction | rewrite in place when direction changes |
| [`FORK.md`](FORK.md) | Buzz vs Crew, sync, delta | rewrite in place |
| [`fork-delta.json`](fork-delta.json) | Which upstream files Crew edits and how to resolve them | edit areas; CI-checked |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Crew boundaries and data flow | rewrite in place |
| [`DECISIONS.md`](DECISIONS.md) | Why (durable, numbered) | **append only** |
| [`STATE.md`](STATE.md) | Where things stand right now, ≤ ~60 lines | rewrite the sentence that is no longer true |
| [`HERMES.md`](HERMES.md) | Hermes hire / bind / offboard runbook | rewrite in place |

Supporting runbooks that are also living but narrow:
[`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md),
[`TESTING.md`](TESTING.md), [`LOCAL-BUILD.md`](LOCAL-BUILD.md),
[`RELEASING.md`](RELEASING.md), [`GUIDES/`](GUIDES/),
[`templates/`](templates/).

## Records (append-only directories)

Dated evidence. Agents create new files here freely; nobody has to keep
them current, and they are never the source of truth. If a record reaches
a durable conclusion, copy the conclusion into one of the seven above.

| Directory | What goes there |
| --- | --- |
| [`spikes/`](spikes/) | Feasibility evidence, one file per spike |
| [`verification/`](verification/) | Reproducible evidence for a delivered slice |
| [`features/`](features/) | Feature plans (stories + slices) |
| [`upstream-proposals/`](upstream-proposals/) | Things Crew would like upstream to absorb |
| [`archive/`](archive/) | Superseded living documents, kept for history |
| `../../plans/` | Agent working plans and reports; frozen, not read |

## The rule, in one line

**Durable → edit one of the seven. Evidence for today → add a record.
There is no third option.**

## Workflow

Every behavior change: spike → contract tests → smallest implementation →
verification → update the affected living document in the same PR.
Details in [`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md). The founder
is the client; CI green is not Accept (D-070).
