# Issue #102 plan update

The plan concludes that this should be **projection-first**: thread identity,
channel scope, ACP questions, receipts, reactions, Git/Project context, and
agent telemetry already have Buzz/Crew seams. The one unavoidable missing fact
is explicit promotion intent; it cannot be inferred from a worktree or receipt.
The founder has now settled **D-1 = option A** (owner-authored tagged kind-9
message, no new event kind), **D-2 = option A** (not deferred board schema),
and **D-3 = option A** (promote anywhere; ordinary channels have no isolated
worktree and must say so plainly in the UI).

Slices: 00 spike/reality check; 01 promote in place and restart reconstruction;
02 live state and inline decision; 03 review in channel; 04 cancel/reopen/
recovery.

Nothing was implemented and no D-number was taken; the plan lives at
`plans/20260811-channel-first-missions/`.

PR: https://github.com/Nuncio-hq/crew/pull/143
