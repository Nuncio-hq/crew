import { parseHandoffTag } from "./handoffTag.ts";

const digest = "a".repeat(64);
const executor = "b".repeat(64);

const parsed = parseHandoffTag([["crew-handoff", executor, digest, "extra"]]);
if (!parsed || parsed.executor !== executor || parsed.goalDigest !== digest) {
  throw new Error("handoff tag should parse and ignore extra fields");
}
if (parseHandoffTag([["crew-handoff", "short", digest]]) !== null) {
  throw new Error("invalid executor must be ignored");
}
if (parseHandoffTag([["crew-evidence", "metrics"]]) !== null) {
  throw new Error("other tags are not handoffs");
}
