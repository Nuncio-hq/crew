# Native huddle/archive stable release integration

Status: source resolution complete; waiting for shared native compile/test gate.

Worktree `crew-wt/upstream-0522`, release `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`, actual common ancestor `4749bc7be3cdb78c2db4ce4864775ba7ab60b4cc`. Ownership only native huddle/archive plus authorized HERMES.md voice-latency section; no git index edits or live config changes.

## Decision and source evidence

Crew commit `69980949d` (#304) explicitly described STT as ports of upstream #6397 and always-on #5671. It introduced a pure VAD endpoint to preserve onset/pre-roll/hysteresis/hangover/PTT grouping. Full 0.5.22 ships that same upstream concept with newer behavior; retaining both implementations would conflict with Crew's extend-upstream rule.

Resolved `huddle/stt.rs` and `stt_tests.rs` to exact released source. This preserves the endpoint protections and adds independent local/remote speech processing, shared human-floor barge-in, and worker-exit release tests. Released operating point: silence31frames(~496ms), onset0.55, offset0.35, onset3frames, pre-roll16frames, hangover6frames, minimumvoiced12frames. No STT_FLUSH_MS runtime override returns; experimental latency options remain opt-in. Root accepted the released model superseding the partial port.

All archive files match target bytes. Crew's archive batching/cursor additions were ports of upstream #6024 and are absorbed by released source; no separate Crew persistence delta remains. All other huddle files match target except the preserved companion-window minimum-size comment; actual minimum remains720x520. Crew spoken pickup guidelines are also present in released agents.rs.

Updated only HERMES.md latency section: old no-levers/300ms statements replaced with accurate released behavior, desktop-process ownership, defaults, and no configuration mutation.

## Validation

- No conflict markers in huddle/archive trees.
- `rustfmt --edition2021 --check` on resolved STT source/tests: PASS (source parsing included).
- Exact-byte comparison across all target archive paths: PASS; all huddle paths target-exact except intentional window comment.
- First shared native cargo-check reached other subsystem errors only; no huddle/archive diagnostic in `/tmp/crew-native-release-check.log` at snapshot.
- Native compile and huddle/archive tests remain in pool worker's shared gate; no competing full compile launched.

Docs impact: minor (HERMES latency section + this report). No live audio-model/device test claimed. Unresolved: shared native compile/test completion and independent review.
