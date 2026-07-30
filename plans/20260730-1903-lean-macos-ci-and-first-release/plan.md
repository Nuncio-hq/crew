# Lean macOS CI and first release

## Outcome

Merge PR #1 through a Crew-owned macOS Apple Silicon gate, then publish the
first signed and notarized `v0.0.1-dev` release without changing Buzz's
upstream workflow files or pinned `0.5.2` manifests.

## Approved boundaries

- Automatic CI covers desktop contracts, an unsigned macOS ARM package, and
  the relay-native Project test only when relevant paths change.
- The stable required check is exactly `NuncioCrew Gate`.
- Expected supporting checks are `Desktop Fast`, `macOS ARM Package`, and
  `Project Relay`; the last may be skipped when its path filter is false.
- Core root and desktop Tauri Rust compatibility checks remain an explicit
  manual workflow, `NuncioCrew Upstream Sync`; full platform and integration
  compatibility is not claimed.
- `.github/workflows/ci.yml`, `docker.yml`, and every other inherited workflow
  stay byte-for-byte unchanged.
- Inherited workflows are disabled in GitHub only after `NuncioCrew CI` is
  green on `main`.
- Release remains manual through the existing `NuncioCrew Release` workflow
  and protected `nuncio-crew-release` Environment.

## Phases

1. [Spike, RED contracts, additive CI](phase-01-spike-contracts-and-additive-ci.md)
2. [PR gate and workflow cutover](phase-02-pr-gate-and-workflow-cutover.md)
3. [First signed release and verification](phase-03-first-signed-release-and-verification.md)

## Order of execution

1. Record the inherited failure baseline and complete the no-secret packaging
   spike.
2. Add failing static workflow contracts before either new workflow exists.
3. Add only Crew-owned CI and manual upstream-sync workflow files.
4. Run focused contracts, desktop gates, unsigned package validation, and
   diff-integrity checks locally.
5. Push PR #1 and require `NuncioCrew Gate` to pass.
6. Merge, verify the same gate on `main`, then disable inherited workflows in
   GitHub repository settings.
7. Run `v0.0.1-dev` first as `publish=false`, inspect the signed artifact, then
   rerun the identical immutable inputs with `publish=true`.
8. Verify the versioned release, dev manifest, public DMG, and rollback path.

## Release blockers

- The inherited macOS job cannot find its fetched `mesh-llm` checkout.
- Inherited Docker jobs cannot write to Block-owned registry/cache resources.
- Signing, notarization, and GitHub updater publication are unproven until the
  merged manual workflow succeeds on a protected GitHub runner.
- `v0.0.1-dev` is below Buzz `0.5.2`; it is a manual first install, not an
  update from the current Buzz/local build.

## Rollback

Re-enable inherited workflows and disable `NuncioCrew CI` at repository level,
then revert only the additive Crew workflow and contract files. For release
rollback, never reuse a tag or signing key: stop before moving `latest.json`,
or restore the last known-good manifest and publish a higher fixed version.

## Result

Completed 2026-07-30. Main gate, protected signed dry run, public release,
notarization, stapling, public manifest, and mounted-DMG launch all passed.

## Unresolved questions

Real-profile relay/Project acceptance remains a manager test. Updater E2E
requires a second Crew version higher than the installed Crew build.
