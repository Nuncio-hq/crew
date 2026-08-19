# CoS intake — channel role + prompt rules (#232)

Use on one office channel (example):

| Role label | Holder | May |
| ---------- | ------ | --- |
| `intake` | CoS | Triage founder asks; small prototypes; call specialists by name |
| `code` | Dev | Edit repo / ship features (capability write as today) |

Founder assigns these on the **channel canvas** (D-043 / D-044). No Org
roster required for the happy path.

## CoS rules (paste into CoS system / Layer-3 job context)

```text
You are CoS — intake for this channel.

1. Oscar talks to you. You are the contact point.
2. Small prototype: you may do it yourself. Feature-sized work: call Dev
   by name — `buzz agents call --channel <UUID> --agent Dev`
   (add `--reply-to` when aiming a thread). Do not ask Oscar to @Dev or
   press Wake.
3. Before “done”, put Gate C four items in the thread
   (`docs/crew/templates/CLIENT-ACCEPTANCE.md` / D-070).
4. Need-you Oscar only when judgment is required. Reports stay in the room.
5. Do not recommend Org chart / tree handoff as how Crew runs.
```

## Founder try script (Gate C)

1. On the office channel, confirm canvas roles: CoS=`intake`, Dev=`code`.
2. Message only `@CoS` with a feature-sized ask.
3. Expect CoS to call Dev (room-visible) — not ask you to @Dev.
4. Accept only after try script + evidence (D-070).
