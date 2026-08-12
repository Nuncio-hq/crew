# Spike 0023 — Per-engine compaction signal matrix (#173)

- **Status:** PASS (narrowed; reuses #169/#180 probe evidence)
- **Date:** 2026-08-12

## Question

Which engines emit a real, harness-observable compaction signal that can
honestly drive `compaction_count`, and which must stay `Unknown` + turn-count
net?

## Decision affected

D-050 / issue #173 — `CompactionSignal` adapters, owner aging banner copy, and
guided handover. Must not fabricate counts for signal-less engines.

## Hypothesis

Hermes and Codex already expose real signals consumed by #180
(`rotation_count` / lineage). buzz-agent exposes `_PostCompact` via
buzz-dev-mcp. Claude transcript compaction is unproven; Grok has none.

## Scope

- Providers: Hermes ACP, Codex ACP, buzz-agent / buzz-dev-mcp, Claude Code,
  Grok (custom harness).
- Files: `docs/crew/spikes/0022-loadsession-reality-matrix.md` (#180 table),
  Crew source (`handle_session_update`, `_PostCompact`), issue #173 probe notes.
- Shared batch with #169/#180 — no second live harness setup in this cloud env.

## Exclusions

- Changing any engine's internal compaction thresholds.
- Deriving counts from `used` / `contextLimit` deltas.
- Owner UI (separate #173 implementation work).

## Pass criteria

A written matrix with per-engine verdict: proven ACP / hook / transcript /
none, and the adapter path Crew will ship.

## Fail criteria

Any engine claimed "known" without a concrete signal path.

## Environment

- Commit: Crew `main` after #180 (`e73960bc…`)
- Evidence: spike 0022 + issue #173 investigation table + in-repo hooks

## Method

1. Reuse spike 0022 / D-049 Hermes provenance + Codex compacted markers.
2. Confirm buzz-dev-mcp `_PostCompact` tool registration and buzz-agent call site.
3. Record Claude / Grok as unknown until a forced-compaction live probe lands.

## Results

| Engine | Signal | Adapter | Verdict |
| --- | --- | --- | --- |
| Hermes ACP | `session_info_update` `_meta.hermes.sessionProvenance` (`compressionDepth`) | `AcpNotification` via existing `parse_engine_rotation_signal` | **Proven — ship** |
| Codex ACP | ACP `context_compacted` / `compacted` when forwarded; else rollout JSONL `type: compacted` / `event_msg`/`context_compacted` | `AcpNotification` or `TranscriptMarker` (fail-loud on parse drift → Unavailable) | **Proven — ship** |
| buzz-agent | MCP `_PostCompact` tool_call (buzz-dev-mcp) | `Hook` counted in `handle_session_update` | **Proven — ship** |
| Claude Code | `PreCompact` hook exists engine-side; transcript compact markers **unproven** on real stores | None in v1 (turn-count net + Unknown copy) | **Unknown — turn-count only** |
| Grok | No compact/summarize/truncate trace in store; not in `known_runtimes` | None | **Unknown — turn-count only** |

`usage_update` / `used`/`size` is decorative only — never feeds the counter.

## Edge cases observed

- Codex rollout format drift must freeze into `Unavailable`, not undercount.
- Hermes birth provenance may seed `rotation_count` for wake (#180) while
  owner-facing `compaction_count` still resets on every `session/new` declare.

## Limitations

- This environment did not re-run live `/compress` or Claude `--autocompact`
  probes; Hermes/Codex rows reuse authenticated evidence from spike 0022 / #180.
- Claude hook+glue remains a follow-up when a forced-compaction probe lands.

## Verdict

**PASS (narrowed).** Ship adapters for Hermes, Codex, and buzz-agent. Claude and
Grok show Unknown + turn-count net (default 100) with no fabricated number.

## Follow-up test contract

1. Counter only moves on real signal; Unknown stays Unknown.
2. Adapter parse failure → Unavailable (sticky).
3. Threshold 3 projects aging; reset on OwnerReset / declare clears it.
4. Turn-count net at 100 for Unknown engines — banner says "long session", never "compacted N×".

## Cleanup

No temporary processes or credentials created for this spike.
