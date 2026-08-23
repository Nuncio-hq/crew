---
title: "CI Webhook Wake Setup"
tags: [workflows, ci, agents, webhooks]
status: active
created: 2026-08-23
---

# CI webhook → Buzz workflow → agent wake

Event-driven alternative to blocking in-turn on `gh pr checks --watch`.

## Overview

```text
GitHub CI completes
  → webhook POST to Buzz workflow
  → workflow send_message with @Agent
  → ACP harness dispatches mention
  → agent resumes in a fresh turn
```

## Steps

### 1. Create the workflow

Use [`ci-complete-wake-workflow.yaml`](ci-complete-wake-workflow.yaml) as a
template. Set:

- `channel:` to your project channel UUID
- `text:` to include the agent's **exact display name** after `@`

Create in Buzz Desktop (channel → Workflows) or via CLI when available.

### 2. Copy the webhook URL

From the workflow detail in Buzz Desktop, copy the webhook trigger URL and
secret (if shown). Exact UI path may vary by version — look for "Webhook" on
the workflow trigger card.

### 3. Configure GitHub

In the repo (or org) **Settings → Webhooks → Add webhook**:

| Field | Value |
|-------|-------|
| Payload URL | Buzz workflow webhook URL |
| Content type | `application/json` |
| Secret | Match Buzz workflow secret if required |
| Events | `Check runs` and/or `Workflow runs` (or `Check suite`) |

Filter noisy events in GitHub (branch, workflow name) or add workflow `filter`
expressions when the engine supports payload fields you need.

### 4. Agent behavior

On wake, the agent should:

1. `gh pr view` / `gh pr checks` for current state
2. Fix failures or merge if green
3. **Not** assume the previous turn is still open

## Limitations

- Workflow `send_message` does not auto-resolve `@Name` to pubkey — use exact
  display names; agents resolve mentions against channel members on dispatch.
- No built-in GitHub payload parsing in this minimal example — extend with
  `call_webhook` echo steps or filter in GitHub for v1.
- `reply_in_thread` is **not** supported for `webhook` triggers.

## See also

- [Agent wait patterns](../GUIDES/AGENT_WAIT_PATTERNS.md)
