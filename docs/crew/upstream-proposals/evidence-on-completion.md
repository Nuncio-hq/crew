# Evidence on completion — upstream proposal draft

> Draft artifact held under D-020. This is not a submitted pull request.

## Evidence on completion

- Bugfix → failing test (before) → passing (after), excerpted text.
- Performance → before/after numbers such as time, query count, or memory.
- UI/visual → before/after capture.
- Refactor → `git diff --stat`, green CI, and a behavior-preserving note.
- New feature → new tests passing, plus a capture of the new state if visual.
- Docs/config → diff or link.
- No cheap proof exists → state plainly what is unproven and how to verify it.
- Capture in place while the work is visible; text-first; excerpt, don't dump.
- Be proportional: never add a decorative screenshot for compliance.
- Do not use computer interaction to manufacture evidence when the task does not
  require it.

This generic proposal is intentionally kept in Crew as a draft because D-020
forbids proposing, drafting, or opening upstream pull requests for this work.
