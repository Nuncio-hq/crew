# Phase 02 — Crew UI: binding, readiness, no-model guard (Slice 2)

- **Status:** Phase 02A (Rust) + Phase 02B (TypeScript UI) COMPLETE
- **Contracts:** C-03, C-04, C-05, C-06, C-10, C-12 (see feature doc §8)
- **PR scope:** 02A Rust core; 02B desktop field model + create/edit + E2E

## Deliverable

Crew-side UX that makes the D-019 conventions enforced instead of
documented:

1. **Profile binding field** on create/edit when the runtime catalog
   projects `profileArg` — validated text field (Phase 03 adds list/
   create). Manager `default` rejected. → **02A storage + 02B UI DONE**
2. **Model/provider suppression** (C-04): named omission
   `ownedByProfile`; informational "Model: decided by profile \<name\>".
   Live session model deferred (no clean dialog read path). → **02B DONE**
3. **Spawn guard** (C-05/C-06) → **02A DONE**
4. **Readiness classes** (C-03/C-12 degraded) → **02A DONE**; config nudge
   copy for `hermesProfile` → **02B DONE**
5. **Duplicate-binding guard** (C-10) → **02A server reject**; **02B**
   surfaces the server error message inline on edit save (no pre-warn
   query helper shipped in 02A)

## Design decisions locked in 02A (A–F)

| ID | Decision |
| -- | -------- |
| A | `KnownAcpRuntime.profile_arg: Option<&'static str>` — only Hermes is `Some("-p")`. Projected as `AcpRuntimeCatalogEntry.profile_arg` / TS `profileArg`. One-rule compliant. |
| B | `ManagedAgentRecord.hermes_profile: Option<String>` (serde default/skip). Validate `^[a-z0-9][a-z0-9_-]{0,63}$`; reject `"default"`. |
| C | Args injection in `resolve_effective_harness_descriptor`: prepend `[flag, name]` before normalized args; skip if flag already present; ignore binding when runtime has no `profile_arg`. |
| D | `strip_model_env_for_profile_locked_runtime` after last user-env write; hash uses post-guard view. |
| E | Hermes readiness: binary + bound profile. No dir existence / no auth probe. |
| F | Duplicate binding: server-side reject on create/update (same relay). |

## 02B decisions / deviations

| Topic | Choice |
| ----- | ------ |
| Capability gate for model ownership | `profileArg` + `providerLocked` + no `modelEnvVar` (projected from Rust). Never `runtime.id === "hermes"` in components. |
| Omission reason | `ownedByProfile` on `AgentConfigOmission` |
| Live model in info row | Omitted for now — label is "decided by profile \<name\>" without "currently \<model\>" |
| Duplicate bind UX | Inline `edit-agent-save-error` with server message (02A reject) |
| File-size ratchet | Extracted `HermesProfileBindingFields`, `EditAgentModelAndProfileSection`, `createHermesBindingFields`, `AgentDefinitionCustomAiFields` — no limit bumps |
| Tiny Rust IPC | `ManagedAgentSummary.hermes_profile` + catalog `provider_locked` projection (needed for capability check) |

## Exit criteria

- 02A: Rust contracts GREEN — **met**
- 02B: C-04 UI + contract tests + smoke E2E — **met** (this commit)
- Phase 03: profile list/create lifecycle remains open
