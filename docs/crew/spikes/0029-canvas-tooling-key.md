# Spike 0029 — canvas `tooling` key round-trip (#196)

- **Status:** PASS
- **Date:** 2026-08-13
- **Issue:** [#196](https://github.com/Nuncio-hq/crew/issues/196)

## Question

Can a `tooling` key live in the existing channel canvas ` ```crew ` YAML
block (kind 40100) alongside roles/routing/capabilities without breaking
current parsers, and can a write preserve unknown keys?

## Decision affected

D-043 / D-044 / D-058 — intent (device type, runtime, dev-server
command) is owner-signed on the canvas. Machine UDIDs stay off the
relay (spike 0002).

## Hypothesis

`parse_canvas_assignments` deserializes a struct with `#[serde(default)]`
and does **not** `deny_unknown_fields`, so `tooling` is ignored today.
`update_canvas_crew_assignment` round-trips `serde_yaml::Value`, so
unknown keys already survive assignment writes.

## Scope

- Unit tests against `update_canvas_crew_assignment` and the new
  `update_canvas_crew_tooling` helper
- No live relay required

## Pass criteria

1. Existing role parse succeeds when `tooling` is present.
2. Assignment update preserves `tooling`.
3. Tooling update preserves `assignments` / `routing` / unknown keys.
4. Parsed tooling contains intent only (no UDID).

## Fail criteria

A tooling write drops roles, or a role write drops tooling, or a UDID
is stored in the canvas.

## Method

Contract tests in `desktop/src-tauri/src/commands/canvas_tooling.rs`
and `desktop/src/features/tool-pane/canvasTooling.test.mjs`.

## Results

See those tests. Verdict follows the test run in this PR.

## Verdict

**PASS** once the named tests are green (Gate 2–7 in the same PR).
