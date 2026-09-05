# Appearance preview restoration

Status: source complete; browser verification assigned to pool_reliability.

## Seam and implementation

Released desktop-v0.5.22 Appearance controls extend existing Crew preferences and the existing shared SegmentedControl. Link and thread controls preview locally while scrubbing; window blur clears temporary state; selection commits through existing setters. Samples stay grouped with their controls and inherit SettingsOptionRow responsive layout.

- `desktop/src/features/settings/ui/AppearanceSettingsControls.tsx`: restore released LinkPreviewSample, ThreadLayoutDiagram/Preview, segmented link/thread controls. Inert sample uses inline SVG and the existing renderer; no external media or active lightbox. Illustrations use Crew muted/background/foreground tokens, preserving Crew palette and existing conversation/font/density/glass/accent controls.
- `desktop/src/shared/ui/link-preview-attachment.tsx`: extract released pure LinkPreviewAttachmentPresentation. Keep saved-preference/media-rewrite wrapper and Crew PR hub subject/context/action unchanged.
- `desktop/src/shared/ui/rich-link-preview-attachment.tsx`: released optional showExpandControl defaults true; false hides generic/tweet expansion buttons without hiding content. Keep Crew container-query sizing.

No new settings store, source file, or preview-only network pipeline. Controls remain 684 lines (existing file was 557), below repository 1000-line guard.

## Verification

- TypeScript compile PASS: `/tmp/crew-appearance-tsc.log`.
- Biome check/write of three owned files PASS; diff-check PASS.
- Existing link/thread preference tests: 5 PASS, `/tmp/crew-appearance-preference-tests.log`.
- Real React SSR diagnostic: 8 render cases PASS across generic/tweet rich default/true/false and compact; false preserves content while default retains expand control. `/tmp/crew-appearance-renderer-tests.log`.
- E2E build PASS: `/tmp/crew-appearance-build.log`.
- Pool owns unchanged three `appearance-previews.spec.ts` browser scenarios and screenshot review. Root owns independent source review.

Docs impact: minor; restored released functionality already included in upgrade scope. Root owns aggregate upgrade docs.

## Adjacent independent review

Root's `agentConfigControls.tsx` label fix reviewed: nonblank discovered options and pending selected fallback use existing resolveModelLabel with provider; IDs/order/descriptions/default/custom logic preserved. Distinct discovered names remain authoritative. Existing label/capability/controls tests: 29 PASS, `/tmp/crew-agent-model-label-review-tests.log`.

Unresolved questions: none.
