import assert from "node:assert/strict";
import test from "node:test";

import {
  FAILURE_NOTICE_E_MARKER,
  FAILURE_NOTICE_TAG,
  parseFailureNotice,
} from "./failureNotice.ts";

test("parseFailureNotice reads cause and failed markers only", () => {
  const root = "a".repeat(64);
  const failed = "b".repeat(64);
  const notice = parseFailureNotice([
    ["h", "channel"],
    [FAILURE_NOTICE_TAG, "retry_exhausted"],
    ["e", root, "", "root"],
    ["e", root, "", "reply"],
    ["e", failed, "", FAILURE_NOTICE_E_MARKER],
    ["e", root, "", FAILURE_NOTICE_E_MARKER],
  ]);
  assert.deepEqual(notice, {
    cause: "retry_exhausted",
    failedEventIds: [failed, root],
  });
});

test("parseFailureNotice returns null without failure_notice tag", () => {
  assert.equal(
    parseFailureNotice([["e", "a".repeat(64), "", FAILURE_NOTICE_E_MARKER]]),
    null,
  );
});

test("parseFailureNotice allows empty failed ids", () => {
  assert.deepEqual(parseFailureNotice([[FAILURE_NOTICE_TAG, "auth"]]), {
    cause: "auth",
    failedEventIds: [],
  });
});
