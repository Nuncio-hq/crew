# Bind Hermes `default` after confirmation — plan

> **Issue:** https://github.com/Nuncio-hq/crew/issues/243
> **Impl branch:** `feat/issue-243-hermes-default-bind`
> **Spike branch (sibling):** `feat/issue-243-hermes-default-spike` — record `docs/crew/spikes/0056-hermes-default-profile-acp-spawn.md`
> Where brief and plan differ, this plan wins.

**Goal:** Founder can hire a Hermes Crew agent on the personal `default` profile (`~/.hermes`) after an explicit confirmation. Crew still does not edit/archive/delete that home profile.

**Architecture:** Restore original D-019 item 7 / S-2.1 AC3 (confirmation, not a hard ban). Keep D-024 owner-only + local. Occupancy still one local record per profile, including `default`.

---

## Locked product

- Confirm bind of `default`. Cancel = no record.
- No Crew write-through of SOUL.md / `config.yaml` on `default`.
- No create/delete/archive of `default`.
- Named-profile create-in-place unchanged.
- Dialog must not stick on "Reading profile settings…" for `default`.

## Spawn (spike 0056 PASS — follow this)

**Use `hermes -p default acp`.** Existing `-p <bound name>` injection is correct. Do **not** omit `-p` for `default`.

Spike (sibling branch, live Hermes 0.20.4): both argv start ACP against `~/.hermes`; neither creates `~/.hermes/profiles/default`. Bare `hermes acp` is sticky-`active_profile` sensitive. See `SPIKE-0056-VERDICT.md` in this worktree.

## Tasks

### 1. Allow the name `default` for bind-only

- Change `validate_hermes_profile_name` so `default` is a valid **binding** name.
- Keep a separate `is_hermes_home_profile(name)` / `crew_may_mutate_hermes_profile(name)` that is false for `default`.
- Soul/config/lifecycle/archive must still reject mutate of `default`.
- `inject_profile_binding_args`: if profile is `default`, do not prepend `-p default` (hypothesis). Tests for both.

RED then GREEN in `hermes_profile.rs` tests. Flip `validate_rejects_default_profile` to `validate_accepts_default_as_bind_name` + `inject_omits_flag_for_default`.

### 2. Desktop binding helpers

- `validateHermesProfileName("default")` returns null (valid).
- `normalizeHermesProfileList` may include a distinguished home row if the list source adds it; do not drop `default` as invalid.
- `shouldShowHermesProfileCreate("default")` is false.
- Write-through UI (`HermesProfileModelField`, `HermesSoulEditor`) must not query/mutate when name is `default`. Show static copy: edit this profile in Hermes, not Crew.

### 3. Confirmation + picker

- Picker offers one distinguished option for the home profile (label like `Personal (default)`), even when `~/.hermes/profiles/` is empty.
- Selecting it requires an explicit confirm dialog listing shared surfaces (Desktop chat, SOUL.md, memory, skills, credentials, cron, gateways).
- Cancel leaves the field unbound.
- Occupancy: second local record still blocked.

### 4. Docs

- STATE.md short note.
- HERMES.md rule 1: confirmation, not never.
- DECISIONS.md: brief note that hard-ban implementation is superseded; confirmation + no write-through. Do not rewrite all of D-019.

### 5. E2E

Update `desktop/tests/e2e/hermes-profile-binding.spec.ts`: default is offered → confirm binds; cancel does not; list still hides raw accidental `profiles/default` if that dir exists? Prefer: home profile is a special row, not a directory name.

## Non-goals

- Push / PR / merge (orchestrator).
- Write-through of `~/.hermes`.
- Public/remote Hermes (D-024).
- Auto-cloning default into a named profile.
- #242.

## Handoff

Commit `ORCHESTRATOR-HANDOFF.md` at worktree root.
