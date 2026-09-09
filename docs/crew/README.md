# Crew documentation

This repository is [Nuncio-hq/crew](https://github.com/Nuncio-hq/crew)
(product name **NuncioCrew**, "Crew"), a fork of
[block/buzz](https://github.com/block/buzz). Use upstream docs in this
checkout for unchanged Buzz behavior. Crew docs describe extensions and
Crew-owned contracts; correct misleading upstream references with a concise
pointer to the relevant Crew doc. Do not duplicate upstream documentation.

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

## Shared design reference

[CompanyOS blueprint](../../design/companyos/README.md) is the one maintained
interactive reference (D-078). Its rendered Stage 0 matrix separates accepted
contracts, proposals and blocked controls; living docs retain authority.

## Records (append-only directories)

Dated evidence, not current product contracts. Existing records remain
historical; a durable conclusion belongs in the relevant living document.
Do not create a record for every task. Temporary plans, progress, and
one-run evidence belong in the task or PR by default.

A new record is justified only when evidence needs lasting reuse and no
existing document fits. State that reason in the task or PR, date the
record, and link its authoritative living document. Moving a new file into
`plans/` or another directory does not bypass this rule. Prefer reusable
tests and scripts over repeated prose reports.

| Directory | What goes there |
| --- | --- |
| [`spikes/`](spikes/) | Feasibility evidence, one file per spike |
| [`verification/`](verification/) | Reproducible evidence for a delivered slice |
| [`features/`](features/) | Feature plans (stories + slices) |
| [`upstream-proposals/`](upstream-proposals/) | Things Crew would like upstream to absorb |
| [`archive/`](archive/) | Superseded living documents, kept for history |
| `../../plans/` | Agent working plans and reports; frozen, not read |

## The rule, in one line

**Durable knowledge → update an existing living document. Temporary evidence
→ task or PR. New documents → justified exceptions, not a delivery ritual.**

## Workflow

Every behavior change: assess uncertainty → spike only if needed → contract tests → smallest implementation →
verification → update the affected living document in the same PR.
If no docs are affected, explain why in the handoff. Keep each fact in one
authoritative location; link rather than repeat. Rewrite stale descriptions
and distinguish shipped behavior, accepted future work, and proposals.
Review affected Crew documentation on every upstream sync. Verify both
links and agreement with the resulting code; formatting alone is insufficient.
Details in [`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md). The founder
is the client; CI green is not Accept (D-070).
