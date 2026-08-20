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
- [`0026-gh-pr-hub-graphql.md`](0026-gh-pr-hub-graphql.md)
- [`0027-tauri-multiwebview-macos.md`](0027-tauri-multiwebview-macos.md)
- [`0028-idb-vs-baguette.md`](0028-idb-vs-baguette.md)
- [`0029-canvas-tooling-key.md`](0029-canvas-tooling-key.md)
- [`0030-pty-dev-server-port.md`](0030-pty-dev-server-port.md)
- [`0031-webview-js-bridge-snapshot.md`](0031-webview-js-bridge-snapshot.md)
- [`0032-hidden-webview-pane-closed.md`](0032-hidden-webview-pane-closed.md)
- [`0033-describe-ui-ref-stability.md`](0033-describe-ui-ref-stability.md)
- [`0034-sim-tap-e2e-latency.md`](0034-sim-tap-e2e-latency.md)
- [`0035-org-roster-ingest.md`](0035-org-roster-ingest.md)
- [`0036-kickoff-gate-latency.md`](0036-kickoff-gate-latency.md)
- [`0037-budget-cutoff-turn-start.md`](0037-budget-cutoff-turn-start.md)
- [`0038-org-reorg-race.md`](0038-org-reorg-race.md)
- [`0039-wiki-page-planning-quality.md`](0039-wiki-page-planning-quality.md)
- [`0040-wiki-incremental-regen.md`](0040-wiki-incremental-regen.md)
- [`0041-wiki-mermaid-pipeline.md`](0041-wiki-mermaid-pipeline.md)
- [`0042-wiki-generator-model-plumbing.md`](0042-wiki-generator-model-plumbing.md)
- [`0043-work-tree-eligibility-selector.md`](0043-work-tree-eligibility-selector.md)
- [`0044-work-tree-disclosure-live-arrival.md`](0044-work-tree-disclosure-live-arrival.md)
- [`0045-needs-you-aggregation-dedupe.md`](0045-needs-you-aggregation-dedupe.md)
- [`0046-token-coverage-audit.md`](0046-token-coverage-audit.md)
- [`0047-cursor-dark-aa.md`](0047-cursor-dark-aa.md)
- [`0048-appearance-migration-mapping.md`](0048-appearance-migration-mapping.md)
- [`0049-container-query-support.md`](0049-container-query-support.md)
- [`0050-letter-soup-heuristic.md`](0050-letter-soup-heuristic.md)
- [`0051-p1-grep-precision.md`](0051-p1-grep-precision.md)
- [`0054-org-product-removal-entry-points.md`](0054-org-product-removal-entry-points.md)
- [`0055-call-by-name-mention-wake.md`](0055-call-by-name-mention-wake.md)
- [`0056-hermes-default-profile-acp-spawn.md`](0056-hermes-default-profile-acp-spawn.md)
- [`0057-tool-pane-live-mac-stability.md`](0057-tool-pane-live-mac-stability.md)
