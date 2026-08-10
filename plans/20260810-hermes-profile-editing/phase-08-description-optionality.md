---
phase: 08
title: Description optionality + layer copy
status: planned
priority: medium
effort: S
dependencies: [02]
---

# Phase 08 — Description optionality + layer copy

Issue #118 thing-to-solve 3. The Crew agent description becomes explicitly
**optional**: empty means no Layer-3 injection at all.

## Honest starting point — most of this already works

| Fact | Evidence |
| ---- | -------- |
| An empty description already normalises to `None` | `desktop/src-tauri/src/managed_agents/types.rs:119` (S-5) |
| `None` already removes the env var rather than setting it empty | `desktop/src-tauri/src/managed_agents/runtime.rs:680-682` (S-4) |
| `None` already yields no system-prompt field in `session/new` | `crates/buzz-acp/src/pool.rs:255` (S-2) → `acp.rs:2355` (S-1) |
| The definition dialog does not require a description to submit | `AgentDefinitionDialog.tsx` submit gate |

So this phase is **contract-proof + copy + docs**, not a new mechanism. Saying
so is the point: the plan does not invent work to fill a DoD box. What is
genuinely unverified is which `SystemPromptTransport` variant the Hermes adapter
path takes — P01/Q5 answers it and E-11 pins it.

## The three layers (the model the UI must teach)

| Layer | Owner | Source | Applies to |
| ----- | ----- | ------ | ---------- |
| **L1 `SOUL.md`** | the Hermes profile | `~/.hermes/profiles/<name>/SOUL.md` | every place the profile runs |
| **L2 base prompt** | the harness | `crates/buzz-acp/src/base_prompt.md` (`lib.rs:1942`, S-3) | every Buzz-managed ACP agent |
| **L3 Crew description** | Crew | `system_prompt` → `BUZZ_ACP_SYSTEM_PROMPT` → `session/new` | this Crew agent only, when non-empty |

`layer3: "append"` is generic — it holds for Claude Code and Codex too. Only L1
is Hermes-specific.

## Work

1. **Prove the contracts** (E-10, E-11, E-12) rather than assume them. If any
   fails, that is a real bug this phase fixes at the seam that broke.
2. **Label the field optional** at `AgentDefinitionDialog.tsx:826-839` and the
   instance-edit equivalent (`AgentInstanceEditDialog.tsx:715`). Today it reads
   "Agent instructions" with placeholder "Describe what this agent should do." —
   nothing tells the founder it is optional, or that leaving it empty is the
   right choice for a Hermes agent whose persona lives in `SOUL.md`.
3. **Explain the layering in one line of helper copy** next to the field, and
   point Hermes users at the `SOUL.md` editor from P07.

## Files

| Path | Owner | Change |
| ---- | ----- | ------ |
| `desktop/src/features/agents/ui/AgentDefinitionDialog.tsx` (upstream, over ratchet) | upstream | **≤6 lines** — optional label + helper mount |
| `desktop/src/features/agents/ui/AgentInstanceEditDialog.tsx` (upstream, over ratchet) | upstream | **≤6 lines** — same |
| `desktop/src/features/agents/ui/AgentInstructionsHelper.tsx` | **new, Crew-only** | the layer explanation, capability-aware (mentions `SOUL.md` only when `personaDoc === "soulMd"`) |
| `crates/buzz-acp` tests | upstream crate, tests only | E-11 payload contract |
| `desktop/src-tauri/src/managed_agents` tests | upstream, tests only | E-10 env contract |

## Non-goals held

- No change to the `BUZZ_ACP_MODEL` strip-at-spawn guard.
- No change to the Layer-3 injection path for non-empty descriptions (E-12).
- No new event kind, no new HTTP endpoint — this rides existing ACP contracts.

## Turns green

E-10, E-11, E-12, E-17.

## Verification

```bash
cargo test -p buzz-acp system_prompt
cargo test --manifest-path desktop/src-tauri/Cargo.toml system_prompt
cd desktop && pnpm test:e2e:smoke
```

Manual (issue § Verification): create a Hermes agent with an **empty**
description, inspect the `session/new` payload, and confirm no system-prompt
field is present.
