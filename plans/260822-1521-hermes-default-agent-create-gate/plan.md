# Hermes default agent create gate

Status: Complete

## Scope

- Let profile-owned/write-through Hermes models satisfy the hidden model gate.
- Preserve ordinary Custom AI provider/model requirements.
- Preserve duplicate Hermes profile occupancy rejection.
- Stop create drafts from claiming an existing managed-agent profile binding.
- Surface a precise submit blocker where the dialog already knows it.

## Implementation

1. Add policy tests for hidden profile-owned/write-through models.
2. Add usage tests for unbound create drafts versus bound agents.
3. Pass model visibility/ownership into the shared AI configuration gate.
4. Make profile usage projection depend on a real bound managed agent.
5. Render neutral prospective copy for an unbound draft.
6. Run targeted tests, desktop unit tests, typecheck, and independent review.

## Success criteria

- Hermes `default` can be created when all visible requirements are satisfied.
- Codex/Claude still require a model; Buzz Agent/Goose still require provider + model.
- A profile bound to another managed agent still blocks create.
- Unbound create UI says `Not yet` and does not claim an active binding.
