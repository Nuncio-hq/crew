import assert from "node:assert/strict";
import { test } from "node:test";

import { projectThreadLabel } from "./projectThreadLabel.ts";

const ROOT = `[buzz-project-context-abc]: <buzz://project-workspace?repo=x&path=%2Ftmp%2Fcrew> "title"

@Claude Opus bây giờ hãy giúp mình brainstorm, để chúng ta có thể quản lí được worktree trong 1 channel`;

test("strips project-context marker and leading mention", () => {
  const label = projectThreadLabel(ROOT);
  assert.ok(label?.startsWith("bây giờ hãy giúp mình brainstorm"));
  assert.ok(label?.endsWith("…"));
  assert.ok(!label.includes("buzz-project-context"));
  assert.ok(!label.includes("@Claude"));
});

test("strips multiple leading mentions", () => {
  const body = `@Claude Opus @Cursor Grok High Fast please ship the registry`;
  assert.equal(projectThreadLabel(body), "please ship the registry");
});

test("image-only and mentions-only roots return null", () => {
  assert.equal(projectThreadLabel("@Claude Opus [screenshot]"), null);
  assert.equal(projectThreadLabel("@Claude Opus"), null);
  assert.equal(
    projectThreadLabel(
      `[buzz-project-context-x]: <buzz://project-workspace?repo=a&path=%2Fx> "t"\n\n@Claude Opus`,
    ),
    null,
  );
});

test("root without project-context marker still derives", () => {
  assert.equal(
    projectThreadLabel("@Claude Opus fix the orphan worktree cleanup"),
    "fix the orphan worktree cleanup",
  );
});

test("truncates on a word boundary", () => {
  const body =
    "@Claude Opus " +
    "one two three four five six seven eight nine ten eleven twelve thirteen";
  const label = projectThreadLabel(body);
  assert.ok(label.endsWith("…"));
  assert.ok(!label.includes("thirteen"));
  assert.ok(!/\w…\w/.test(label));
});

test("CRLF and blank lines do not break the split", () => {
  const body =
    '[buzz-project-context-x]: <buzz://project-workspace?repo=a&path=%2Fx> "t"\r\n\r\n\r\n@Claude Opus hello world';
  assert.equal(projectThreadLabel(body), "hello world");
});
