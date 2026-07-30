# Development Workflow

## Core rule

No behavior change goes directly from idea to implementation.

```text
Spike -> RED tests -> edge-case tests -> approved plan -> implementation
```

The workflow is intentionally evidence-first because the manager reviews intent
and outcomes rather than supervising code construction.

## Gate 0: Intake

Write the intended outcome in manager language:

- Who needs what?
- What becomes possible?
- What must remain unchanged?
- What would count as failure?
- Which decisions are already locked?

Do not begin by naming files or libraries unless they are genuine constraints.

## Gate 1: Feasibility spike

Every feature starts with a spike under `docs/crew/spikes/`.

A spike must:

- ask one decision-changing question;
- use the smallest realistic environment;
- exercise the real boundary when that boundary is the uncertainty;
- define pass, fail, and inconclusive before running;
- collect reproducible evidence;
- identify provider/platform differences;
- list limitations and unanswered questions;
- clean up disposable artifacts;
- end with `PASS`, `FAIL`, or `INCONCLUSIVE`.

A spike is not production implementation. Code used only for a spike stays
disposable unless a later plan explicitly promotes it.

If the spike fails, do not implement around the failure. Revise the design,
run a narrower spike, or ask for a product decision.

## Gate 2: Contract and test design

After a passing spike, translate intent into observable contracts.

For each contract identify:

- input or triggering event;
- authoritative state before the action;
- expected events and visible result;
- forbidden side effects;
- error behavior;
- retry and recovery behavior;
- concurrency or ordering behavior.

The test plan must be reviewable before production code changes.

## Gate 3: RED

Write the smallest test that expresses the missing behavior and run it.

RED is valid only when:

- the test fails for the intended missing behavior;
- setup and fixtures are healthy;
- the assertion observes a public contract rather than implementation trivia;
- the failure message explains the mismatch.

Capture the command and the relevant failure. A test that passes immediately
does not prove the new contract; repair the test before continuing.

## Gate 4: Edge cases before implementation

Add the cases most likely to change the design while change is still cheap.
Use [`TESTING.md`](TESTING.md) as the checklist.

At minimum consider:

- invalid and missing input;
- duplicate or replayed events;
- out-of-order events;
- concurrent transitions;
- cancellation and timeout;
- restart, reconnect, and resume;
- filesystem permission and missing-path failures;
- provider differences;
- upstream compatibility.

Do not attempt exhaustive coverage blindly. Prioritize cases that could change
the architecture, state model, or user experience.

## Gate 5: Plan approval

The implementation plan must state:

- manager-visible outcome;
- spike evidence;
- tests already RED;
- files to add;
- upstream files, if any, that must be edited;
- event and state transitions;
- rollback strategy;
- verification commands;
- unresolved questions.

No production implementation starts until the manager approves this plan.

## Gate 6: Smallest implementation

Implement only enough to satisfy the approved contracts.

Rules:

- prefer new Crew-owned files;
- preserve upstream behavior;
- keep upstream-file edits within the explicit diff budget;
- do not introduce speculative abstractions;
- do not weaken, delete, skip, or rewrite tests merely to make them pass;
- do not silently expand scope;
- stop if evidence contradicts a locked decision.

## Gate 7: GREEN and refactor

Run the narrow tests first. Once green:

1. Refactor without changing the contract.
2. Re-run the narrow tests after every meaningful refactor.
3. Run affected upstream suites.
4. Run the repository quality gates required by upstream `AGENTS.md`.

Separate product failures from environment limitations. Report both precisely.

## Gate 8: Review and documentation

Review must verify:

- the user-approved intent is preserved;
- tests cover the actual failure modes;
- relay events remain authoritative where required;
- filesystem and media boundaries remain intact;
- the fork surface is still small;
- no unrelated upstream behavior changed.

Then update:

- `STATE.md` with current truth;
- `DECISIONS.md` only for durable new decisions;
- the spike with final follow-up evidence;
- user-facing or architecture docs when behavior changed.

## Documentation-only changes

Documentation changes still begin with a scope spike: inspect upstream docs,
confirm the new file does not collide, and identify authoritative sources.
They do not require contrived unit tests. Their RED equivalent is a documented
validation target such as a broken link, missing required section, or an
additive-only diff assertion. Validate links, formatting, and Git diff before
completion.

## Stop conditions

Stop and ask rather than infer when:

- a proposed change reverses a locked product decision;
- an upstream existing-file edit exceeds the agreed budget;
- the spike is inconclusive;
- tests expose a different contract than the approved intent;
- credentials, external publication, deletion, or deployment require new
  authority;
- provider behavior differs in a way that changes user experience.
