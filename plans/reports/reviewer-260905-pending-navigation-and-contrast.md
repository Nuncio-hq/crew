# Pending navigation / contrast review

## Findings resolved
Repository citation matching incorrectly lowercased the full coordinate. `entityLink.ts` preserves case-sensitive repository d-tags, while the newly used Wiki guard equated `owner:Crew` and `owner:crew`. Confirmed with a source experiment: valid parsed `Crew` route matched `crew`.
Root authorized correction: strip kind prefix and normalize owner hex only; preserve the complete d-tag suffix, including embedded colons. Helper tests cover case-distinct identifiers and uppercase owner hex; mounted RepositoryFilesPanel regression verifies a case-distinct pending citation remains unconsumed.
Validation: nine helper/mounted tests pass; TypeScript and Biome pass.

## Other reviewed behavior
- AppShell now installs the existing combined message/entity deep-link hook with the same main-window-only guard; message navigation stays included.
- Entity activation produces a fresh navigation token carried through router state to ProjectDetailScreen/WorkspaceTabs. Repeated same-URL activation bypasses route deduplication but still passes the existing navigation guard.
- File citations explicitly request Files and the requested path. RepositoryFilesPanel waits for matching repository/file, consumes only a matching available citation, and reapplies line ranges on repeated activation without requiring remount.
- Restart contrast adjustment targets only the restart action under Crew Light. Default Start/error colors and other themes retain their established tokens. CSS specificity applies the tiny-label correction without changing lifecycle routing.

## Reviewer separation
Messaging ownership transfer was authored by this reviewer and is not claimed as independently reviewed here. Pool reviewer was asked to inspect it independently. Its mounted-hook/store and browser-navigation evidence was previously reported.

## Open questions
None in this reviewed scope after the repository-coordinate fix. Pending unrelated project-list integration work remains owned by root.
