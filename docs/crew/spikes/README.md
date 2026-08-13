# Spike Protocol

A spike answers one technical question that could change the design or stop
implementation.

## Required properties

- **Decision-linked:** State which decision depends on the result.
- **Bounded:** Limit scope, time, providers, and files.
- **Realistic:** Exercise the uncertain boundary with real components when
  feasible.
- **Disposable:** Do not quietly promote spike code into production.
- **Reproducible:** Record versions, commands, fixtures, and expected evidence.
- **Honest:** Distinguish `PASS`, `FAIL`, and `INCONCLUSIVE`.
- **Clean:** Remove temporary data and report what remains.

## Required sections

Use [`../templates/SPIKE.md`](../templates/SPIKE.md). Every spike includes:

1. Question.
2. Decision affected.
3. Hypothesis.
4. Scope and exclusions.
5. Pass/fail criteria.
6. Environment.
7. Method.
8. Results.
9. Edge cases observed.
10. Limitations.
11. Verdict.
12. Follow-up test contract.
13. Cleanup.

## Evidence rules

- A provider's final message is not sufficient evidence; verify the filesystem,
  relay, or observable output independently.
- A successful process exit is not sufficient evidence.
- Do not use `/tmp` alone for filesystem-sandbox conclusions because providers
  may treat it specially.
- Do not use mocks when the question is about ACP, relay, filesystem, or
  permission boundaries.
- Record unrelated warnings separately from failures.
- Never record credentials or secret values.

## Naming

```text
NNNN-short-decision-question.md
```

Numbers are chronological records, not plan phase identifiers used in code.

## Records

- [`0001-project-workspace-absolute-path.md`](0001-project-workspace-absolute-path.md)
- [`0002-project-local-location-schema.md`](0002-project-local-location-schema.md)
- [`0003-tauri-project-folder-picker.md`](0003-tauri-project-folder-picker.md)
- [`0004-nuncio-crew-local-release-build.md`](0004-nuncio-crew-local-release-build.md)
- [`0005-folder-first-project-create.md`](0005-folder-first-project-create.md)
- [`0006-reuse-existing-git-reader-for-exact-local-workspace.md`](0006-reuse-existing-git-reader-for-exact-local-workspace.md)
- [`0007-manual-dual-channel-release.md`](0007-manual-dual-channel-release.md)
- [`0008-lean-macos-arm-ci.md`](0008-lean-macos-arm-ci.md)
- [`0009-profile-bound-hermes-acp-spawn.md`](0009-profile-bound-hermes-acp-spawn.md)
- [`0010-hermes-headless-auth-probe.md`](0010-hermes-headless-auth-probe.md)
- [`0011-headless-hermes-profile-lifecycle.md`](0011-headless-hermes-profile-lifecycle.md)
- [`0012-one-profile-concurrent-acp.md`](0012-one-profile-concurrent-acp.md)
- [`0013-buzz-acp-model-leak-suppression.md`](0013-buzz-acp-model-leak-suppression.md)
- [`0014-agent-attention-recovery.md`](0014-agent-attention-recovery.md)
- [`0015-role-record-projection.md`](0015-role-record-projection.md)
- [`0016-role-prompt-adherence-matrix.md`](0016-role-prompt-adherence-matrix.md)
- [`0017-capability-spawn-grant-deny.md`](0017-capability-spawn-grant-deny.md)
- [`0018-spawn-granularity.md`](0018-spawn-granularity.md)
- [`0019-native-tool-process-cost.md`](0019-native-tool-process-cost.md)
- [`0021-evidence-tag-roundtrip.md`](0021-evidence-tag-roundtrip.md)
- [`0022-loadsession-reality-matrix.md`](0022-loadsession-reality-matrix.md)
- [`0023-compaction-signal-matrix.md`](0023-compaction-signal-matrix.md)
- [`0024-non-git-add-project.md`](0024-non-git-add-project.md)
- [`0025-acp-plan-live-wire.md`](0025-acp-plan-live-wire.md)
