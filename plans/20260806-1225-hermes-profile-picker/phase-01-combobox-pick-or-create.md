# Phase 01 — Combobox pick-or-create (04a)

- **Status:** COMPLETE (this PR)
- **Issue:** #78
- **Contracts:** P-01 … P-05, P-07
- **PR scope:** desktop TS/UI + tests + HERMES.md; no Rust

## Deliverable

Replace the free-text-only Hermes profile control with a **combobox** that
lists disk profiles and still supports explicit create-in-place.

Manager flow:

1. Open Create/Edit agent → choose **Hermes Agent** (or any `profileArg` runtime).
2. Profile control loads `listHermesProfiles()`.
3. Dropdown shows existing names (sorted); typeahead filters.
4. Click a row → value binds.
5. Type a **new** valid name not in the list → existing
   **Create profile '\<name\>'** button remains the only create path.
6. After successful create → list invalidates/refetches; name stays selected.

## Design decisions

| ID | Topic | Choice |
| -- | ----- | ------ |
| A1 | Control shape | Combobox (Popover + filter input), patterned on `ChannelCombobox` — not a closed `<select>` |
| A2 | Data source | Existing `listHermesProfiles()` only; no new IPC |
| A3 | `default` | Filter out client-side even if dir ever contains it; validation still rejects typed `default` |
| A4 | Fetch timing | Query when the profile field mounts / becomes visible (create Customize tab or edit dialog open). Short `staleTime` (e.g. 10–30s) + invalidate on create success |
| A5 | List failure | Keep editable text value + validation; show quiet helper (“Couldn’t load profiles — type a name”). Do not block the dialog |
| A6 | Empty disk | Empty list + helper steers to create-in-place; placeholder e.g. `scout` |
| A7 | Create affordance | Keep `HermesProfileCreateAffordance` as sibling; on success call `onCreated` **and** invalidate list query |
| A8 | Capability gate | Unchanged: field only when `getRenderableHermesProfileField` / `profileArg` field model says so |
| A9 | File-size | Prefer extracting `HermesProfileCombobox.tsx` (or similar) rather than growing `HermesProfileBindingFields.tsx` past ratchet |
| A10 | Accessibility | `role="combobox"`, keyboard ↑↓/Enter/Escape, labelled by existing RequiredFieldLabel |

## Pure logic (unit-test first)

Add pure helpers in `hermesProfileBinding.ts` (or sibling module):

```ts
// sketches — names flexible
filterHermesProfileOptions(profiles: string[], query: string): string[]
// - drop default / invalid names
// - case-insensitive substring filter
// - stable sort

shouldShowHermesProfileCreate(name: string, profiles: string[]): boolean
// - valid name AND not already in profiles (and not default)
```

RED tests before UI wiring.

## UI wiring

| Piece | Change |
| ----- | ------ |
| `HermesProfileField` | Swap bare `Input` for combobox; retain `data-testid="hermes-profile-field"`; input id stays compatible with existing E2E (`#persona-hermes-profile` / edit equivalent) **or** update E2E in same PR |
| Create affordance | After create success → `queryClient.invalidateQueries` (or callback `onProfilesChanged`) |
| Create + Edit surfaces | No extra props beyond optional `relayUrl` deferred to Phase 2 |

### Suggested DOM / testids

- `hermes-profile-field` — root (keep)
- `hermes-profile-combobox-trigger` — open list
- `hermes-profile-combobox-list` — options container
- `hermes-profile-option` — each row (`data-profile="<name>"`)
- `hermes-profile-error` — keep
- `hermes-profile-create-*` — keep

## E2E (extend `hermes-profile-binding.spec.ts`)

Mock bridge already implements `list_hermes_profiles` via `mockHermesProfiles`.

| Test | Setup | Assert |
| ---- | ----- | ------ |
| pick existing | seed profiles `scout`, `builder` | open create → Hermes → open combobox → both options → pick scout → field value scout; model row “decided by profile scout” |
| create new still works | empty or without `research` | type research → Create profile → success path (existing) + option appears on reopen |
| default not listed | seed including accidental `default` if mock allows | option absent; typing default still errors |
| goose hide | unchanged | no field |

Update any fill-via-`#persona-hermes-profile` steps to combobox interactions where the input is no longer a plain textbox — keep at least one path that types a novel name for create.

## Docs

`docs/crew/HERMES.md` § Hiring:

> Preferred: pick runtime **Hermes Agent**, open **Hermes profile**, choose
> an existing profile **or** type a new name and **Create profile**.

## Exit criteria

- [ ] P-01…P-05, P-07 GREEN (unit + E2E)
- [ ] `just desktop-typecheck` + desktop `pnpm check` (+ px-text, file-sizes)
- [ ] No Rust diff
- [ ] HERMES.md updated
- [ ] Parent plan Phase 04a marked done when merged

## Out of scope (this phase)

Occupancy badges, name suggestion from agent title, Hire-from-disk panel.
