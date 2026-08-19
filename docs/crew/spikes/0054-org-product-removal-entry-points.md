# Spike 0054 — Org product entry points for removal (#233)

- **Status:** PASS
- **Date:** 2026-08-19
- **Issue:** #233
- **Verdict lean:** **P2** — full product+doc removal; protocol (`30680`) inert for sync; supersede D-060 *product*

## Question

What must leave the founder/agent surface so Org is not recommendable, and
what must stay for thin-fork / relay sync?

## Pass criteria

1. Every desktop nav / route / chart / roster editor / handoff-assign chrome is listed.
2. Agent-facing ORG-CHECK / officer-loop / STATE sell of Org is listed.
3. E2E that exists only to demo the chart is listed.
4. A clear P1 / P2 / P3 choice with founder lean recorded.

## Results (inventory summary)

| Layer | Action |
| ----- | ------ |
| Sidebar Org + `/org` + chart/editor + handoff/budget chrome | **remove** |
| HERMES officer loop, STATE #198 product sell, FOUNDER-PRODUCT Org L1 | **rewrite / remove** |
| `org-hierarchy` e2e + Org halves of office-chrome / responsive `/org` | **remove** |
| Desktop read/write of 30680 + pending handoff tags | **remove** |
| ACP ORG-CHECK prompt section + turn-start budget product | **make inert** |
| Relay ingest + `KIND_ORG_ROSTER` constant | **keep** (sync safety) |
| Channel roles D-043/D-044; #230/#232 | **unchanged** |

Full path inventory lived in the implementer pass on this branch (search:
`OrgScreen`, `open-org-view`, `ORG-CHECK`, `e2eOrgRoster`, `buzz org`).

## Verdict

**PASS → P2.** Do not leave a hidden Org settings entry. Prefer deletion over
demotion. Escalate to P3 (relay drop) only if agents still rediscover 30680
after product+prompt removal.

## Follow-up test contract (RED)

1. Sidebar has no `open-org-view`.
2. Navigating `/org` redirects to Inbox (`/`).
3. Agent docs/prompts no longer teach ORG-CHECK / org chart as how Crew runs.
