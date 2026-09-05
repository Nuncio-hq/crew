# Desktop CI reconciliation

Work context: `/Users/a1241968/Desktop/Oscar/crew-wt/upstream-0522`; PR #342.

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
- Final desktop JS gate:7013 tests,7012 passed,0 failed,1 existing skipped in98.6s (/tmp/crew-final-js-gate-pool.log). Final full Projects8/8 passed23.5s (/tmp/crew-ci-projects-complete.log). All desktop check guards, tsc --noEmit and git diff --check passed. Immutable build /tmp/crew-release-e2e-dist-access-guard. No worker commits/index/push/CI reruns.

Docs impact: minor; source-backed fixes and semantic test updates recorded here. No unresolved questions.

## Reviewed screenshot evidence

- `desktop/test-results/projects-v3-screenshots/08-access-help-channel-draft.png`: scoped real composer after hidden repository falls back to its accessible linked project. Canonical repository chip preserves `tab=commits`; focused case passed3.6s. SHA256 `ae72583b34c4fca598f6522136e9fe377d26c3cd781ffff0e320055440b317e4`.
- `desktop/test-results/projects-v3-screenshots/01-workspace-overview.png` and `03-issue-detail.png`: inspected outcome/context and issue comment surfaces.
- `desktop/test-results/thread-pr-hub/14-submitted-context-collapsed.png` and `15-submitted-context-expanded.png`: distinct collapsed/expanded submitted repository context.
- `desktop/test-results/crew-wiki/10-file-citation.png`: repository file citation and highlighted range.
- `desktop/test-results/appearance-previews/*.png`: four restored link preview/thread preview layouts.

All listed screenshot hashes are distinct; no relay media hosting used. Source and test edits frozen after final evidence capture.
