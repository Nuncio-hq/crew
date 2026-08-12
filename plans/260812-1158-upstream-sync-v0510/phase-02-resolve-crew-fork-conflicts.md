---
phase: 2
title: "Resolve Crew fork conflicts"
status: pending
priority: P1
effort: "3-6h"
dependencies: [1]
---

# Phase 2: Resolve Crew fork conflicts

## Overview

Clear every merge conflict with the thin-fork rule: **take upstream's version,
then re-apply the Crew delta on top.** Do not line-merge large rewrites.
Preserve Crew evidence / ACP / CLI seams listed in `docs/crew/UPSTREAM-SYNC.md`.

## Requirements

- Functional: tree builds conceptually; Crew evidence + ACP prompt assertion still present.
- Non-functional: D-022 file-size baselines updated only for upstream growth.

## Architecture

```text
desktop-v0.5.10 (upstream)
        │
        ▼
  take upstream file
        │
        ▼
  re-apply Crew delta (evidence props, clap flag, playwright match, ACP assert)
        │
        ▼
  sync/upstream-2026-08-12
```

## Related Code Files

### Must preserve Crew behavior

| File | Crew delta to keep |
|---|---|
| `crates/buzz-acp/src/lib.rs` | Prompt/office assertion + Crew modules alongside upstream ACP tests |
| `crates/buzz-acp/src/base_prompt.md` | Office-level behavioral section (if conflict appears) |
| `crates/buzz-cli/src/lib.rs` | Evidence flag on `messages send` + channel `--visibility` from upstream |
| `crates/buzz-cli/src/commands/evidence.rs` | Crew-owned module (should be ours-only) |
| `desktop/playwright.config.ts` | Crew evidence contracts in smoke `testMatch` |
| `desktop/src/features/messages/ui/MessageRow.tsx` | Evidence-card prop pass-through **and** upstream Send-to-channel |
| `desktop/src/features/messages/ui/MessageRowDefaultBody.tsx` | Evidence tag dispatch before Markdown |
| `desktop/src/features/communities/useCommunityInit.ts` | `resetCommunityState()` singleton list — add any new upstream resets |

### Predicted conflict clusters (~67)

- Root: `.github/workflows/ci.yml`, `.release/desktop-candidate.json`, `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, `Justfile`, `RELEASING.md`, `pnpm-lock.yaml`
- ACP: `acp.rs`, `config.rs`, `lib.rs`, `pool.rs`, `queue.rs`
- CLI: `crates/buzz-cli/src/lib.rs`
- Relay: `crates/buzz-relay/src/handlers/ingest.rs`
- Tauri: `Cargo.toml`/`lock`, `lib.rs`, `commands/*`, `events.rs`, `initial_window.rs`, `managed_agents/runtime.rs`, `tauri.conf.json`
- Desktop agents / channels / messages / settings / shared API / e2e helpers (see plan overview)

## Implementation Steps

1. For each conflicted path:

```bash
# Prefer when Crew delta is small / additive:
git checkout --theirs -- <path>   # NOTE: in a merge, "theirs" = incoming tag
# Then re-apply Crew edits from origin/main for that path.
```

Confirm direction once: during `git merge desktop-v0.5.10` while on Crew branch,
**ours = Crew**, **theirs = Buzz tag**. Prefer `theirs` then re-apply Crew.

2. **ACP cluster** — after taking upstream:
   - Keep Crew `mod` declarations / tests that assert office prompt contract.
   - Accept upstream usage/session-context reductions (#5423) and permission revert (#5323).
   - Run a quick `rg` for Crew-only symbols that must still exist.

3. **CLI** — merge `--visibility` (upstream) with evidence flag (Crew). Both additive.

4. **MessageRow.tsx** — keep:
   - Upstream Send-to-channel UI/actions
   - Crew evidence prop pass-through into `MessageRowDefaultBody`
   Do not move evidence logic back into `MessageRow`.

5. **playwright.config.ts** — keep Crew evidence specs in smoke project; accept upstream search/link-preview test registrations.

6. **Settings / theme / glass** — prefer upstream Settings panels + theme.css; re-apply any Crew SettingsView pin/version row. Visual QA in Phase 4.

7. **useCommunityInit.ts** — diff upstream resets; append any new community-scoped singleton resets Crew must call.

8. **Lockfiles** — regenerate rather than hand-merge when possible:

```bash
# After Cargo.toml settled:
cargo metadata --format-version 1 >/dev/null
# After package.json settled:
cd desktop && pnpm install
```

9. **D-022 baselines** — if `desktop/scripts/file-size-baselines.json` or guard fails on upstream-grown files, update recorded `lines` to exact `wc -l` from the guard output (upstream growth only).

10. Complete merge:

```bash
git add -A
git commit -s -m "$(cat <<'EOF'
chore(sync): merge Buzz desktop-v0.5.10 into Crew

EOF
)"
```

If pre-commit `desktop-tauri-fmt` fails in worktree: run `just desktop-tauri-fmt` from the **main checkout**, re-stage, new commit (do not `--no-verify`).

## Success Criteria

- [ ] `git diff --name-only --diff-filter=U` empty
- [ ] Merge commit exists with DCO sign-off
- [ ] `git merge-base --is-ancestor desktop-v0.5.10 HEAD`
- [ ] Evidence symbols still present (`evidenceTag`, evidence clap flag, MessageRowDefaultBody dispatch)
- [ ] ACP prompt assertion still present in `buzz-acp` tests
- [ ] No accidental deletion of Crew-owned `crates/buzz-cli/src/commands/evidence.rs`

## Risk Assessment

| Risk | Mitigation |
|---|---|
| Silent drop of evidence pass-through | Phase 4 evidence e2e / unit contracts |
| Wrong ours/theirs during merge | Documented direction + spot-check `git show :2:path` vs `:3:path` |
| Huge ACP pool/usage rewrite | Prefer upstream file; re-apply only Crew assertions |

## Next Steps

Phase 3 — local compile/lint/unit gates before pushing CI.
