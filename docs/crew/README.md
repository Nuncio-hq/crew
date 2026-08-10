# Crew Documentation

**This repository is [Nuncio-hq/crew](https://github.com/Nuncio-hq/crew), not
[block/buzz](https://github.com/block/buzz).** Crew (product name **NuncioCrew**)
is Nuncio's thin fork of Buzz. It turns Buzz into mission control for a manager
coordinating a team of long-running coding agents.

Start with [`IDENTITY.md`](IDENTITY.md) if you are unsure when to say "Buzz"
versus "NuncioCrew" / "Crew".

This directory contains Crew-specific product rules and engineering decisions.
Upstream documentation remains intact so updates from `block/buzz` stay easy to
review and merge. Leaving "Buzz" in upstream docs is intentional — do not mass
rename those files.

## Authority and reading order

Before researching, planning, or changing Crew, an agent must read:

1. [`IDENTITY.md`](IDENTITY.md) — fork vs upstream naming (read first).
2. [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md) — founder north star (Hermes-first,
   Buzz contracts, mobile, what “company” means in plain language).
3. [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md) — how to explain,
   refuse mis-assignment, and stay honest with this founder.
4. Upstream [`AGENTS.md`](../../AGENTS.md) and [`CLAUDE.md`](../../CLAUDE.md)
   (Buzz codebase conventions; this checkout is still the Buzz tree).
5. This file.
6. [`VISION.md`](VISION.md) — older mission-control framing; if it conflicts
   with `FOUNDER-PRODUCT.md`, surface the conflict (do not silently pick).
7. [`ARCHITECTURE.md`](ARCHITECTURE.md).
8. [`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md).
9. [`TESTING.md`](TESTING.md).
10. [`STATE.md`](STATE.md) and [`DECISIONS.md`](DECISIONS.md).
11. The relevant spike and feature plan.

Upstream rules govern the Buzz codebase. Crew rules add stricter fork,
product, and delivery constraints. If they conflict, stop and surface the
conflict instead of silently choosing one.

## Non-negotiable workflow

Every behavior change follows this sequence:

```text
question
  -> feasibility spike
  -> evidence and verdict
  -> failing contract tests
  -> edge-case tests
  -> approved implementation plan
  -> smallest implementation
  -> refactor while green
  -> full verification
```

No production implementation begins before the spike is conclusive, the test
contract is visible, and the manager approves the plan.

## Documentation map

| Document                                             | Purpose                            | Update style                 |
| ---------------------------------------------------- | ---------------------------------- | ---------------------------- |
| [`IDENTITY.md`](IDENTITY.md)                         | Fork vs Buzz naming for agents     | When identity/CI/paths change |
| [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md)           | Founder north star; Hermes + Buzz  | When product direction locks |
| [`AGENT-WORKING-AGREEMENT.md`](AGENT-WORKING-AGREEMENT.md) | Plain-language agent collaboration | When communication rules change |
| [`VISION.md`](VISION.md)                             | Mission-control intent (legacy framing) | Rarely; reconcile with founder product |
| [`ARCHITECTURE.md`](ARCHITECTURE.md)                 | Crew boundaries and data flow      | When architecture changes    |
| [`HERMES.md`](HERMES.md)                             | Hermes hire/spawn runbook          | When Hermes ops change       |
| [`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md) | Mandatory delivery gates           | Rarely                       |
| [`TESTING.md`](TESTING.md)                           | TDD and edge-case strategy         | As test surfaces evolve      |
| [`LOCAL-BUILD.md`](LOCAL-BUILD.md)                   | Build and test NuncioCrew locally  | When packaging changes       |
| [`CI.md`](CI.md)                                     | Lean merge and upstream-sync gates | When CI scope changes         |
| [`RELEASING.md`](RELEASING.md)                       | Manual dev/stable release runbook  | When distribution changes    |
| [`UPSTREAM-SYNC.md`](UPSTREAM-SYNC.md)               | Thin-fork and sync runbook         | When Git workflow changes    |
| [`STATE.md`](STATE.md)                               | Short, current project state       | Frequently; rewrite in place |
| [`DECISIONS.md`](DECISIONS.md)                       | Durable rationale                  | Append only                  |
| [`features/`](features/README.md)                    | Feature plans (stories + slices)   | One document per initiative  |
| [`spikes/`](spikes/README.md)                        | Feasibility evidence               | One record per spike         |
| [`verification/`](verification/README.md)            | Reproducible feature evidence      | One record per delivered slice |
| [`templates/`](templates/)                           | Required work artifacts            | When workflow changes        |

## Current scope

The first product slice makes a NIP-34 Project record point to a local
workspace folder without changing repository identity and without changing
`session/new.cwd`. See [`STATE.md`](STATE.md) for the exact boundary.
