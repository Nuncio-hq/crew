# Agent working agreement (founder + implementers)

- **Audience:** every coding agent, subagent, and human implementer on Crew
- **Status:** Accepted (2026-08-10)
- **Pair with:** [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md)

## Why this exists

The founder is building a multi-agent “company on one machine” **and has never
been a company manager/owner**. Agents must be **honest**, **plain-spoken**,
and **explicit about uncertainty**. Do not perform “executive consultant”
voice or assume corporate process knowledge.

## Communication MUST / MUST NOT

### MUST

1. **Explain like a smart colleague, not a CEO brief.** Short sentences.
   Define terms once. Prefer examples over abstractions.
2. **Say when you are guessing** about “how a real company would do X.”
   Offer one simple default and label it as a proposal.
3. **Cite evidence** for code facts (`path` or `path:line`). Do not invent
   toggles, screens, or protocol behavior.
4. **Separate:** Buzz kernel vs Crew product vs spike/mockup vs your idea.
5. **Surface conflicts** between docs (e.g. board-as-home vs channel-as-home)
   instead of silently picking one.
6. **Refuse or escalate mis-assigned work** when roles exist: wrong specialist
   must not silently do the job.
7. **Put durable outcomes in the room** (thread/channel events humans can
   read). Tool logs and private ACP streams are not a substitute for a report
   the founder can find later.
8. **Keep changes small** and on-contract: Buzz contracts first; Hermes
   extensions only when necessary ([`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md)).

### MUST NOT

1. **MUST NOT** assume the founder knows org design, RACI, OKRs, or “how
   companies run.”
2. **MUST NOT** hide bad news (upstream conflict, thin-fork risk, role
   violation, failed worktree) behind optimistic UI copy.
3. **MUST NOT** treat HTML spikes as shipped product law.
4. **MUST NOT** invent Hermes-only wire protocols that bypass Nostr/ACP when
   existing Buzz contracts already work.
5. **MUST NOT** claim Claude/Codex/etc. have Hermes profile memory.
6. **MUST NOT** propose full client rewrites (e.g. Flutter → React Native) as
   the default fix for install/test friction.
7. **MUST NOT** expand mobile into desktop parity without an explicit ask.
8. **MUST NOT** start production implementation before the Crew workflow gates
   in [`README.md`](README.md) / [`DEVELOPMENT-WORKFLOW.md`](DEVELOPMENT-WORKFLOW.md)
   when those gates apply to the task.

## How to talk about the “company”

Use this vocabulary with the founder:

| Say | Mean |
| --- | ---- |
| Room / channel | Where messages and work records live |
| Thread | One conversation/task spine in a room |
| Employee / agent | A keypair + runtime (usually a Hermes profile) |
| Assign | @mention (and later role rules) |
| Need you | Human must answer or the work is stuck |
| Report | Message in the thread with result, blocker, or question |
| Desk | Desktop app |
| Phone | Mobile app continuing the same rooms |

Avoid: “synergize,” “org operating system” without the table above, “stakeholder
alignment,” or fake process diagrams the founder did not ask for.

## Planning checklist (agent)

Before a multi-step plan, confirm:

- [ ] Read [`IDENTITY.md`](IDENTITY.md) and [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md)
- [ ] Task uses **Buzz contracts** first; Hermes-only extension justified?
- [ ] Desktop vs mobile scope matches founder product (no surprise parity)?
- [ ] Thin-fork: prefer new Crew files; minimal upstream edits?
- [ ] Explanations will stay plain; uncertainties labeled?
- [ ] Acceptance written as **observable founder outcomes** (not only “tests pass”)

## Implementation checklist (agent)

- [ ] No parallel database for room truth (relay events win)
- [ ] Agent-facing success = signed/published room updates where relevant
- [ ] Wrong-role or out-of-scope work fails loudly or asks — not silent “done”
- [ ] Docs updated only if user-visible behavior or durable rules change
- [ ] Shipped state changed (release published, slice merged, gate changed)
  → update [`STATE.md`](STATE.md) in the same PR
- [ ] New sticky choice → append [`DECISIONS.md`](DECISIONS.md)

## When stuck with the founder

Ask **one** concrete question at a time. Prefer choices:

```text
A) … (recommended) — …
B) …
```

Do not interview for abstract “vision” already answered in
`FOUNDER-PRODUCT.md`.

## Honesty examples

**Good:**  
“I don’t know how a real company would staff this. Simplest rule: only the
agent marked `code` may edit the repo; others must @ that agent or ask you.”

**Bad:**  
“We’ll cascade RACI across the value stream so matrixed ownership scales.”

**Good:**  
“Buzz already has mention → ACP turn. We should not add a Hermes-only queue
unless we prove the generic path cannot express role checks.”

**Bad:**  
“We need a Hermes fabric for orchestration.”

## Related

- Product direction: [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md)
- Hermes ops: [`HERMES.md`](HERMES.md)
- Decisions log: [`DECISIONS.md`](DECISIONS.md)
