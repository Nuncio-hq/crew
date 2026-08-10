---
phase: 01
title: Spike 0015 — readiness signals and archive mechanics
status: planned
priority: P0
effort: S (0.5-1 d)
dependencies: []
---

# Phase 01 — Spike 0015: readiness signals and archive mechanics

## Why this phase exists

D-008 requires a spike before implementation when the mechanism is
unknown. Four things are genuinely unknown and every one of them would
otherwise be guessed inside an implementation PR:

1. Which `config.yaml` conditions honestly mean `broken-config` (OQ-3).
2. Where archives live — the "existing backups area" the issue cites
   **does not exist** in this repo (OQ-1, plan DD-5).
3. What an archive costs and in what format (OQ-2).
4. What can cheaply trigger readiness re-evaluation after a healthy
   spawn (OQ-4).

This phase produces evidence and a decision record. **No production
code.**

## Deliverable

`docs/crew/spikes/0015-profile-readiness-and-archive.md` — next free
spike number (highest existing is `0014-agent-attention-recovery.md`).

## Investigation items

### I1 — `config.yaml` shape and failure modes (→ OQ-3)

- Read a healthy Hermes v0.20.0 profile under
  `$HERMES_HOME/profiles/<name>` (or `~/.hermes/profiles/<name>`).
- Enumerate the exact keys Hermes requires to start a session with a
  model (`model.provider`, `model.default`, others).
- Fault-inject on a scratch profile and record actual Hermes behavior:
  truncated YAML; valid YAML with no model block; valid YAML with a
  nonexistent model id.
- **Decide** which of these Crew classifies as `broken-config`, biasing
  conservative: a false `broken-config` blocks a working agent and is
  strictly worse than the boolean it replaces (plan R7). Unparsable YAML
  is the floor; anything above it needs evidence here.
- Record whether an invalid *model id* is detectable locally at all
  (it may only be knowable to the provider — if so, it is **not**
  `broken-config`, and the issue's mention of it must be recorded as
  not-locally-detectable rather than silently implemented).

### I2 — Binary probe cost and caching (→ readiness latency risk)

- Time `hermes --version` cold and warm.
- Confirm the resolved command path the desktop app sees, reusing the
  PATH augmentation already applied in
  `desktop/src-tauri/src/managed_agents/discovery.rs`.
- Propose a TTL and the invalidation points; note that
  `commands/agent_discovery.rs:295-330` and `:425-455` already
  re-evaluate readiness post-install (seam S17).

### I3 — Archive location (→ OQ-1)

- Confirm by search that no `NuncioCrew Backups` area exists (expected:
  it does not).
- Enumerate candidate locations: the app data dir
  (`com.nuncio.crew`), a sibling `…/profile-archives/`, or a
  user-visible path.
- Weigh: discoverability by the owner vs. accidental sync to iCloud vs.
  permission model. `desktop/src-tauri/src/util.rs:86,242,275` shows the
  existing restricted-permission backup helpers — reference for file
  mode, not a reusable archive area.
- **Decide and record one location.** Later phases cite this record;
  no later phase may hardcode a path first.

### I4 — Archive format and size (→ OQ-2)

- Measure a real profile: total size, and size after excluding
  `audio_cache/`, `image_cache/`, `logs/` and any other transient
  directories actually found on disk (do not assume the issue's list is
  complete — enumerate what is there).
- Compare `tar.gz` (needs a crate) against a plain recursive copy (no
  new dependency, larger on disk, trivial restore).
- **Decide** the format that meets "archives stay small" with the
  smallest dependency delta, and record the definitive exclusion list.
- Record how a pre-action **size estimate** is computed cheaply, since
  the issue requires showing it before archiving.

### I5 — Re-evaluation trigger (→ OQ-4)

- Determine what can cheaply detect post-spawn breakage: turn-boundary
  check, filesystem watch on the profile dir, or a timer.
- Cost each; note that a filesystem watch adds a dependency and a
  per-agent handle.
- **Recommend** one, with the fallback being turn-boundary (cheapest,
  no new machinery).

### I6 — Running-agent liveness read (→ phase 04 guard)

- Identify the authoritative in-process read for "this agent has a live
  runtime pair", starting from
  `desktop/src-tauri/src/managed_agents/runtime/stop.rs` and
  `runtime_commands.rs:313` (`stop_managed_agent_runtime`).
- **Confirm and record** that no SIGTERM→wait→SIGKILL graceful stop
  exists: `stop.rs:40,120,153` use `Child::kill()`; the only `SIGTERM`
  in `managed_agents` is `discovery.rs:927`, inside the auth-probe
  timeout. The spike record is where this correction to the issue's
  premise is captured for reviewers (plan DD-4).

## Files

- **Create:** `docs/crew/spikes/0015-profile-readiness-and-archive.md`
- **Read only:** `managed_agents/hermes_profile_lifecycle.rs`,
  `managed_agents/readiness/hermes.rs`, `managed_agents/readiness.rs`,
  `managed_agents/runtime/stop.rs`, `managed_agents/discovery.rs`,
  `commands/agent_discovery.rs`, `src-tauri/src/util.rs`
- **Must not touch:** any `desktop/src/**` or `src-tauri/src/**` source

## Validation

- Every open question OQ-1 through OQ-4 has a recorded decision with
  the evidence that produced it (measured numbers, observed CLI output,
  or a cited `path:line`) — not a preference.
- I1 states explicitly whether an invalid model id is locally
  detectable.
- I6 states explicitly that graceful stop does not exist today.
- Spike record links the spike-0010 auth-probe ask so `auth-unknown`
  has a durable citation for DoD #5.
- `just ci` green (docs-only change, but the gate is not skipped).

## Risk and rollback

- **Risk:** the spike concludes that `broken-config` is not reliably
  detectable beyond unparsable YAML. That is a legitimate outcome —
  phase 02 then ships the narrower honest state and the plan records
  the reduction rather than faking depth.
- **Risk:** no acceptable archive location exists without a new
  user-facing surface. Escalate to the issue owner before phase 04
  rather than inventing one.
- **Rollback:** delete the spike record. No product surface changes.

## PR

Docs-only PR to `Nuncio-hq/crew` (D-020), branch
`agents/profile-lifecycle-spike`. `git commit -s`. Does not change
shipped state, so `docs/crew/STATE.md` is not required by #117 for this
phase — every later phase does require it.
