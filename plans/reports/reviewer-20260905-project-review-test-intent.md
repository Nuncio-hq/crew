# Project review browser test intent

Status: DONE. Independent read-only review; no actionable findings in current test migration. Browser owner completed the final integrated suite: 40/40 passed in 1.5m, verified in /tmp/crew-pr-review-final40.log.

Compared project-pr-review.spec.ts against HEAD: 38 tests become 40; neither version has skip, fixme, or expected-failure annotations. Added conditional branches select deterministic section-specific controls; they do not bypass assertions based on observed product state. No catch-and-ignore or early-return path was added. requiredBox throws on missing geometry.

Preserved behavior:
- Review publication ordering, approve/request-changes, reviewer identity, draft/open/closed transitions, managed ownership, merge denial and terminal recovery remain substantive. Changed selectors follow Review/Task vocabulary and current collapsible diff sections.
- Missing-checkout coverage still invokes Fetch and Clone and checks clone_project_repository. Remote/local, immutable tags, branch deletion protection, lazy file previews, create-review and create-task tests remain.
- All four overview list types exercise keyboard focus/Space, Shift range selection, exact selected counts, discussion choices, channel browser, and clearing. List/header geometry and group checkbox alignment remain measured.
- Search runs all six sections, proves impossible-query zero results, positive results, repository exclusion, and Escape restoration. Section/query changes clear selection.
- Discussion preparation preserves an existing channel draft, deduplicates its entity link across repeats, and never sends automatically. Switching repository clears prior selected task context before preparing the new repository draft.
- Repository context collapse measures workspace expansion and restoration; resizing proves pointer-following width, zero transition duration during drag, and double-click reset.
- New negative membership coverage removes actual fixture membership and invalidates the query; unavailable and unjoined destinations preserve selection, route and drafts. Join invokes join_channel and prepares the draft. Pending membership uses a held native read, proves retained selection/disabled destinations, then releases and successfully prepares discussion.

Intentional surface migrations:
- Obsolete global detached agent-chat rails and activity-stat layouts are replaced by Crew outcome navigation, inline selection actions, and ordinary channel discussion drafts.
- Add-channel dialog coverage lives at project-commit-detail.spec.ts:397–417 on the actual channel home, using the same useAddProjectChannelMutation path. The earlier global dialog assertion only opened/dismissed it; the retained test preserves that obligation.
- Actual send and hidden submitted context disclosure remain in thread-pr-hub.spec.ts:536–600: native send payload, UUID-scoped context, collapsed transcript hiding, expand/re-collapse, and distinct screenshots. They are not inferred from draft-only tests.

Validation: final integrated suite 40/40 passed in 1.5m (/tmp/crew-pr-review-final40.log); the native local-source branch-round-trip and lazy-preview pair also passed 2/2 in 5.7s (/tmp/crew-pr-review-native-final2.log). These selections overlap. All 40 cases also passed first attempt in Linux smoke shard 3. The whole shard was 324 passed, four existing skips and one unrelated Nostr startup flake out of 329 total, 14.2m (/tmp/crew-pool-linux-shard3.log); it is not reported as a clean whole-shard pass. Official source TypeScript and the full desktop unit pass are recorded separately; ad-hoc E2E TypeScript compilation is not a configured passing gate.

Unresolved questions: none.
