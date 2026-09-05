# Projects overview search and selection review

Status: DONE. Source review and final mounted edge-case evidence complete. No implementation edits.

All three findings resolved in the current source:

1. Projects/Repositories list selection restored. ProjectsView builds visible filtered/sorted ranges using projectListSelectionItem and repositoryListSelectionItem, then passes selectionRangeItems to both list rows. Canonical identities match the row builders. ProjectItem consolidation preserves list/grid props and grid layout. Existing grid cards have no selection implementation.
2. Discussion destination membership enforced at the final write boundary. useProjectDiscussInChannel reads the current channels query cache at click time and returns false before writing a draft or navigating for missing membership/loading state. ProjectSelectionDiscussAction disables unavailable candidates and awaits both join and membership refetch before selection. ChannelBrowserDialog awaits its join callback, so newly joined membership is available even before React commits a rerender.
3. Failed discussion preserves selection. useProjectRepositoryDiscussion returns false on loading/error/unavailable destination and returns the final callback result; ProjectsSelectionCountMenu clears only when that result is not false. Existing void callbacks remain compatible.

Search review: existing token matcher handles case-insensitive multiword matching; filtering precedes preserved owner/access/sort behavior, ChannelsList receives the query, query/scope reset clears stale selection, and outcome search retains existing project outcome cards. No separate search correctness finding.

Validation: root's mounted search test passes 1/1 (1.6s), verified in /tmp/crew-root-global-search-test.log. Owner TypeScript check reported passing. The final project-review browser suite passed 40/40 in 1.5m (/tmp/crew-pr-review-final40.log), including unavailable-destination, successful join, pending membership, keyboard/range selection, search-negative and scope-reset coverage. Full desktop unit rerun after these source changes passes 7013 with one existing skip (7014 total), zero failures, 148 suites, 98.57s; /tmp/crew-final-search-selection-desktop-tests.log. The final source census is recorded in 260905-final-source-coverage.json.

Post-unit directory follow-up independently reviewed: ProjectSelectionDiscussAction uses the existing demand-enabled useOpenChannelDirectoryQuery and member-authoritative mergeOpenChannelDirectory pattern from AppShellOverlays. The merged discovery superset feeds only the channel browser; direct candidates and final write guard stay member-only. Successful join awaits membership refetch before selecting. Official source TypeScript passed; focused mounted membership/join coverage passed within the final 40-test browser suite. This follow-up was also included in the later final full unit run: 7031 passed, one existing skip (7032 total), /tmp/crew-final-virtual-ack-desktop-tests.log.

Unresolved questions: none. Pending work is validation evidence, not a source review finding.
