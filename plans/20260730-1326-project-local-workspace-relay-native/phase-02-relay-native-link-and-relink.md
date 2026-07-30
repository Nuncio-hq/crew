# Phase 2 — Relay-native link and relink

## Overview

- Priority: high
- Status: complete
- Purpose: make local workspace a location of a Buzz Project, not a new record.

## Files to add

- `desktop/src/features/projects/lib/project-local-workspace.ts`
- `desktop/src/features/projects/lib/project-local-workspace-relay.ts`
- `desktop/src/features/projects/ui/crew-project-workspace-panel.tsx`
- `desktop/src/features/projects/ui/crew-project-workspace-consent-dialog.tsx`
- `desktop/src/features/projects/ui/crew-project-workspace-status.tsx`
- `desktop/src/features/projects/ui/crew-projects-screen.tsx`
- `desktop/src/shared/api/tauri-project-folder-dialog.ts`

The six existing RED contract files remain the executable specification.

## Existing files to modify

- `desktop/src/app/routes/projects.tsx`: load the additive Crew wrapper, which
  renders the existing `ProjectsView` plus the workspace control.
- `desktop/package.json`: add the supported dialog JavaScript package.
- `pnpm-lock.yaml`: lock the dependency.

## State transition

```text
relay Project
  -> choose folder
  -> disclose plaintext relay publication
  -> reuse canonical channel or create one
  -> fetch latest (pubkey, 30617, d)
  -> replace only local location; add canonical channel if absent
  -> sign
  -> relay OK
  -> fetch exact signed event id
  -> show Linked
```

Cancel produces no event. Relay timeout, rejection, or mismatched read-back
keeps the previous relay Project authoritative and shows a retryable error.

## Implementation rules

1. Parse only `["buzz-location", "local", path]`.
2. Preserve raw path bytes after syntactic absolute-path validation.
3. Treat duplicate local locations as invalid; explicit relink collapses them.
4. Preserve Project content and durable non-local tags. Strip transient
   `auth`, matching native Buzz update behavior.
5. Select the latest addressable event by newest `created_at`, then lowest
   lexical id for a tie.
6. Never return the locally signed event as saved state; return relay read-back.
7. Do not infer Git status or change clone metadata.
8. If channel creation succeeds but Project publication fails, report the
   created channel id for retry; do not silently delete relay state.
9. Only the current Project owner may sign a replacement.
10. Advance replacement time to the observed relay head plus one second.
11. Fail closed for malformed, duplicate, or conflicting `buzz-channel` tags.

## Success criteria

- A Project made by `buzz create project` is linkable without changing its
  `(pubkey, d)`.
- Cold reload reconstructs the linked path from kind `30617`.
- Unknown and Buzz protection tags survive relink.
- Unsupported clients can ignore the Crew tag and still read the Project.

## Rollback

Remove the additive wrapper and restore the route import. Existing Project
events remain valid; Buzz ignores the unknown location tag.
