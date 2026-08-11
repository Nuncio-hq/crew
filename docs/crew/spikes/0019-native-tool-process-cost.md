# Spike 0019 — native-tool capability process cost

## Question

What does it cost to use one native-tool engine process per
`(agent, thread)` if session-addressed native permission configuration cannot
provide a per-channel floor?

This is a measurement-only spike. Slice 3 remains the shipped two-tier model:
`buzz-dev-mcp` is channel-scoped through ACP `mcpServers`, while native
file/shell tools remain process-scoped. On engines with native file/shell
tools, channel dev-mcp denial is a Crew rule, not a wall.

## Method and host

The measurement started only its own `codex-acp` child processes and terminated
those children after sampling. It did not touch the relay, desktop, or testing
agent processes.

Observed VM:

| Property | Value |
| --- | --- |
| CPU | 8 logical CPUs |
| RAM | 32,881,112 KiB (31.4 GiB) |
| Swap | 0 B |
| File-descriptor limit | 65,536 |
| Kernel | 5.15.200 |
| Engine | `codex-acp` 1.1.14, Codex CLI 0.147.0 |
| Claude | Not obtainable; `claude` is not installed |

The credential-free harness launched `codex-acp` with stdin held open, waited
one second, and sampled each child from `/proc/<pid>/status` and
`/proc/<pid>/fd`. The complete raw result is
`assets/0019-native-tool-process-cost/codex-process-measurements.json`; the
measurement script is adjacent to it.

## Observed process cost

### Codex ACP idle RSS and startup

| Child processes | Sampled | Aggregate RSS | Mean RSS/process | Mean FDs/process | Launch time for batch |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1 | 97.9 MiB | 97.9 MiB | 24.0 | 0.75 ms |
| 8 | 8 | 778.8 MiB | 97.4 MiB | 24.0 | 5.85 ms |
| 24 | 24 | 2,341.3 MiB | 97.6 MiB | 24.0 | 80.04 ms |
| 48 | 48 | 4,458.2 MiB | 92.9 MiB | 23.38 | 1,107.26 ms |

RSS is resident memory observed one second after launch, before a model
turn. The lower 48-process mean is a sampling-time effect, not evidence that
Codex becomes cheaper; aggregate RSS is the capacity number to use.

At 48 processes, observed aggregate RSS was 4.46 GiB, or approximately 14.2%
of this VM's 31.4 GiB. The measurement did not reach RAM exhaustion, FD
exhaustion, or a process failure. Launch time did degrade sharply between 24
and 48 children (80.04 ms to 1,107.26 ms for the batch), indicating CPU or
process-launch contention rather than an FD ceiling. The measurement did not
run a model turn, so it contains no native-tool turn RSS.

### Turns and warm sessions — measured on authenticated engines

The owner completed device-code login for Codex (`codex login --device-auth`)
and for the official xAI Grok Build CLI (`grok login --device-auth`,
`grok 1.0.0`), so the previously UNMEASURED rows are now observed. Each engine
was spawned once, initialized, given one turn on a cold session, then given a
second turn on a **new session created on the same warm process**. RSS is the
adapter process plus its direct children; the prompt was a single-token reply
(`Reply with exactly: PONG`), so first-token latency is dominated by the model
round trip, not by Crew.

| Measurement | Codex (`codex-acp` 1.1.14) | Grok (`grok agent stdio` 1.0.0) |
| --- | ---: | ---: |
| spawn → `initialize` returned | 352.4 ms | 201.6 ms |
| spawn → cold session ready (`session/new`) | 1,503.2 ms | 1,302.2 ms |
| cold session first token | 3,084.5 ms | 3,310.8 ms |
| cold turn total | 3,459.9 ms | 3,358.9 ms |
| **warm `session/new`** | **100.3 ms** | **50.1 ms** |
| warm session first token | 3,649.7 ms | 920.8 ms |
| warm `session/new` → first token | 3,750.0 ms | 971.0 ms |
| idle RSS after `initialize` | 145.0 MiB | 70.1 MiB |
| peak RSS across two turns | 159.2 MiB | 109.4 MiB |
| RSS growth from idle | +14.2 MiB | +39.3 MiB |

Assets: `assets/0019-native-tool-process-cost/real-codex-latency.json`,
`assets/0019-native-tool-process-cost/real-grok-latency.json`, probe script
`assets/0019-native-tool-process-cost/acp_latency_probe.py`.

Two things matter for the process-per-thread question:

1. **A new session on a warm process costs 50–100 ms; a new process costs
   1.30–1.50 s before the session is even usable.** Process-per-`(agent,
   thread)` therefore adds roughly 1.2–1.4 s of dead time to every first
   message in a new thread — 13–30× the warm-session path.
2. **Turn RSS is modest** (+14 MiB Codex, +39 MiB Grok over idle), so the
   dominant per-process cost is the ~70–145 MiB idle baseline, matching the
   idle scaling measured above.

Claude idle/turn RSS and all Claude latency measurements remain **UNMEASURED**:
the `claude` binary was not installed in this environment
(`bash: claude: command not found`).

## Laptop extrapolation

This is an explicitly labeled extrapolation, not an observation. Assume a
founder-class laptop with 16 GiB RAM, 8 logical CPU cores, and 1 GiB reserved
for the OS/desktop, leaving approximately 15 GiB for engine processes and
the application. Applying the observed Codex idle mean of roughly 93–98 MiB
per process gives:

| Processes | Linear idle-RSS extrapolation |
| ---: | ---: |
| 48 | 4.5–4.7 GiB |
| 96 | 8.9–9.4 GiB |
| 128 | 11.9–12.5 GiB |
| 160 | 14.9–15.6 GiB (at or beyond the assumed budget) |

The laptop values are linear extrapolations from idle RSS only. They do not
predict turn RSS, model cache behavior, thermal throttling, swap, or engine
startup contention. At 48 processes, RAM alone is not the limiting factor
under this assumption; the observed 24→48 launch jump is the first measured
contention signal.

## Re-keying cost in `pool.rs`

The current process pool is agent-keyed: runtime identity is one managed agent
process (with agent/relay identity), and `OwnedAgent.state.sessions` is a
`HashMap<Uuid, String>` keyed by the conversation/channel UUID. `try_claim`
first prefers an idle process containing that channel UUID, then any idle
process. One `AcpClient` owns one subprocess, while multiple ACP sessions
share it.

Re-keying to `(agent, thread)` would therefore not be a one-line map-key
change. A rough diff shape is:

1. Introduce a thread identity in `PromptSource`/`TaskMeta` and the pool's
   session state, while retaining the real routing channel separately.
2. Change session, turn-count, core/canvas cache, routing-channel, workspace
   binding, invalidation, affinity, and `try_claim` lookups from `Uuid` to a
   composite key.
3. Change lifecycle/runtime sizing so a thread key selects or spawns a
   dedicated `AcpClient`/`OwnedAgent` rather than merely a second `session/new`
   on an existing process.
4. Revisit recovery, eviction, process-exit, and desktop managed-agent
   runtime bookkeeping, all of which currently reason about one process with
   many channel sessions.

The existing ACP transport already accepts per-session `mcpServers`; the
expensive part of process-per-thread is process lifecycle and pool semantics,
not the wire request shape. A realistic implementation would touch
`pool.rs`, the ACP runtime/spawn path, managed-agent runtime sizing and
eviction, and tests for affinity and recovery. This spike does not implement
that change.

## Recommendation

**Do not implement process-per-`(agent, thread)`.** Two measured results now
point the same way:

1. Spike 0018's authenticated real-engine run showed both tested engines
   enforce a **per-session** native-tool floor on one process (Codex via
   session-addressed `set_config_option` `mode`, Grok via `set_mode`), so
   process-per-thread is not required to get a per-thread floor.
2. The cost of taking it anyway is real: 1.30–1.50 s to a usable session on a
   cold process versus 50–100 ms for a new session on a warm one, on top of a
   ~70–145 MiB idle baseline per process (4.46 GiB at 48 Codex processes, and
   batch launch time degrading from 80.04 ms at 24 to 1,107.26 ms at 48).

The cheaper and stronger change is to source the session's ACP permission mode
from the channel's role assignment, using the seam Crew already has at
`crates/buzz-acp/src/pool.rs:1103-1110`, instead of from shared process
context. That keeps one process per agent and still gives each thread its own
enforced floor.

Process-per-thread should be reconsidered only for an engine that advertises no
session-scoped permission control; Claude is untested here
(`bash: claude: command not found`) and must be measured before any claim is
made about it. The `pool.rs` re-keying shape above stays on record as the
baseline cost if that ever becomes necessary.
