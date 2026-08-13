# Spike 0034 — End-to-end `sim_tap` latency MCP → endpoint → bridge (#197)

- **Status:** INCONCLUSIVE (live HID) / PASS (in-repo path)
- **Date:** 2026-08-13
- **Issue:** [#197](https://github.com/Nuncio-hq/crew/issues/197)

## Question

Does `sim_tap` through MCP → `POST /agent-control` → #196 bridge stay
under 300 ms on a warm instrument (excluding boot)?

## Decision affected

Request/response tools (no streams). Per-instrument single-flight.
Humans preempt in-flight taps with `lease_held`.

## Hypothesis

The extra hop is a localhost JSON POST plus the existing `dispatch_hid`
spawn. Warm-path overhead should be well under 300 ms when the bridge
binary is a no-op fake.

## Scope

- In-process fake bridge (process boundary stub)
- Axum server bound to `127.0.0.1:0`

## Exclusions

- Live CoreSimulator HID
- Counting simulator boot (ensure-on-use may block up to 60 s)

## Pass criteria

Fake-bridge `sim_tap` over the bound endpoint returns in < 300 ms.

## Environment

- OS: Linux cloud agent; no `simctl`

## Results

Live HID: INCONCLUSIVE.
In-repo: `sim_tap_warm_path_under_300ms` drives the bound control
endpoint against a fake bridge and asserts elapsed < 300 ms.

## Verdict

INCONCLUSIVE live / PASS in-repo for the architectural hop. A real
Mac + baguette measurement remains a follow-up on hardware.

## Follow-up test contract

`sim_tap_warm_path_under_300ms`.

## Cleanup

Test server is dropped with the runtime.
