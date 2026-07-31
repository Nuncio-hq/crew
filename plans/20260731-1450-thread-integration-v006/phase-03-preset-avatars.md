# Phase 03 — Provider avatars for preset harnesses + Devin

- **Status:** Not started
- **Priority:** medium

## Context — two different avatar surfaces

Read this before touching anything; they are easy to confuse.

| Surface | Source | Covered today |
| --- | --- | --- |
| Harness gallery icon (settings/onboarding) | `RUNTIME_MARKS` + `PRESET_LOGOS` in `desktop/src/features/onboarding/ui/` — bundled assets | **all 8 presets** |
| Managed agent's chat/profile avatar | `avatar_url` on `KnownAcpRuntime` (`discovery.rs:16-19`), consumed via `managed_agent_avatar_url` → `resolve_created_avatar_url` (`commands/agents.rs:234`) | only `goose`, `claude`, `codex`, `buzz-agent` |

The gap Oscar hit is the **second** row: an agent created on Cursor, Grok Build,
Kimi and friends gets no vendor avatar in chat, because `PresetHarness` has no
`avatar_url` field at all (`discovery.rs` preset builder hardcodes
`avatar_url: String::new()`).

The profile avatar is published to the relay and rendered by other clients, so
it must be a **public URL** — a bundled asset path cannot serve it. That is why
`claude` and `codex` point at vendor CDN URLs.

## Requirements

1. Add `avatar_url: &'static str` to `PresetHarness` and populate it for all 8
   presets: `cursor`, `omp`, `grok`, `opencode`, `kimi`, `amp`, `hermes`,
   `openclaw`.
2. Add a `devin` entry with its avatar, even though the CLI is not yet in use.
3. Every URL must be an official vendor asset. **Do not draw, crop, recolor, or
   otherwise generate an image.**
4. Keep the existing security line intact: static URLs compiled into the binary
   are fine; user-supplied avatar URLs from custom harnesses stay rejected
   (`discovery.rs:1778`, "F1 security fix"). Do not relax that path.

## Source priority for each URL

1. The vendor's VS Code Marketplace extension icon CDN — same shape as the
   existing `CLAUDE_CODE_AVATAR_URL` / `CODEX_AVATAR_URL`.
2. The vendor's own docs or site logo asset, as `GOOSE_AVATAR_URL` does.
3. The vendor's GitHub organization avatar
   (`https://avatars.githubusercontent.com/u/<id>`).

Verify each URL before committing: it must return HTTP 200 with an `image/*`
content type. Record the chosen source per provider in the PR description.
Add attribution to `desktop/public/harness-logos/CREDITS.md` where the asset
licence calls for it.

## Devin

Devin has no verified ACP command. Add the catalog entry with the command and
args taken from Devin's official CLI documentation. If the docs describe no
ACP or stdio agent mode, still add the entry with its documented CLI command
and say so plainly in `install_hint` — **do not invent arguments** to make it
look runnable. Note the outcome in the PR description either way.

## Files

- `desktop/src-tauri/src/managed_agents/discovery.rs`
- `desktop/src/features/onboarding/ui/RuntimeIcon.tsx` /
  `HarnessMarks.tsx` — only if Devin needs a gallery icon too
- `desktop/src/features/onboarding/ui/presetLogos.test.mjs` — the existing
  coverage guard must still pass with a 9th preset
- `desktop/public/harness-logos/CREDITS.md`

## Validation

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml
cd desktop && pnpm test
just ci
```

Manual: create an agent on a preset harness and confirm its chat avatar is the
vendor mark, not initials.

## Risk

A vendor CDN URL can rot. The avatar is resolved once at agent creation, so a
later 404 degrades to the existing fallback rather than breaking the app —
acceptable, and the same exposure `claude` and `codex` already carry.

## Rollback

Revert the commit. Agents created in the meantime keep the avatar already
written to their profile; that is cosmetic and needs no migration.
