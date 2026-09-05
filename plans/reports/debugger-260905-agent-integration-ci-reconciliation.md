# Agent integration CI reconciliation

## Source-backed changes
- Authored relay availability is independent of the local managed main runtime. `AgentRuntimeAvatarControl` shows lifecycle controls from `isActive`; Online/Away can come from another thread worker. Tests now retain Start for a stopped local main process while checking authored profile/member presence independently. Member-menu keyboard activation starts exactly one runtime while presence remains Away.
- Profile primary action uses `start_managed_agent` (`UserProfilePanel.tsx` → `useStartManagedAgentMutation`); member menu uses relay-scoped `start_managed_agent_runtime`. Tests assert each real boundary.
- Presence rendering uses semantic `bg-attention` and `bg-success` tokens. Avatar no-remount, offline semantics, and shared presence request count remain asserted.
- Provider deletion copy now describes remote deletion and possible orphaned deployment. Behavioral assertions remain: unknown availability sends shutdown first, failed shutdown preserves record/membership, known Offline deletes without shutdown, successful retry proceeds in order.
- Team avatar outline uses `bg-card`. Test resolves the selected theme's opaque card color instead of assuming white, retaining overlap geometry, inset/radius, mask, and border/shadow checks.
- Catalog instructions are exact text in `persona-catalog-exact-instructions`, preserving raw instruction review; old chat Markdown selector was obsolete. Overflow checks remain.
- Unknown publisher fixture now signs with a deterministic non-registry key and derives its valid pubkey, exercising the unresolved-profile fallback with a valid event.
- Welcome kickoff uses Honey and Pollen, matching the shipped starter definitions.

## Actual header fix
Crew's third header action, Archived Hermes profiles, exceeded the old 40rem compact breakpoint.
At a 650px container / 16px root font, browser measurement found actions 503.84375px, 16px gap, copy 130.15625px, description 72px (three lines).
Updated only the two AgentsView container toggles from 40rem to 48rem. No controls removed or truncated.
Browser test now verifies full actions and single-line description at 800px; at 650px, defaults/stop/archives remain available through the compact menu with single-line description and focus restoration.
Existing wide desktop defaults case also passes.

## Validation
- Original 13 CI failures reproduced locally in integration project against immutable E2E bundle.
- Reconciled subset: 13/14 pass in 23.4s; remaining profile test corrected its IPC assertion to the actual source-backed command and rerun separately.
- TypeScript compile passes; focused Biome checks pass.
- Logs: `/tmp/crew-agent-integration-triage.log`, `/tmp/crew-agent-integration-reconciled.log`, `/tmp/crew-agent-presence-final.log`, `/tmp/crew-agent-header-measure.log`.
- Fresh immutable bundle: `/tmp/crew-release-e2e-dist-agent-integration`.

Docs impact: minor (AgentsView responsive bug fix; changelog entry appropriate).

## Open questions
None.
