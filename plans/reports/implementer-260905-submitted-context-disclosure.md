# Submitted context disclosure

Status: source complete; root review requested; pool owns actual-send browser adaptation.

## Existing Crew seam

Crew prepends generated Markdown reference definitions to explicit-agent messages in its normal channel/thread composer. `ThreadComposerViewContext` supplies the current thread/workspace selection; `useMentionSendComplete` appends it after workspace resolution. These signed definitions stay hidden in normal Markdown. The released pill was mounted only inside the unused legacy Projects prompt page and its legacy trailing-context parser cannot parse Crew's leading definitions.

## Minimal implementation

- `project-view-agent-context.ts`: add extractCrewSubmittedAgentContext for anchored generated UUID reference definitions. Require matching workspace/view URL shape and usable scope or repository/path. Read at most one of each generated kind. Return the exact signed prefix, never rewrite content.
- `MessageRowDefaultBody.tsx`: use existing ProjectAgentSubmittedContextPill above normal Markdown for the current user's own raw signer (fallback display author only when raw signer absent). Shared channel/thread rendering preserves existing navigation and bindings. Evidence/handover rendering unchanged.
- `ProjectAgentSubmittedContextPill.tsx`: optional className allows pl-0 when inside an already-indented message body; default legacy spacing unchanged.
- Existing project-view context contract tests cover exact byte preservation, stacked/single definitions, prose/fence/reference lookalikes, malformed scope/URL, escaped metadata, and duplicate-kind stopping.

No message tags, event IDs, signed content, sender authorization, routing, navigation, composer preparation, or thread binding changed. No legacy Projects prompt/chat surface restored. Pre-send context preview is separate and not claimed by this patch.

## Verification

- TypeScript compile PASS: `/tmp/crew-submitted-context-tsc.log`.
- Biome and diff-check PASS.
- Context contract/wiring tests: 29 PASS, `/tmp/crew-submitted-context-tests.log`.
- E2E build PASS: `/tmp/crew-submitted-context-build.log`; immutable output `/tmp/crew-release-e2e-dist-submitted-context` (not final entity-navigation evidence; root subsequently edits that separate path).
- Pool can reuse `openProjectThread` in `thread-pr-hub.spec.ts` to send an explicit Reviewer Agent mention against a real thread workspace model; verify native sent payload and collapsed/expanded disclosure. Browser result pending.

Docs impact: minor; root owns aggregate upgrade docs.

Unresolved questions: none.
