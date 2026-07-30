# Crew Documentation

Crew is Nuncio's thin fork of [block/buzz](https://github.com/block/buzz).
It turns Buzz into mission control for a manager coordinating a team of
long-running coding agents.

This directory contains Crew-specific product rules and engineering decisions.
Upstream documentation remains intact so updates from `block/buzz` stay easy to
review and merge.

## Authority and reading order

Before researching, planning, or changing Crew, an agent must read:

1. Upstream [`AGENTS.md`](../../AGENTS.md) and [`CLAUDE.md`](../../CLAUDE.md).
2. This file.
3. [`VISION.md`](VISION.md).
4. [`ARCHITECTURE.md`](ARCHITECTURE.md).
5. [`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md).
6. [`TESTING.md`](TESTING.md).
7. [`STATE.md`](STATE.md) and [`DECISIONS.md`](DECISIONS.md).
8. The relevant spike and feature plan.

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
| [`VISION.md`](VISION.md)                             | Product intent and locked behavior | Rarely                       |
| [`ARCHITECTURE.md`](ARCHITECTURE.md)                 | Crew boundaries and data flow      | When architecture changes    |
| [`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md) | Mandatory delivery gates           | Rarely                       |
| [`TESTING.md`](TESTING.md)                           | TDD and edge-case strategy         | As test surfaces evolve      |
| [`LOCAL-BUILD.md`](LOCAL-BUILD.md)                   | Build and test NuncioCrew locally  | When packaging changes       |
| [`CI.md`](CI.md)                                     | Lean merge and upstream-sync gates | When CI scope changes         |
| [`RELEASING.md`](RELEASING.md)                       | Manual dev/stable release runbook  | When distribution changes    |
| [`UPSTREAM-SYNC.md`](UPSTREAM-SYNC.md)               | Thin-fork and sync runbook         | When Git workflow changes    |
| [`STATE.md`](STATE.md)                               | Short, current project state       | Frequently; rewrite in place |
| [`DECISIONS.md`](DECISIONS.md)                       | Durable rationale                  | Append only                  |
| [`spikes/`](spikes/README.md)                        | Feasibility evidence               | One record per spike         |
| [`verification/`](verification/README.md)            | Reproducible feature evidence      | One record per delivered slice |
| [`templates/`](templates/)                           | Required work artifacts            | When workflow changes        |

## Current scope

The first product slice makes a NIP-34 Project record point to a local
workspace folder without changing repository identity and without changing
`session/new.cwd`. See [`STATE.md`](STATE.md) for the exact boundary.
