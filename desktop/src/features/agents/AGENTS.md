# Agent Configuration — Contributor Rules

Scope: `desktop/src/features/agents/` (config surfaces, shared config renderer,
and the agent config core). Read this before changing how harness / provider /
model / effort configuration is modeled, rendered, persisted, or applied.

Plan of record: `Buzz/Harness-Provider-Model.md` in Morgan's Obsidian vault
(PR sequence, decisions log). PRs: #2140 (rename), #2148 (flag reduction),
#2156 (honest model states), #2158 (Agent Config Core).

## The one rule

**Harness capability facts have exactly one source: the Rust runtime catalog.**
`KnownAcpRuntime` (`desktop/src-tauri/src/managed_agents/discovery/runtime_metadata.rs`)
declares each harness's model/provider/effort env keys and capabilities. Spawn
applies them; `AcpRuntimeCatalogEntry` exposes them over IPC; and
`lib/agentConfigCore.ts` projects them into field descriptors. The frontend
never maintains a rival copy of this table. Setup guidance follows the same
rule: `requires_external_cli` is derived from `KnownAcpRuntime` and projected
to the UI rather than inferred from a runtime ID in a component.

**Preset-only capabilities** that have no `KnownAcpRuntime` row (Cursor, and
other PATH-probed presets) are derived in
`shared/api/runtimeCapabilities.ts` (`deriveRuntimeCapabilities`) from the
catalog entry id — same place Hermes' `personaDoc` already lives. Today that
covers `selectableAutoModel` for Cursor (persists model id `"auto"`, spawn
injects `cursor-agent --model auto`). Do not add `runtime.id === "…"` checks
in dialog render code; extend `deriveRuntimeCapabilities` instead.

**Second metadata source: command-keyed execution policy.**
`harness_max_parallelism` (`managed_agents/parallelism.rs`) maps the harness's
static command string to a spawn-time cap (`OPENCLAW_MAX_PARALLELISM = 5` for
OpenClaw). This cap is not a `KnownAcpRuntime` field because it applies to
preset harnesses (like OpenClaw) that are not in the builtin catalog. It is
projected onto `AcpRuntimeCatalogEntry.max_parallelism` by all four
catalog-producing constructors (builtin discovery, preset catalog, custom
discovery, custom-save response) using the **static definition command**, not
the resolved `entry.command` (which may be `null` for unavailable entries).
The frontend reads `maxParallelism` from the catalog entry and never keeps a
separate constant.

If you need a new capability fact (a new env key, a native option, a "supports
X" flag): add it to `KnownAcpRuntime` first, expose it on
`AcpRuntimeCatalogEntry`, then project it through the core. Preset-only facts
go through `deriveRuntimeCapabilities`. Do not shortcut with a TypeScript
lookup table or an id comparison in a component.

## Rules

1. **No hardcoded harness-ID checks in render code.** `runtime.id === "claude"`
   belongs in `deriveAgentConfigFieldModel` (once, with a named reason), never
   in a component. Components ask the field model what exists
   (`hasRenderableAgentConfigField`, `getRenderableEffortField`).
2. **Effort reads/writes go through the descriptor.** Use the effort
   descriptor's `currentPersistence` key — never a raw
   `BUZZ_AGENT_THINKING_EFFORT` literal in UI code. `currentPersistence` is
   where the value lives *today*; `targetApplication` is how the harness
   *should* receive it. They intentionally differ until PR 2.7 migrates
   Goose/Claude — do not "fix" one to match the other without doing the
   migration work.
3. **Field absence has a named reason, not a boolean.** Model resolution is a
   capability: `modelSource` selects profile write-through versus adapter
   settings. Codex effort is `ownedByModelId`; Claude effort is
   `deferredUntilNativeOptionsAvailable`; `ownedByProfile` is retained only
   for runtimes that genuinely cannot expose a model, not for Hermes'
   write-through path. New absences get new named reasons in
   `AgentConfigOmission` / `render` — never a `showX` prop.
4. **The clearing policy is the named types.** `onContextChange:
   "resetDependentValues"` (user changed harness/provider → dependent values
   reset everywhere) vs `onCatalogMismatch: "explainOnly" | "onboardingCleanup"`
   (an async catalog miss never silently erases saved state outside
   onboarding's named cleanup). Do not add mutation booleans like
   `clearInvalidModel`; extend the policy types.
5. **"Metadata unknown" ≠ "harness lacks the capability".** Passing
   `runtime: undefined` to the core means fields won't render. Surfaces must
   gate on the runtime catalog query settling (loading/error states) rather
   than letting fields silently vanish — see `AgentDefaultsEditor` /
   `DefaultConfigStep` for the pattern.
6. **One canonical behavior, disclosure presets for visibility.** Behavior
   flags were deliberately killed in #2148 (`CANONICAL_CONFIG_BEHAVIORS`).
   Surface differences are expressed via the `disclosure` preset, not new
   boolean props.  **Exception:** `onboarding-essential` hides happy-path
   helper copy (provider/model descriptions) but a non-null model-discovery
   status always bypasses the preset and renders the status line — enforced
   via `shouldShowModelStatusMessage()` (`AgentConfigFields.tsx`).
   Additionally, a successful discovery response that yields no usable options
   (`supportsSwitching:false` or empty model list) synthesizes a warning status
   via `synthesizeEmptyDiscoveryStatus()` and is intentionally **not cached**
   so that closing → reopening the dialog re-runs discovery after the user
   installs or signs into the CLI (`isCacheableDiscoveryResponse()`).
   Hermes profile binding (`hermesProfile` field kind) is projected only for
   `definition`/`instance` scopes when `profileArg` is set — create/edit
   render it; global defaults do not. Create/edit submission must clear a
   stored profile when the prospective runtime no longer advertises
   `profileArg`; hidden form state must never preserve a binding. Profile
   occupancy is installation-wide, not relay-scoped: one managed-agent record
   owns runtime pairs for every configured community, while its `relayUrl` is a
   legacy pin ignored by effective relay resolution. Bind one local Hermes
   profile to one local record and disclose that its memory, skills, and state
   are shared across the record's configured communities.
7. **Onboarding setup detects readiness; it does not select defaults.** The
   setup page derives visible and ready harnesses from the runtime catalog and
   only offers install or sign-in actions. The following defaults page is the
   sole onboarding surface that chooses `preferred_runtime`. Its complete draft
   lives in machine-onboarding session state, so Back performs no write and
   restores even incomplete edits when the user returns. Skip abandons that
   draft and advances with zero config writes. Next is the only persistence
   boundary: it consumes the shared renderer's `onValidityChange` signal,
   disables editing while awaiting `set_global_agent_config`, advances only on
   success, and leaves the draft in place with a retryable inline error on
   failure. A harness selection alone does not enable Next when the harness
   requires provider/model/credential config (e.g. buzz-agent with no
   provider). Baked build env and runtime-file config satisfy the gate. Drafts
   intentionally do not survive an app restart.
   `onboarding-agent-defaults.spec.ts` is the acceptance gate for anything
   touching this flow or the shared renderer.
8. **Omit the Model control only after a confirmed successful empty
   discovery on an optional-model harness.** When the field model marks model
   as `acpNative` (Claude Code / Codex), `shouldRenderModelControl` hides the
   picker while discovery is in flight and after IPC resolves with no usable
   options (`modelDiscoverySuccessfulEmpty` / `isSuccessfulEmptyDiscovery`).
   A thrown or unavailable discovery keeps the control so #2246 failure UI can
   render, and must not heal/clear persisted model or effort. Full disclosure
   still shows the control when Custom model is available. Required-model
   harnesses always keep the field. **Separately**, `ownedByProfile` omits the
   model control before discovery runs — that path does not use
   `shouldRenderModelControl`. A persona document renders only when the runtime
   declares one, and render code still never branches on `runtime.id`. Gate:
   `defaults hides model when optional
   harness has empty discovery` (and the failed-discovery counterpart) in
   `onboarding-agent-defaults.spec.ts`; profile-owned model in
   `lib/agentConfigCore.test.mjs` + `ui/agentConfigFieldsContract.test.mjs` +
   `hermes-profile-binding.spec.ts`.
9. **The defaults modal is progressively disclosed.** An unset global config
   starts on the Buzz Agent-first deployment fallback and carries that visible
   harness into the next saved edit. The `progressive-defaults` disclosure
   preset therefore begins at Provider for Buzz Agent, then reveals Model,
   Effort, and Advanced only after a provider is configured. Harnesses whose
   runtime metadata has no provider field skip that gate. Reveals animate their
   height through Motion and become immediate when reduced motion is requested.
   Once the Advanced toggle is visible, its expanded state is exclusively
   user-controlled: provider, harness, and required-env changes must never
   open it automatically in defaults, create, or edit flows. In Create mode,
   `Run on` belongs in Advanced directly after **Who can send instructions**;
   keep it out of the basic create fields. The defaults summary follows
   preferred-harness changes saved while the dialog is open, and its configured
   state includes required credentials as well as provider/model values. If no
   available harness can resolve, Create starts in Customize and lets unavailable
   catalog entries be selected only to expose their setup guidance; submission
   remains blocked.
   Advanced-only required credentials and incomplete remote **Run on** setup
   mark the collapsed Advanced toggle without opening it, and block incomplete
   saves.
   Runtime-file credentials satisfy Global Defaults just as they do Create and
   Edit. In Edit,
   selecting Custom command keeps its required command field beside the harness
   picker rather than hiding it in Advanced.
10. **Catalog visibility is community-scoped relay state, never a global
    definition field.** `AgentDefinition.shared` is only the active
    relay+owner projection returned to the UI. Durable heads and pending
    publications live in the scoped retention database, and explicit share
    toggles await relay acceptance before the UI claims that an agent was
    published or removed. A queued update must stay visibly queued, and the
    catalog itself must render only relay-confirmed publications — never an
    optimistic local persona.
11. **Shared agent access names the consequence where it is selected.** The
   shared respond-to field shows a persistent warning whenever `anyone` **or**
   `allowlist` is selected — both hand the host's access to someone other than
   the owner, so both disclose it and only the audience phrase differs. This
   covers persona-backed create and edit surfaces. Keep that disclosure in
   the shared field instead of adding surface-specific flags. It renders
   directly below the selector for `anyone` but *after* the people picker for
   `allowlist`, so it never sits between the user and the selection they came
   to make. The copy leads with the audience ("Anyone can use this agent to
   access…") so it reads as a warning rather than an explanation, and stays one
   sentence — don't split the mechanism into a second sentence. Both the machine
   and the stakes it names come from `lib/agentAccessWarning.ts`, keyed on an
   optional `runLocation`: instance surfaces resolve it from
   `ManagedAgent.backend` via `runLocationForBackend`, and the create flow from
   `WhereToRunDraft.runOn` via `runLocationForRunOn`. `AgentDialog` is the one
   place that resolves it for dialog surfaces and publishes it through
   `ui/AgentRunLocationContext.tsx`; the field reads that context and lets an
   explicit `runLocation` prop win. Do **not** thread the value as a prop
   through `AgentDefinitionDialog` / `AgentInstanceEditDialog` — neither uses
   the value itself, and the shared context keeps the dialog boundary stable.
   Surfaces rendered outside `AgentDialog` (e.g. `EditRespondToDialog`) pass the
   prop directly. Local names "your
   computer, including files, accounts, and connected tools"; remote names "the
   server it runs on, including any accounts and tools available there" —
   deliberately *not* the owner's files, which aren't theirs to describe on a
   host they don't own. **An unknown location falls back to the local wording —
   never hedge with "computer or server".** A remote host requires an
   installed `buzz-backend-*` provider, and without one `WhereToRunSection`
   never renders, so "server" would name a concept the owner has never been
   shown; when it *is* remote they picked that host from the selector
   themselves. Never synthesize a run location a surface doesn't have. Don't
   expose `respond-to`, `allowlist`, Nostr, or harness jargon in primary UI
   copy. **The owner-only-access build capability is backend-independent.** When
   `getAgentAccessOwnerOnly()` is true, every managed agent's access control is
   locked to owner-only, including provider-backed agents. A provider backend
   does not prove remote execution and must never create a policy carve-out.
12. **Shared instructions must be reviewable byte-for-byte.** Agent definitions
   execute their `system_prompt` verbatim, so catalog and snapshot review
   surfaces render the literal prompt, never the chat Markdown projection
   (which can conceal spoilers, link destinations, and image sources). Reject
   Unicode default-ignorable, bidirectional-formatting, and non-layout control
   characters at both the untrusted catalog parser and the Rust persistence /
   import boundary. Do not silently strip them: rejection keeps the reviewed
   string identical to the executed string. New sharing paths must reuse the
   same validation before they persist or activate a definition.
13. **Profile runtime sections render only reported agent data.** Missing
   runtime, model, status, command, MCP, advanced, or diagnostics values stay
   absent in every build mode. Do not fill profile or agent-panel gaps with
   development/staging examples, preview controls, or synthetic configuration;
   those values can be mistaken for the viewed agent's real configuration.
   Configuration rows show the effective value regardless of whether it came
   from an explicit choice, global default, config file, or runtime override.
   Do not add provenance lines, shadowed/struck-through values, pre-start
   placeholders, or whole-section dimming; use an em dash for an unknown value.
   Info, activity, agent-configuration, and model-setting rows use the same bare
   16px leading-icon treatment as agent management actions. Keep semantic icons
   visible in profile variants and do not wrap them in background shapes. An
   owned agent profile is entry-point invariant: opening the same deployed
   agent from Agents, a DM, or a channel must expose the same actions, tabs,
   fields, and profile-wide activity selection. Caller context may control the
   panel shell or return navigation, but must not filter or replace profile
   content. Explicit public-key targets are always exact, including stopped,
   archived, and relay-only identities. Only explicit persona navigation may
   select a representative or offer persona Start; a relay persona link cannot
   borrow a local sibling's management controls. See
   [the identity contract](../../../../docs/agent-profile-identity.md).
   Availability dots read relay presence, never a saved deployment
   receipt or runtime status. Failed/disconnected reads are unknown; lifecycle
   actions retain their separate routing. Presence does not grant Stop authority or constrain Crew
   thread-worker parallelism; lifecycle controls retain their scoped ownership checks. Deletion also consumes that same exact-key availability reader:
   unknown requests shutdown when a channel exists, request failure retains the
   record, and only established Offline keeps the intentional no-request path.
   Unqueried persona siblings are unknown. No presence state grants deletion or
   Stop authority; native local stop-before-remove remains independent. See
   [the availability contract](../../../../docs/agent-availability.md).
   The shared cloud marker means “Not managed on this device” only
   after ownership and successful local inventory are known. It does not imply
   hosting location, availability, or permission. Keep all identity surfaces on
   the shared provenance context, without per-row directory subscriptions. See
   [the provenance contract](../../../../docs/agent-management-provenance.md).
14. **Thinking effort has two surfaces: a local-only WRITE control and a
   read-only two-facts DISPLAY.** The write control is `EffortPickerField`
   (`ui/EffortPickerField.tsx`), a self-contained section component mounted in
   `AgentInstanceEditDialog` beside the Model block. It is **Save-gated, not
   direct-write**: the control is fully controlled by the parent dialog
   (`value`/`onChange`) and owns no mutation. The dialog persists the selection
   by embedding `effortLevel` in the locked `update_managed_agent` IPC call, so
   the effort write is atomic with any access-policy change and can never race
   or survive a Cancel or failed Save. There is no standalone
   `persistAgentEffortLevel` setter. Its gating and option compute live in the
   pure helper `ui/effortPicker.ts` (`effortPickerState`): the picker renders
   only when `agent.backend.type === "local"` **AND** a `thought_level`
   `effortConfigId` has been discovered from the running session (absent
   pre-first-session and for runtimes/models without effort support). Local-only
   is load-bearing, not cosmetic — the Rust command rejects non-local backends
   because remote effort is set at deploy time via `policy_env`. Because the
   control reads its inputs from the config surface the dialog already fetches
   (`useAgentConfigSurface`), it integrates into the dialog's existing field
   group without additional IPC. The read-only display is the `thinkingEffort`
   normalized field rendered by `AgentConfigPanel` via `NormalizedRow`, which
   already shows both facts — `field.value` (canonical, the effort the next
   spawn will launch with) and, when a running ACP session differs,
   `field.overriddenValue` struck through (the live session's current effort).
   No component owns "configured vs current" logic; the reader's canonical tier
   ordering feeds both facts. Do not add a second effort write path or restate
   the two-facts logic in a component.

15. **Owner-only builds discover only verified same-owner remote agents.**
    The native `list_relay_agents` boundary authenticates ownership through the
    agent's NIP-OA profile, then retains only agents with verified ownership
    when the compiled owner-only capability is present. The frontend applies
    the active-user policy so same-owner agents on another machine remain
    mentionable while other-owned agents stay hidden. OSS builds retain the
    complete policy-filtered relay directory. Local `agents-data-changed`
    events refresh only local persona/team/managed-agent caches; they must
    never invalidate the remote relay directory.

16. **A hidden field never blocks Save, and a visible incomplete one explains
    itself.** `agentAiConfigurationSubmitBlockReason` derives the submit-block
    copy from the same visibility inputs the fields render from
    (`needsProviderSelection` / `needsModelSelection`), so a provider or model
    the runtime metadata hides can't silently disable the footer. Dialogs pass
    the reason to the footer instead of computing their own message.
17. **Definition edits seed access from the instance being edited.** A
    definition's stored `respondTo` / `respondToAllowlist` can lag behind the
    managed-agent instance enforcing it, so `editPersonaDialogState` takes the
    linked instance's access and prefers it — a definition edit must not
    re-widen access the owner already narrowed on the instance. Conversely,
    `personaManagedAgentUpdate` pushes a non-null definition policy down to its
    linked instances (allowlist only when the mode is `allowlist`); a null
    definition policy is "unspecified" and never overwrites instance access.
18. **Archived identities are omitted from forward-looking discovery, not from
    the archive surfaces.** `buildUnifiedGroups` / `pickProfileAgent` take the
    `useIsArchivedPredicate()` predicate and drop archived standalone and
    unknown-definition instances, and skip archived instances when resolving a
    definition card's navigation target (returning `undefined` when every
    linked instance is archived). Matched definition groups keep their full
    instance list so the card still represents them, and the predicate's
    fail-open-while-loading and self-exempt behavior is load-bearing — don't
    reimplement archive checks locally.

19. **The persona `description` is public display metadata.** It is optional,
   capped at 280 characters, and validated through the shared visible-text
   policy (`validate_agent_description_text` in `definition_validation.rs`)
   on the raw authored bytes at create/update, snapshot import, publication,
   inbound sync, and the untrusted catalog parser — rejected, never stripped.
   It is deliberately EXCLUDED from `persona_content_hash`
   (`description_change_does_not_change_content_hash`), so a description-only
   edit never flips the restart badge on linked instances. Only the AUTHORED
   description exists — there is deliberately no derived/generated fallback;
   a blank description publishes an empty kind:0 `about`, exactly as before
   the field existed. Agent and team snapshots carry the authored description
   in the member profile's `about` and validate it before import. The trim/empty
   resolution exists twice and must stay in
   sync (port changes in the same PR): `lib/agentDescription.ts`
   (`effectiveAgentDescription`) feeds display surfaces, and its Rust twin
   (`managed_agents/agent_description.rs`, `effective_agent_description` /
   `record_effective_description`) feeds the publish path, where
   `profile_needs_sync` compares `about` (None == empty) so description edits
   reconcile instead of being clobbered. Persona-linked instances do not own a
   second description copy; snapshot export materializes the definition value
   only into the portable snapshot, and a dangling link resolves no description
   rather than reviving stale instance metadata. The agents-page card face shows the
   authored description as its second line, falling back to the model label
   when none exists (`UnifiedAgentsSection.tsx` composes it;
   `AgentIdentityCard` takes a presentational `subtitle`). The community catalog
   shows the same authored description before consent: a clamped two-line list
   subtitle for scanning and the full safely wrapped value in persona detail.
   The dialog field
   lives in `ui/AgentDescriptionField.tsx` (`AgentIdentityFields`), not
   inline in the over-1000-line dialogs.

20. **Databricks model discovery has one shared catalog authority.** Desktop and ACP call the shared `buzz-agent` discovery library; Desktop passes the effective merged `DATABRICKS_MODEL_FILTER` explicitly, and the library applies it to raw workspace endpoint IDs and Unity Catalog model-service FQNs after the additive union. A successful filtered-empty catalog is authoritative: it stays empty, disables switching, and never falls through to configured or known-model fallback. UC FQNs are catalog data and always use the MLflow Chat Completions route, regardless of family-looking text in their components. Global Defaults preserves the discovered model ID as the selected value while its closed trigger renders the provider-scoped display label; do not force the raw persisted ID over that label.

## Thread-scoped runtime controls

Crew observer controls preserve `conversationId` and optional `turnId` so one
logical agent can run multiple thread workers. Stop acknowledgements echo the
optional `requestId`; match the control type, channel, conversation and request
before claiming success. Relay delivery alone is not a confirmed cancellation.
A queued conversation can be drained without an active turn. Do not replace
these controls with channel-only routing or create a new session as a fallback.

## The tests that enforce this

- `lib/agentConfigCore.test.mjs` — field model per harness × scope, clearing
  policy, `hermesProfile` / `ownedByProfile`. Update when the capability model
  changes.
- `ui/agentConfigFieldsContract.test.mjs` — canonical behaviors + disclosure
  presets + `shouldShowModelStatusMessage` status-bypass +
  `shouldRenderModelControl` (successful-empty omit vs failure keep) +
  profile-owned informational row contract.
- `ui/hermesProfileBindingContract.test.mjs` — client validation + capability
  helpers for profile binding.
- `ui/usePersonaModelDiscovery.test.mjs` — `synthesizeEmptyDiscoveryStatus`,
  `isCacheableDiscoveryResponse`, `deriveModelDiscoveryPending`,
  `isSuccessfulEmptyDiscovery`. If the "reopen to retry" copy becomes inert
  again, these tests will catch it.
- `ui/respondToFieldContract.test.mjs` — plain-language mode labels, the
  persistent warning contract for shared agent access, and its two render
  positions (after the people picker for `allowlist`).
- `ui/unifiedAgentGroups.test.mjs` — archive-aware grouping and definition
  target selection, including the fail-open predicate and all-archived cases.
- `ui/agentAiConfigurationPolicy.test.mjs` — submit-block reasons vs hidden
  provider/model fields.
- `ui/personaDialogState.test.mjs` — instance-seeded access in edit state.
- `features/profile/ui/UserProfilePanelUtils.test.mjs` — definition→instance
  access sync, including the null-policy no-op.
- `lib/agentAccessWarning.test.mjs` — every mode × run-location copy variant
  plus both resolvers, including unknown-reads-as-local and
  blank-`runOn`-is-not-a-provider.
- `desktop/tests/e2e/onboarding-agent-defaults.spec.ts` — onboarding behavior
  acceptance coverage for readiness, failure states, defaults, session-draft
  restoration, zero-write Skip, Next save failure/retry, navigation, and
  successful-empty vs failed optional-model discovery.
- `desktop/tests/e2e/hermes-profile-binding.spec.ts` — create/edit binding
  field visibility, validation, profile-owned model row, duplicate-bind error,
  create-in-place button, delete keep/delete choice (defaults to keep).
- `desktop/tests/e2e/agents.spec.ts` — community catalog descriptions remain
  visible in the list and full detail before Add agent, including long
  unbroken Unicode text without horizontal overflow.
- `lib/agentDescription.test.mjs` — authored-description resolution: trim,
  blank/missing → null.
- Rust: `runtime_metadata_env_vars` tests pin spawn-time key application.
- Rust: `hermes_profile_lifecycle` tests pin create/delete against a fake
  `hermes` + temp `HERMES_HOME` (including the spike 0011 exit-0-but-present
  trap).
- Rust: persona sharing/retention tests pin relay+owner scoping, durable
  enqueue errors, relay rejection/unavailability, and accepted publication.

## Keep this file true

**If you change how agent configuration is modeled, rendered, persisted,
applied, or cleared — update this file in the same PR.** A rule that no longer
matches the code is worse than no rule; a new pattern that isn't written down
here will be broken by the next agent that never learns it existed. Reviewers:
treat a config-behavior diff without a matching AGENTS.md diff (or an explicit
"no rules changed" note) as incomplete.
