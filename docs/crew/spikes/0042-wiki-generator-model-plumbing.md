# Spike 0042 — Generator model plumbing (#200)

- **Status:** PASS
- **Date:** 2026-08-13
- **Issue:** [#200](https://github.com/Nuncio-hq/crew/issues/200)

## Question

Which provider/key path does the governed desktop worker use on day one,
versus an agent-runtime generate?

## Decision affected

D-061: generator is caller-agnostic; pick one day-one path.

## Hypothesis

Desktop worker is the default caller. Heuristic generation is always
available (tests, air-gapped, no billing). Optional OpenAI-compatible
HTTP (`CREW_WIKI_API_KEY`, `CREW_WIKI_MODEL`, crate feature `llm`) is
the paid path. Agent-runtime generate is not day-one — MCP
`wiki_generate` only acquires the same lock and asks the desktop worker
to publish.

## Scope

- `crew-wiki::generate::{HeuristicGenerator, generator_from_env}`
- `desktop/src-tauri/src/wiki_worker.rs`
- `buzz-dev-mcp` `wiki_generate`

## Exclusions

A hosted Crew Wiki service. Per-agent BYOK. Bundling a local GGUF.

## Pass criteria

No API key → heuristic markdown with mermaid + citations. Parallel
`wiki_generate` for the same repo returns `generate_in_progress`.
Desktop invoke `wiki_generate` exists.

## Fail criteria

Unbounded parallel gens. Required cloud key for the happy path.
Agent subprocess as the only publisher (D-028: founder signs).

## Environment

Unit tests in `crew-wiki` and `buzz-dev-mcp`. No live LLM in this VM.

## Method

Read env in `generator_from_env`. Default returns `HeuristicGenerator`.
Lock test covers governance.

## Results

**Pick:** heuristic default; OpenAI-compatible HTTP when
`CREW_WIKI_API_KEY` is set on an `llm`-featured build. Agent-runtime
generate is deferred.

## Edge cases observed

Missing git snapshot still returns `accepted: true` with `pages: 0` so
the renderer can publish a stub (E2E / empty checkout).

## Limitations

Live OpenAI was not called. Recorded as fixture/env-gated.

## Verdict

PASS

## Follow-up test contract

Heuristic mermaid/language tests. Generate lock test. E2E generate via
mock invoke.

## Cleanup

None.
