# Messaging preview CI reconciliation

## Result
- Seven previously failing messaging preview cases pass against fresh immutable E2E bundle (`/tmp/crew-ci-messaging-fresh.log`).
- Twelve mounted publish-before-wake tests pass, including preview ownership across actual component unmount and community reset (`/tmp/crew-preview-publish-regression.log`).
- Accepted-preview channel navigation browser regression passes (`/tmp/crew-preview-navigation-rerun.log`).
- Actual Reviewer mention/send submitted-context disclosure regression passes (`/tmp/crew-submitted-context.log`).
- TypeScript and focused Biome checks pass.

## Production corrections
- Standalone/thread composer uses the existing shared background upload/preview overlay, matching the existing ChannelPane integration.
- Text sends without explicit agent workspace requirements clear after admission/readiness and before pending preview preparation. Explicit-agent workspace resolution still precedes clear; mention authorization remains fresh immediately before publish.
- At plain-send acceptance, preparation ownership moves out of the component's unmount-cancel set. The existing community store retains its promoted-send controller and cancels on relay/community reset. Existing finally cleanup releases ownership.
- Cancelled preparation uses the existing failure restoration path. Aborted sends retain the existing prohibition on draft-store persistence: draft callbacks use the current global relay scope and writing a captured key after community switch could leak an old-community draft.

## Why earlier evidence missed ownership
- Released upstream's active preparation hook aborts all component-owned preparations on unmount, and its send path retains ownership until completion. Previous preview tests only waited in the original composer; none navigated or unmounted after acceptance.
- Earlier Crew text sends kept the draft visible until preview resolution. Moving clear earlier made unmount cancellation capable of losing the visible draft, so the accepted-send ownership transition required its own mounted-hook/store regression.
- Tests now distinguish accepted navigation (one publish to captured channel), pre-admission unmount (cancel/no publish), community reset (cancel/no publish/no foreign draft writes), and explicit mounted cancellation (original editor text restored).

## Test reconciliation
- Preview loading state belongs to the inner attachment element; update state selectors without weakening geometry checks.
- Cold resolver fixture now matches the exact first-paint test title.
- Pending Send is enabled; accepted send clears composer and supports Skip while retaining exactly-once publication.
- Image upload failure matches released metadata-only snapshot fallback; assert attempted image/favicon uploads and positional snapshot fields.
- Submitted context coverage uses Crew's actual channel/thread composer seam and outgoing UUID definitions, replacing obsolete page-specific fixture-only coverage after its replacement passes.

## Open questions
None. Legacy day-divider and inline-thread assertions now match rendered channel mention text and both pass (4.6s).
