# Phase 01 — Contracts and release lane

## Context

- [Plan](plan.md)
- [`docs/crew/DEVELOPMENT-WORKFLOW.md`](../../docs/crew/DEVELOPMENT-WORKFLOW.md)
- [`docs/crew/DECISIONS.md`](../../docs/crew/DECISIONS.md)

## Status

Implementation and local verification complete; PR and signed dry run pending.

## Requirements

- GitHub Actions has only a manual trigger.
- Inputs select exact source ref, version, channel, and publish/dry-run mode.
- Invalid versions, channel mismatches, moved refs, and existing releases fail
  before signing or publishing.
- Dev and stable updater endpoints stay separate.
- Local build cannot inherit updater configuration.
- Buzz source version and exact upstream commit remain machine-readable.
- Credentials enter only through a protected, `main`-only GitHub Environment.

## Architecture

```text
Manager Run workflow
  -> validate immutable ref + channel/version
  -> patch artifact version in CI checkout
  -> build macOS arm64 with Nuncio config
  -> Developer ID sign + Apple notarize
  -> Tauri updater signature
  -> versioned GitHub Release
  -> selected rolling latest.json release
```

## Implementation steps

1. Add failing static and helper contract tests.
2. Add a pure release-input classifier and pin metadata.
3. Add Nuncio distribution config and manual GitHub workflow.
4. Inject the local build marker and clear updater environment.
5. Point Nuncio manual downloads at the Crew release page.
6. Run focused and upstream desktop gates.
7. Complete independent review and update Crew documentation.

## Success criteria

- Contract tests demonstrate RED before implementation and GREEN after.
- No automatic workflow trigger exists.
- No `block/apple-codesign-action` or `block/buzz` release URL is used.
- No private credential appears in the diff or command output.
- Release is not published during implementation.

## Risks

- `v0.0.1-dev` cannot update the existing `0.5.2` local build; first install is
  manual.
- Nuncio distribution identity does not automatically migrate Buzz Keychain
  state; the first release may require importing the same identity in-app.
- GitHub signing/notarization cannot be proven end to end until the merged
  workflow completes its manager-approved dry run.

## Unresolved questions

None for implementation. Public release execution remains a separate gate.
