# Desktop message, notification, and huddle port verification

Work context: `/Users/a1241968/Desktop/Oscar/crew-wt/upstream-0522`.
Released source: `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15` (`desktop-v0.5.22`). No worker commits, index mutations, or pushes.

## Message integration

- Resolved message conflicts semantically; retained Crew workspace binding/context, thread orientation/workbench models, role/avatar presentation, exact captured send targets, edit-as-undo cancellation, and extracted complete-send hook.
- Integrated paired mention-caret settlement, addressed-agent controls, frozen background link previews, acknowledged first-DM sends, canonical channel/message path links, reusable thread controls/skeleton chrome, date labels, and metadata grouping.
- Captured `forceRest`, `transport`, and root identity reach the send mutation. Cached verified reply roots win; captured roots remain fallback for uncached parents and optimistic rows. Relay-returned root identity remains authoritative after acceptance.
- Background preview preparation precedes atomic REST send; relay acceptance precedes queued wakes. Revoked or reference-only recipients cannot trigger wakes. Preview cancellation publishes and wakes nothing.
- Fixed an eager-clear merge regression: text-only sends resolve fresh workspace context before clearing, then publish. Context failure leaves the existing draft untouched. Retained upload-start clearing/recovery. Source guard updated for the extracted clear helper; a mounted send-hook regression drives a real failing workspace lookup.
- Extended the released audience store with explicit thread identity and initial verified agent recipients. Workbench and thread panel share canonical thread scope; explicit null/empty roots remain unresolved. Root-tag initialization remains supported. Migrated only the explicit old enablement preference when the new key is absent; old persisted recipients are not imported across communities.
- External forge commit previews may omit a relay repository announcement; only repository share-link UI is conditional. No fabricated repository model.
- Extracted status rendering to `message-status-metadata.tsx` (49 lines), retaining Sending, edited, withdrawn, and already-read outcomes.

## Notification / huddle audit

- `AppShell.helpers.ts`: activation ordering, rejection containment, cancellation, and best-effort reveal already release-equivalent. Crew wiki/workbench routes retained.
- Notification formatting, sender resolution, and event/feed targets already release-equivalent. Exact event/root anchors and neutral unresolved-sender copy retained. Crew installs focus/visibility redrain listeners before the initial macOS activation drain; no regression to later installation.
- Huddle level contexts and mic analyser already implement the release's render isolation, with Crew's latency behavior and theme tokens intact.
- Imported missing `068a83b097` PTT initialization/resync hunks. Starting/joining in push-to-talk now initializes muted, matching native `manual_mic_unmuted = false`; mode changes synchronize visible mute state to the native STT gate. Mounted tests stop before opening audio and verify both start/join plus resync. Existing high-frequency level isolation tests remain green.

## Evidence

- Full messages suite after workspace preflight correction: **1375 passed**, `/tmp/crew-messages-post-workspace-fix.log`.
- Final focused app/notifications/huddle/workspace/send-order suite after the no-clear/PTT corrections: **193 passed**, `/tmp/crew-audit-final-tests.log`.
- Whole desktop `tsc --noEmit`: **pass**, `/tmp/crew-audit-final-tsc.log`.
- Scoped Biome and `git diff --check`: pass.
- Final formatted message sizes for central baseline tightening: MessageComposer **1026**, MessageRow **1012**, MessageThreadPanel **984**. HuddleContext **980**.
- Standalone ACP idle default and README updated to **1500 seconds**, preserving tracked-tool **2400** and native pool **1800**. Config tests **119 passed**, `/tmp/crew-acp-final-config-tests.log`.

Docs impact: minor — ACP README plus this implementation/audit report. Root owns release/changelog integration and final repository gate/UI review.

Unresolved questions: none.
