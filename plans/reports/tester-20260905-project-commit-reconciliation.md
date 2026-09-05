# Project commit smoke reconciliation

Scope: project-commit-detail smoke failures from upstream sync. No commits, no new skips. Existing three upstream create-dialog skips retained.

- Restored repository selector on existing repository-change callback.
- Restored matched discussion thread reading in the existing conversation controller; compose-to-channel scoped drafts unchanged. Profile closes before opening a conversation. Project/repository reset clears conversation selection.
- Repository back/breadcrumb now navigates the actual linked channel.
- Native mock bridge implements publish_project_owner_announcement contract: owner/kind/dtag/time validation, rejection without storage, accepted event readback after lost acknowledgment.
- Tests follow current outcome/detail and channel-home routes. Separate folder-first create test exercises native folder selection boundary, actual repository save, signed event metadata, and duplicate rejection. Home controls and workspace/thread accessibility tests retain independent coverage.

Validation: TypeScript, Biome, file-size guard, diff check passed. Existing retained-review consumer unit test passed with repository reset callbacks supplied. Combined project-commit-detail + projects-v3-screenshots: **22 passed, 3 existing skips, 54.7 seconds**, zero retries. Smoke run log: /tmp/crew-project-commit-final.log. Built artifact: /tmp/crew-acp-final-context-pass-dist. ProjectDetailScreen: 998 lines.

Docs impact: minor; root owns consolidated upstream-sync/changelog update.

Unresolved: root independent review pending.
