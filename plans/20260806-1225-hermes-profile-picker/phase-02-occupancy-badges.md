# Phase 02 — Occupancy badges (04b)

- **Status:** COMPLETE (this PR)
- **Issue:** #78 (Should)
- **Contracts:** P-06 (+ strengthen C-10 UX)
- **Depends on:** Phase 01 combobox
- **PR scope:** desktop TS only; may ship with Phase 01 or as immediate follow-up

## Deliverable

Each profile option shows whether it is **free** or **already bound** to
another managed agent on the **current relay**, so Oscar sees C-10 before Save.

```
scout      free
builder    bound · Builder
```

## Design decisions

| ID | Topic | Choice |
| -- | ----- | ------ |
| B1 | Source of truth for bind | Server reject on create/update remains authoritative (#60). Badge is **early feedback only** |
| B2 | Occupancy join | Client-side: `listHermesProfiles()` ⨯ managed agents where `hermesProfile` matches and `relayUrl` equals current community relay |
| B3 | Edit-self | When editing agent A bound to `scout`, `scout` shows as current binding (e.g. “this agent” / no “bound to other”) — selecting it stays valid |
| B4 | Select bound-other | **Allow select**; disable primary Save/Create with inline reason mirroring server copy; do not hard-remove from list (discoverability) |
| B5 | No IPC for occupancy | Do not add `list_hermes_profile_bindings` unless client join proves insufficient |
| B6 | Missing agent name | Fall back to truncated pubkey or “another agent” if display name empty |
| B7 | Multi-community | Scope strictly by `relay_url` string equality used by server duplicate check |

## Pure logic (unit-test first)

```ts
type ProfileOccupancy =
  | { status: "free" }
  | { status: "bound"; agentName: string; agentPubkey: string }
  | { status: "self" }; // editing the agent that already holds it

buildHermesProfileOccupancy(args: {
  profiles: string[];
  agents: Array<{
    pubkey: string;
    name: string;
    hermesProfile: string | null;
    relayUrl: string;
  }>;
  relayUrl: string;
  editingPubkey?: string | null;
}): Map<string, ProfileOccupancy>
```

RED cases: free, bound-other same relay, bound-other different relay → free,
self when editing, unbound agents ignored, `default` never mapped.

## UI

- Option secondary text / badge: `free` (muted) vs `bound · {name}` (warning tone)
- When value is bound-other: inline error under field (reuse
  `hermes-profile-error` or sibling testid `hermes-profile-occupied-error`)
- Create/Save disabled when occupied-other (in addition to name validation)

Wire `relayUrl` + agent list + optional `editingPubkey` into
`HermesProfileField` from create/edit parents (props only; no id checks).

## E2E

| Test | Setup | Assert |
| ---- | ----- | ------ |
| badge bound | mock agent bound to `scout` on test relay; list includes scout | option shows bound label |
| block second hire | create second agent, pick scout | Save/Create disabled or save error path still safe |
| edit self | open edit on scout-bound agent | scout selectable; save not blocked solely for occupancy |

## Exit criteria

- [ ] P-06 GREEN
- [ ] Unit coverage for occupancy map
- [ ] E2E badge + block path
- [ ] Server C-10 path unchanged (no Rust)

## Out of scope

Cross-relay “global” occupancy, deleting the other agent from the picker,
model/provider chips on options.
