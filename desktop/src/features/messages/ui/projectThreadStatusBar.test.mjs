import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

describe("project thread sticky status bar", () => {
  it("mounts the status bar outside the scroll region in MessageThreadPanel", () => {
    const source = readFileSync(join(here, "MessageThreadPanel.tsx"), "utf8");
    const scrollStart = source.indexOf("const threadScrollRegion");
    const stickyMount = source.indexOf("<ProjectThreadWorkspacePanel");
    const scrollUsage = source.indexOf("{threadScrollRegion}", stickyMount);
    assert.ok(scrollStart > 0, "scroll region declaration exists");
    assert.ok(stickyMount > 0, "status bar is mounted");
    assert.ok(
      stickyMount > scrollStart,
      "status bar mount is after the scroll region is defined",
    );
    assert.ok(
      scrollUsage > stickyMount,
      "status bar is rendered before the scroll region child",
    );
    assert.equal(source.includes("ProjectThreadWorkspacePanel"), true);
    // Must not remount inside the head/message scroll content.
    const headBlock = source.slice(
      source.indexOf('data-testid="message-thread-head"'),
      source.indexOf('data-testid="message-thread-replies"'),
    );
    assert.equal(
      headBlock.includes("ProjectThreadWorkspacePanel"),
      false,
      "status bar must not sit inside the scrollable thread head",
    );
  });

  it("suppresses composer bot activity when the open thread has project context", () => {
    const source = readFileSync(join(here, "MessageThreadPanel.tsx"), "utf8");
    assert.match(source, /projectThreadStickyBarOwnsAgentSignal/);
    assert.match(source, /stickyBarOwnsAgentSignal/);
    assert.match(source, /showComposerBotActivity/);
    assert.match(source, /showComposerBotActivity && activityAccessoryContent/);
    // The rule must come from the shared helper, which is unit-tested against
    // the bar's real visibility. Re-deriving it inline here is how the two
    // drifted apart before: composer suppressed while the bar never rendered.
    assert.equal(
      source.includes("parseProjectThreadContext(threadHead.body)"),
      false,
      "suppression must not re-derive project context inline",
    );
  });

  it("routes sticky-bar Stop through the shared composer cancel path", () => {
    const panel = readFileSync(
      join(here, "ProjectThreadWorkspacePanel.tsx"),
      "utf8",
    );
    const threadPanel = readFileSync(
      join(here, "MessageThreadPanel.tsx"),
      "utf8",
    );
    assert.match(panel, /useComposerAgentStop/);
    assert.match(threadPanel, /channelId=\{channelId\}/);
    assert.equal(panel.includes("cancelManagedAgentTurn"), false);
  });
});
