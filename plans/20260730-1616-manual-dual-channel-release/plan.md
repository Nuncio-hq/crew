# Manual dual-channel release

## Outcome

Ship a clearly marked local build and a manager-triggered NuncioCrew macOS
release lane without changing Buzz's upstream release workflow.

## Approved behavior

- Local flavor shows `Local`, keeps the Buzz local identity, and has no updater.
- Release workflow runs only through GitHub `workflow_dispatch`.
- First release is `v0.0.1-dev`; stable and dev use separate updater manifests.
- Dev users may receive a later stable release; stable users never receive dev.
- Committed Buzz desktop manifests remain pinned to `0.5.2`.
- First distributed target is macOS Apple Silicon with Nuncio identity.
- No credentials are committed, logged, or copied into the repository.

## Phases

1. **RED contracts** — local marker, manual trigger, channel/version validation,
   upstream pin, endpoint isolation.
2. **Implementation** — additive Crew config, release helper, GitHub workflow,
   local marker, and Nuncio release download URL.
3. **Verification** — focused tests, desktop checks, workflow audit, signing
   preflight, and independent review.
4. **Publication gate** — protected Environment and encrypted credentials are
   configured; manually run the action after merge, then verify install and
   updater end to end.

## Files

Prefer new Crew-owned files under `.github/workflows/`, `desktop/scripts/`,
`desktop/src/testing/`, and `docs/crew/`. Existing Buzz files may receive only
the smallest build-marker and release-URL integration edits.

## Rollback

Remove the Crew workflow/config/helper files and revert the two small desktop
integration edits. The upstream Buzz release workflow and pinned manifests
remain untouched.

## Verification

```text
cd desktop
node --import ./test-loader.mjs --experimental-strip-types --test \
  src/testing/nuncio-crew-*.test.mjs
pnpm typecheck
pnpm check
pnpm build
```

Publishing, tag creation, and a public release require a separate explicit
execution step after the implementation PR is merged and its signed dry run is
green.
