# Spike 0015 — Hermes profile readiness and archive mechanics

- **Status:** PASS (readiness decisions; Hermes runtime measurements
  unobtainable on this machine)
- **Date:** 2026-08-10
- **Question:** Which profile signals can Crew honestly classify before spawn,
  and which archive boundary should later offboarding phases use?

## Decisions

### OQ-3 — Conservative config parsing

Crew classifies `broken-config` only when `profiles/<name>/config.yaml` exists
but does not parse as YAML with the existing `serde_yaml` dependency. An
absent `config.yaml` is not broken. Missing required model fields and invalid
model IDs are deliberately not classified locally: Hermes v0.20.0 provides no
evidence-gatherable local contract for those checks here, and a false blocking
state is worse than the boolean it replaces.

No Hermes binary is installed on this machine, so the CLI behavior of
fault-injected profiles could not be measured here. This is a design narrowing,
not a measurement.

### OQ-2 — Archive format

Later archive work will use `tar.gz`, through the existing `tar` and `flate2`
dependencies, without adding a crate. A real Hermes profile size comparison
was not possible because no Hermes binary or profile fixture is installed here.

### OQ-1 — Archive location

The archive root is `<nest_dir()>/profile-archives/` (`~/.buzz/profile-archives`
in normal builds and `~/.buzz-dev/profile-archives` in development). It is
Crew-owned, created with mode `0o700` on Unix, and never placed inside
`~/.hermes`.

### OQ-4 — Re-evaluation trigger

Readiness is recomputed at every status projection (`status_for_with` and
`agent_summary`). The Hermes binary probe is cached for 60 seconds, with an
explicit invalidation after successful runtime installation. This surfaces
post-spawn breakage and clears repaired state without an app restart.

### I6 — Liveness correction

The issue's cited graceful-stop premise does not match the current
`managed_agents/runtime/stop.rs` call sites: stop uses `Child::kill()` and
`terminate_process` rather than implementing the cited SIGTERM → wait →
SIGKILL sequence there. The only direct SIGTERM use found in
`managed_agents/discovery.rs:927` is the auth-probe timeout. (The lower-level
Unix process helper contains signal escalation, but `stop.rs` does not expose
the issue's claimed graceful contract.)

## Evidence and limitations

- Existing lifecycle path resolution is in
  `managed_agents/hermes_profile_lifecycle.rs:440-468`.
- The readiness pipeline is `managed_agents/readiness.rs:314-356`, with
  Hermes-specific checks delegated to `readiness/hermes.rs`.
- The approved archive location is a sibling of the Crew nest, not a
  pre-existing “NuncioCrew Backups” area; repository search found no such
  directory.
- The Hermes headless-auth limitation and upstream ask are recorded in
  [spike 0010](0010-hermes-headless-auth-probe.md).

**Verdict: PASS** for the conservative readiness and archive decisions;
Hermes-specific measurements remain explicitly unobtainable in this
environment.
