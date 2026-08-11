# Closing comment for #104

Closing #104 after **#134 has merged**, per the founder-decided disposition
recorded in [PR #145](https://github.com/Nuncio-hq/crew/pull/145) and
[`plan.md §6a`](../plan.md#6a-decided-founder-2026-08-11). This comment is
intended to be pasted by the founder with the real replacement issue numbers.

## What shipped or is already covered

| Phase | Disposition | Evidence |
| --- | --- | --- |
| 01 — trusted autonomy / local boundary | Shipped through #106/#107: owner-only and local enforcement, shared-profile visibility, and duplicate-binding protection are on main. | `desktop/src-tauri/src/managed_agents/hermes_profile.rs:60-70,140-208`; `desktop/src/features/agents/lib/hermesProfileBinding.ts:265`; `desktop/src/features/agents/ui/HermesProfileBindingFields.tsx:78,106-118` |
| 02 — Hermes Doctor | #134 covers the named readiness states and profile/binary preflight; the remaining Doctor slice is R1. | `desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:20-39,59-80,87-149` on PR #134 / branch `agents/profile-lifecycle-hardening` |
| 03 — Needs You | The mechanism is shipped on main: form normalization, same-turn answer handling, owner-only authorization, and distinct attention projection are implemented and tested. Live-Hermes certification moves to R2. | `crates/buzz-acp/src/elicitation.rs:1543-1617,1426-1457`; `crates/buzz-acp/src/acp.rs:1586-1647`; `desktop/src/features/agents/agentAttention.ts:30-40,188-192`; `desktop/src/features/agents/agentAttention.test.mjs:145-172` |
| 04 — Project Runner | The deterministic worktree substrate is engine-generic on main; Hermes certification moves to R2. The durable branch/commit/PR result criterion belongs to #121/#128, not this issue. | `docs/crew/STATE.md:118-127`; `crates/buzz-acp/src/thread_workspace.rs:29-61`; `crates/buzz-cli/src/commands/evidence.rs:1-64` on PR #128 / branch `devin/1786360062-evidence-thread-log`; `crates/buzz-acp/src/base_prompt.md:32-34` |
| 05 — profile custody | #134 covers Crew archive/restore/permanent-delete and refusal while running; the remaining import/readiness tail is tracked separately. | `desktop/src-tauri/src/managed_agents/hermes_profile_archive.rs:764-871`; `desktop/src-tauri/src/commands/hermes_profiles.rs:81-86,147-153,203-208` on PR #134 / branch `agents/profile-lifecycle-hardening` |
| 06 — capability view | Moved to R3, gated on S-B and the still-open Q2 ownership question. | `plans/20260810-hermes-profile-editing/plan.md:69-96` on PR #123 / branch `docs/plans-issues-117-121`; `crates/buzz-acp/src/config.rs:735-743` |

## Follow-up and standing ask

- **R1:** [#TBD — Hermes Doctor completion](issue-r1-doctor-completion.md)
- **R2:** [#TBD — live-Hermes certification records](issue-r2-live-certification.md)
- **R3:** [#TBD — effective capability view](issue-r3-capability-view.md)

C-12/auth truthfulness remains the standing Hermes-side ask: no headless
Hermes auth probe exists, and #134's honest `AuthUnknown` state is the interim
answer (`desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:35-38`
on PR #134 / branch `agents/profile-lifecycle-hardening`).

The original #104 “Crew does not configure model/provider” non-goal is
superseded by #149 / D-038. D-038 records Crew model/provider write-through,
keeps Hermes as the single source of truth, and carries the D-019 presentation
and `HERMES.md` rule-2 supersession
(`docs/crew/DECISIONS.md:493-511` on PR #149 / branch
`agents/hermes-profile-editing`). The #104 non-goal lived only in the GitHub
issue body and is moot on closure.

The STATE.md Hermes drift remains filed for #124; this closeout does not edit
`STATE.md`, `DECISIONS.md`, or `HERMES.md`.
