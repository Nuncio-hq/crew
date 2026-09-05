# Desktop CI reconciliation

Work context: `/Users/a1241968/Desktop/Oscar/crew-wt/upstream-0522`; PR #342.

## Latest source-freeze status

Independent final desktop unit run: **7031 passed, one existing skip, zero failures**
(7032 total, 98.23s), `/tmp/crew-final-virtual-ack-desktop-tests.log`. Latest late-fix
review: [independent desktop reconciliation](reviewer-20260905-final-desktop-reconciliation.md).
Repository selection, contextual thread reading and actual channel-home navigation,
plus faithful native publication and folder-first Save coverage, are recorded in
[project reconciliation](tester-20260905-project-commit-reconciliation.md).

Root's final seven-spec browser selection passed **91/91, retries zero, 4.5m**
(`/tmp/crew-root-final-seven.log`): aging, onboarding, relay, smoke, thread-head,
Nostr, and overscroll coverage. Counts overlap earlier focused selections.
The official source TypeScript check and six source-guard commands pass.
Full official `pnpm check` passed on the final candidate including the Virtua
acknowledgment patch, formatted tests, native fixtures and channel directory: 3190 files, all six guards, three
existing warnings and five infos; `/tmp/crew-root-final-ack-desktop-check.log`. Supplemental ad-hoc
E2E TypeScript compilation is outside the configured source gate and is not
claimed clean; the repository's test files are excluded from its source tsconfig.

The final Projects review selection passed **40/40 in 1.5m** on local source
checkpoint `36410d3f6` (`/tmp/crew-pr-review-final40.log`). It includes search,
keyboard/range selection, safe drafts, unavailable membership, immediate join
and pending-membership recovery. The restored native local-snapshot/lazy-read
pair passed **2/2 in 5.7s** (`/tmp/crew-pr-review-native-final2.log`); these cases
also appear in the 40-test suite. See [test-intent review](reviewer-20260905-project-review-test-intent.md).
The final full unit run includes the open-directory production follow-up, restored
native test fixtures and Virtua acknowledgment patch. Independent source reviews
and focused browser results also cover these changes. Linux smoke shard 3 completed **329 total: 324 passed, four existing skips and
one Nostr startup flake in 14.2m** (`/tmp/crew-pool-linux-shard3.log`). All 40
Projects review cases passed on their first attempt. The test-only consent event
listener readiness fix then passed the complete Nostr spec **11/11, retries zero,
19.5s** on Linux (`/tmp/crew-pool-linux-nostr-ready.log`), plus the original
failing case repeated three times **3/3 in 7.3s**, retries zero
(`/tmp/crew-pool-linux-nostr-ready-repeat.log`), and 33/33 across three
macOS repeats (`/tmp/crew-root-nostr-listener.log`). This focused verification does
not relabel the original 329-case run as clean. The Linux container is Ubuntu
Noble arm64 with Playwright 1.60; GitHub runner architecture differs.
The test-only readiness follow-up is local commit `3d46956e1`.
The final virtualization correction passed the complete 11-case browser suite
on macOS (40.2s) and Linux (53.7s), retries zero, no initial failures or skips.
Logs: `/tmp/crew-pool-final-virtualization-mac.log` and
`/tmp/crew-pool-final-virtualization-linux.log`. Both used the uninstrumented
`/tmp/crew-planner-virtua-ack-final-dist`. The two focused Linux cascade/reader
cases repeated three times passed 6/6 in 1.7m with no retries. Final source
checkpoint: `5d11998fe`. See [virtualization parity](tester-260905-final-virtualization-parity.md)
and [cascade investigation](debugger-260905-virtualization-cascade-linux.md).

Gate/manual workflows passed on `0ef5491fc7faee1f19f109c0e4a5a7c7ae4b0890`.
The whole CI run was cancelled after smoke shards 3/4 timed out with failures.
Later repairs are locally verified. [PR #342 checks](https://github.com/Nuncio-hq/crew/pull/342/checks) are the canonical remote evidence. Merge requires full NuncioCrew CI (including every desktop smoke shard) and manual upstream compatibility to pass on the exact source head being merged.
The detailed selections below are historical evidence, not a cumulative final total.

Projects selection and filtered-search screenshots were published through the
required screenshot helper after distinct-hash review:
[PR screenshot evidence](https://github.com/Nuncio-hq/crew/pull/342#issuecomment-5551722821),
immutable image commit `800d5feafbb8c9194a53b192a779d0bd73a7e82e`.
This publishes only screenshot assets; it does not establish a new source revision
or a passing remote check for a source revision.

## Verified fixes and fixture reconciliation

- Badge + muted-light-channel: pin explicit `crew-light` fixture; ThemeProvider intentionally migrates legacy themes and pins Crew accent blue. Badge still checks exact applied primary, count and contrast; mute still tests exact 0.5 light opacity, dark companion retained. Three focused cases pass.
- Agent stopped error + positive relay presence: preserve Crew lifecycle/availability separation. AgentRuntimeAvatarControl explicitly keeps local stopped runtime error actionable despite an independent thread worker's Online/Away relay presence. Test now observes Online/Away DM avatar, error action, exact agent Runtime panel and close. Pass.
- DM cache reseed collector: recognize `send_channel_message` alongside legacy websocket EVENT; actual publisher uses HTTP bridge. Navigation-away and stale reopen checks retained. Pass.
- Empty channel intro: current create-agent action test ID replaces removed add-agent ID; all card content, layout and actual Add agents flow checks retained. Pass.
- Doctor layout: shared SettingsOptionGroup aligns section headers at flex-end; card count, order, geometry, install/auth assertions retained. Pass.
- Huddle callback: native websocket bridge now delivers arrays; test direct IPC callback consumes frame batch instead of ignoring all frames. Silent writes persist and emit no live data; default root/reply/summary still exactly asserted. Pass.
- Copy-link rail: narrow thread hides quick reactions/divider by container query. DOM action order still checks all buttons; visible reaction/reply order, exact canonical clipboard links through both surfaces retained. Pass.
- Profile hover: Crew profile and sidebar action buttons share sidebar-border/35, channel row uses a different token. Compare real shared sidebar action hover sample. Pass.
- Pairing: SettingsOptionGroup moved visual card styles to inner settings-section-card; retain 16px radius, nontransparent background and entire pairing lifecycle assertions. Pass.
- Navigation: scope get_event count to the exact welcome event instead of unrelated profile/background hydration, preserve count=1. Current message chip uses Open message in channel general and no visible # prefix. Both root-open/reopen cases pass.
- Restart contrast: explicit Crew Light fixture, named attention token expectations; preserved WCAG4.5 and geometry. Found actual contrast4.435; root scoped CSS fix passes original threshold.
- Wiki: file entity links are buttons; retain real citation navigation, auto-reveal, highlight, project Wiki tab and company flows. Added repeated native deep link to same file lines2-3 after original1-3 and verify line1 unhighlighted. Root/ACP route+request-key fixes pass entire flow4.2s.
- Projects discovery: accessible owner-filter group and Crew outcome heading/cards replace old upstream ProjectsOverview IDs. Repository availability + outcome content cases pass.
- Project snapshot fixture: restored released projectRepoSnapshotDelayMs/Error handling dropped from get_project_repo_snapshot branch. Centered loading animation/disappearance passes; restricted owner/error now renders.
- Submitted context: after planner actual-send replacement passed2.9s, relocated stale ProjectsOverview panel case to thread-pr-hub actual explicit Reviewer mention/send fixture. Raw hidden UUID context persists; visible transcript hides it until collapsed pill expands and hides again. Added distinct collapsed/expanded screenshots. With root/ACP shared renderer fix passes6s batch with contrast.

## Verified production cases from other owners

- Appearance three released preview behaviors + two model display labels: all5 pass8.7s on immutable ci-reconciled bundle.
- Messaging seven preview cases: planner reports7pass19.6s; later day-divider + single-level-thread2pass4.6s.
- Persistent audience mention button: passes unchanged.

## Final follow-up validation

- Owned full smoke sweep: 192 total, 186 passed, five failures, one existing skipped; follow-up fixes addressed channel welcome/chip copy, description-bridge readiness, intentional push-to-talk starts-muted behavior, and an actual late-autofocus popover dismissal.
- Huddle voice cause: first click opened/focused TTS switch, then late composer autofocus stole focus and Radix dismissed the menu. Production useComposerAutofocus now respects focused dialog/menu/listbox descendants while preserving sidebar-navigation autofocus. Mounted regressions 4/4 passed; entire huddle spec24/24 passed37.6s, original early click retained with no focus wait. Root independent review passed.
- Fresh CI five-case reconciliation: shared selection-formatting popover colors, native FileCard download before/after edit with exact URL/filename, Crew text-only inbox zoom, OSS cross-owner allowlist actual send, Crew owner-only denied mention/send. All five passed6.6s.
- Projects restored ACP production repository action panel, Channels list, create-task/PR dialogs. Broad screenshot case passes12.9s batch with huddle early click. Preserved clone/source/fetch layout, issue/PR comments and metadata, list row density, contributors, Channels, existing create dialogs. Removed obsolete upstream detached pod/resizable embedded agent chat geometry because Crew uses outcome page plus ordinary channel discussion. Original context-send contract has a separate real explicit Reviewer mention/send test in thread-pr-hub, preserving raw UUID context and collapsed/expanded transcript.
- Eight Projects screenshot artifacts have distinct SHA256 hashes. Inspected workspace overview and issue detail images; actual Crew outcome, repository action controls, comments and details rendered.
- Restricted access fixture preserves hidden repo and owner invite copy. ACP production guard prevents discussion navigation into inaccessible linked channels and selects only accessible repository/project/selection candidates. Added independently configured projectHomeChannelId fixture for valid accessible project fallback while repo binding stays inaccessible. Hidden case and accessible fallback now under final browser validation.
- Final desktop JS gate:7031 tests,7012 passed,0 failed,1 existing skipped in98.6s (/tmp/crew-final-js-gate-pool.log). Final full Projects8/8 passed23.5s (/tmp/crew-ci-projects-complete.log). All desktop check guards, tsc --noEmit and git diff --check passed. Immutable build /tmp/crew-release-e2e-dist-access-guard. No worker commits/index/push/CI reruns.

Docs impact: minor; source-backed fixes and semantic test updates recorded here. No unresolved questions.

## Reviewed screenshot evidence

- `desktop/test-results/projects-v3-screenshots/08-access-help-channel-draft.png`: scoped real composer after hidden repository falls back to its accessible linked project. Canonical repository chip preserves `tab=commits`; focused case passed3.6s. SHA256 `ae72583b34c4fca598f6522136e9fe377d26c3cd781ffff0e320055440b317e4`.
- `desktop/test-results/projects-v3-screenshots/01-workspace-overview.png` and `03-issue-detail.png`: inspected outcome/context and issue comment surfaces.
- `desktop/test-results/thread-pr-hub/14-submitted-context-collapsed.png` and `15-submitted-context-expanded.png`: distinct collapsed/expanded submitted repository context.
- `desktop/test-results/crew-wiki/10-file-citation.png`: repository file citation and highlighted range.
- `desktop/test-results/appearance-previews/*.png`: four restored link preview/thread preview layouts.

All listed screenshot hashes are distinct; no relay media hosting used. Source and test edits frozen after final evidence capture.
