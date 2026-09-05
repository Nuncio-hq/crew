# Wiki repeated file ranges

Status: source complete; root review requested; pool extends browser coverage.

## Root cause

The entity-link producer originally navigated only to the project root. Root restored file tab/path navigation and released entityNavigationId production. However, RepositoryFilesPanel consumed pending ranges only when its files array changed, so reopening the same file with a new range did not update the highlight. Its unscoped consume also allowed another repository panel to remove a pending citation.

## Minimal consumer fix

- ProjectWorkspaceTabs passes existing initialTabRequestKey as fileOpenRequestKey and actual project.id to RepositoryFilesPanel.
- The pending-file effect reacts to each request key, peeks with the repository guard, waits for the requested file to exist, then consumes with the same guard and updates selection/range.
- No workspace or file-panel remount, new store, route model, or navigation changes in this worker patch.

## Verification

- TypeScript compile PASS: `/tmp/crew-wiki-range-tsc.log`.
- Biome PASS.
- Existing ProjectWorkspaceTabs.test.mjs adds an actual mounted RepositoryFilesPanel regression. First range1–2 changes to4–5 on the same file and same DOM panel; another repository's request stays pending without changing the displayed range; its target repository consumes2–3; unavailable file stays pending until a later snapshot contains it.
- 8 focused tests PASS including Wiki coordinate normalization/store contracts: `/tmp/crew-wiki-range-tests.log`.
- Pool owns browser extension. Root owns producer changes in AppShell/useAppNavigation/entityLinks and independent review.

Docs impact: minor; root owns aggregate upgrade docs.

Unresolved questions: none.
