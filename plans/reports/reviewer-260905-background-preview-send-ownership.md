# Independent review: accepted background preview sends

Work context: `/Users/a1241968/Desktop/Oscar/crew-wt/upstream-0522`.

Status: DONE. No actionable correctness finding in the pending ownership delta.

Reviewed `useMentionSendComplete.ts` against its committed base, `useActivePreparedLinkPreviews.ts`, `linkPreviewPreparationStore.ts`, the mounted send/store tests, and the App/community reset boundary.

- Early composer clearing applies only after admission and readiness preflight, only plain sends with no explicit agent workspace context. Agent workspace resolution retains its previous clear-after-success boundary.
- Ownership transfer removes the accepted preparation from the composer's unmount cancellation set; the promoted send remains in the community store until finally/release. Same-community navigation therefore cannot silently drop an accepted plain send.
- The frozen destination/thread and sender callback remain captured. Authorization is revalidated immediately before signing/publish; the final publish guard still checks the promoted AbortSignal.
- Community reset aborts promoted sends and clears tasks/jobs. AppReady is gated by appliedKey matching current communityKey and remounted by key, so old components cannot become new-community send destinations.
- Cancelled/failed pending preview preparation restores a still-mounted empty composer without overwriting newer text; recovery persistence is suppressed after promoted cancellation. Final release runs regardless of outcome.
- Queued agent starts remain strictly after successful relay publication. Rejected publishes do not wake; cancellation racing an accepted publish does not discard the wake for a message already delivered.

Validation: independently ran the actual mounted `publish-before-wake.test.mjs` suite; 12 tests pass. Coverage includes plain early clear, fresh mention authorization, navigation survival, preflight cancellation, community cancellation, restoration, publication failure, and publish-before-wake ordering. Browser ownership case was separately run by implementation agent and reported passing; not counted as independent execution here.

Docs impact: minor, behavioral ownership clarification only. No source changes from reviewer. Unresolved questions: none.
