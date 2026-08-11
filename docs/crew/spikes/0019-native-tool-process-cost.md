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

### Turns and warm sessions

First-token latency for a cold process and a warm-session `session/new` were
**UNMEASURED**. The real Codex ACP attempt reached initialization but rejected
`session/new` with:

```text
Internal error: plan type is required for chatgpt authentication
```

The standalone Codex CLI then reached the backend and returned HTTP 401:

```text
auth_recovery_outcome="recovery_failed_permanent"
Turn error: Your access token could not be refreshed because you have since
logged out or signed in to another account. Please sign in again.
```

Consequently, one-turn RSS, cold first-token latency, warm-session creation
latency, and warm-session first-token latency have no observed values. No
modeled values are substituted.

Claude idle/turn RSS and all Claude latency measurements are **UNMEASURED**:
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

Do not reshape Slice 3 to process-per-thread based on this measurement alone.
At 48 Codex ACP processes, idle aggregate RSS was 4.46 GiB and no RAM or FD
ceiling was reached on the 31.4 GiB VM, but batch launch time increased from
80.04 ms at 24 to 1,107.26 ms at 48. Native-turn cost is unknown because
authentication blocked every real turn, and Claude was unavailable. Keep the
current two-tier model and its explicit limitation until a fresh credential
permits first-token/turn measurements and a real session-addressed native
permission test. If that test shows the native floor must be process-scoped,
use these numbers as a baseline for a separately scoped process-per-thread
design rather than silently assuming it is cheap.
