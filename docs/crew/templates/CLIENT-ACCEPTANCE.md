# Client acceptance — paste into the thread before Gate C

Copy this block into the work thread when asking the founder to Accept.
Do not mark done from CI or tool logs alone (issue #234 / D-070).

```text
## Ready for client try (Gate C)

### 3-line story
- Hired: <what Oscar asked for>
- Works now: <what he can do / see>
- Deliberately not done: <out of scope this slice>

### 2-minute try script
1. <exact click / message / route>
2. <exact click / message>
3. Expected: <observable outcome>

### Evidence
- <evidence tied to acceptance criteria: workflow result, test, measurements, or visual capture>
- Environment: <Mock E2E | Local app | Multi-session | relevant non-code context>
- Checks: <what ran, outcome, and CI link when applicable>

### Honest limit
- <e.g. mock-only; one agent; not load-tested>

Please run the try script (~5 min). Match → Accept. Mismatch / fog →
Reject + one sentence. Mid-flight Need you → answer; do not debug for us.
```
