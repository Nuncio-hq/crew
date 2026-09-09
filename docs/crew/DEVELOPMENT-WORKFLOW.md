# Development Workflow

## Core rule

No behavior change goes directly from idea to implementation.

```text
Assess uncertainty -> spike if needed -> RED tests -> edge-case tests -> approved plan -> implementation
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

### Shared reference for CompanyOS UI work

Before production UI implementation, update the canonical
[CompanyOS blueprint](../../design/companyos/README.md) with the founder.
Start at `index.html` for each area's meaning, boundaries and open decisions;
follow its state links into `prototype.html` to exercise the UI.
Its `src/blueprint.js` is the single source for review state IDs and expected
behavior; the Blueprint panel renders those rules beside the interactive UI. Use the method in `PRODUCT.md`: an
overall journey, screen images or interactive states, and adjacent review
notes explaining purpose, user actions, behavior, acceptance, and open questions.
Include failure/recovery scenarios and expected results. Scenario controls
and implementation notes stay outside the app frame; simulations are labeled.

Use the recorded #344 approval for demonstrated Project/Wiki v0.9 states; do
not ask for the same approval again. Resolve only named outstanding gates.
Approval of one screen
does not approve all proposals in the reference. Each handoff points to the
specific reference state and its matching living-doc contract; verification
compares the real flow to that state. Update this same artifact in place.
The first HTML artifact is justified as the shared visual/interactive surface
that Markdown cannot provide, not as permission to add per-task specs. Backend
work with no affected user experience does not require a contrived mock screen.


## Gate 1: Feasibility spike

First identify any decision-changing uncertainty. Run a spike only when
existing code, tests, and documentation cannot resolve it; otherwise record
the supporting evidence briefly in the task or PR and proceed to Gate 2.
A spike does not require a new file. Keep its question, result, and evidence
in the task or PR by default. Apply the [documentation policy](README.md)
before adding any lasting record under `docs/crew/spikes/` or elsewhere.

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

After resolving feasibility, translate intent into observable contracts.

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
- feasibility evidence (including spike results when a spike was needed);
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

Select and verify the test environment using
[`TESTING.md`](TESTING.md#test-environments-and-real-data-staging-d-077)
before starting agents or write tests. The daily relay is not a fallback
for unprovisioned staging or missing fixture data.

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

Before completion, identify affected authoritative docs and update them in
the same change. If none are affected, explain why in the task or PR.
Update only the documents whose content changes:

- `STATE.md` with current truth;
- `DECISIONS.md` only for durable new decisions;
- any existing spike record whose conclusion changes;
- user-facing or architecture docs when behavior changed.

Review documentation against the final implementation, including links and
contradictions. Keep each fact in one home and link to it elsewhere; replace
stale text rather than appending progress logs. Distinguish shipped behavior,
accepted future work, and proposals. Preserve decision history while marking
superseded decisions with a successor reference.

For unchanged Buzz behavior, refer to upstream docs in this checkout. For
Crew extensions, document the delta; for Crew-owned components, document
the current Crew contract. Update ownership references in `FORK.md`,
`ARCHITECTURE.md`, and applicable `fork-delta.json` areas when they change.
An upstream sync includes this review for affected Crew differences, even
when Git reports no documentation conflicts.

Edit or consolidate existing docs first. Any new doc, including a plan or
verification record, needs a lasting purpose and a reason no existing doc
fits, stated in the task or PR. Temporary evidence stays in the task or PR.

## Documentation-only changes

Documentation changes begin by inspecting the affected docs and identifying
authoritative sources. No separate scope-spike file is required. Resolve
contradictions in existing documents and justify any necessary new file
under the policy above.
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

For the CompanyOS delivery ledger, related issues may share one unchanged
reference/candidate build and runtime run, while retaining issue-to-case mapping.
Each issue still needs its own safe evidence/readback comment. Prototype-only
screenshots are labeled as such and cannot satisfy real-runtime acceptance.
