---
phase: 5
title: "Pin docs and open PR"
status: pending
priority: P1
effort: "1h"
dependencies: [4]
---

# Phase 5: Pin docs and open PR

## Overview

Record the new Buzz pin in machine-readable + human docs, open a sync PR
against `Nuncio-hq/crew` `main`, and merge only after Gate + Upstream Sync
evidence is attached.

## Requirements

- Functional: pin JSON matches merged tag/commit; STATE/RELEASING mention 0.5.10.
- Non-functional: PR targets Crew only (D-020); commits signed-off.

## Related Code Files

- Modify: `docs/crew/upstream-buzz.json`
- Modify: `docs/crew/STATE.md` (Local / Buzz source pin sections)
- Modify if they still say 0.5.7: `docs/crew/RELEASING.md`, `docs/crew/DECISIONS.md` (only version strings — no decision rewrite)
- Optional: Settings UI string if hard-coded (prefer reading from pin JSON if already wired)

## Implementation Steps

1. Update pin:

```json
{
  "buzzVersion": "0.5.10",
  "buzzTag": "desktop-v0.5.10",
  "buzzCommit": "1fb49103002e898607a7f6fd554cb51e94d92e08"
}
```

2. Update `docs/crew/STATE.md` lines that claim `0.5.7` / `desktop-v0.5.7` / old SHA.

3. Commit docs (DCO):

```bash
git add docs/crew/upstream-buzz.json docs/crew/STATE.md
git commit -s -m "$(cat <<'EOF'
docs(crew): pin Buzz upstream to desktop-v0.5.10

EOF
)"
git push
```

4. Open PR with `gh` to **Nuncio-hq/crew**:

```bash
gh pr create --repo Nuncio-hq/crew --base main --head sync/upstream-2026-08-12 \
  --title "chore(sync): Buzz desktop-v0.5.10" \
  --body "$(cat <<'EOF'
## Summary
- Merge Buzz `desktop-v0.5.10` (`1fb4910300…`) into Crew (from pin `0.5.7`).
- Preserve Crew evidence / ACP / CLI fork seams; take upstream glass, agents, perf, send-to-channel.
- Update `docs/crew/upstream-buzz.json` pin.

## Why 0.5.10 (not 0.5.9)
0.5.10 removes 0.5.9+ desktop perf regressions and speeds `get_channels`.

## Test plan
- [ ] `NuncioCrew Upstream Sync` green on this branch HEAD
- [ ] `NuncioCrew Gate` green
- [ ] Evidence card smoke
- [ ] Agents unified add/edit smoke
- [ ] Thread Send-to-channel smoke
- [ ] Settings/glass + theme preserve smoke
- [ ] Channel switch / focus refetch / timeline retention feel OK
- [ ] `git merge-base --is-ancestor desktop-v0.5.10 HEAD`

## Fork delta
\`\`\`
$(git diff --stat desktop-v0.5.10...HEAD | tail -20)
\`\`\`

EOF
)"
```

5. After merge: delete sync branch remotely if policy says so; note follow-up Local rebuild / Crew release separately if deferred.

## Success Criteria

- [ ] `upstream-buzz.json` matches tag + commit above
- [ ] STATE docs consistent with pin
- [ ] PR URL on `Nuncio-hq/crew` (not block/buzz)
- [ ] Gate green; Upstream Sync evidence linked in PR
- [ ] Plan phases checked complete via `ck plan check` after merge

## Risk Assessment

| Risk | Mitigation |
|---|---|
| Pin SHA typo | Copy from `git rev-parse desktop-v0.5.10` |
| Docs say Local shows 0.5.10 before rebuild | State clearly if Settings still shows old until Local rebuild |

## Rollback

Revert sync PR; pin JSON returns to 0.5.7. Do not force-push main.
