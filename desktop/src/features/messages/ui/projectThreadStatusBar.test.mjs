import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

describe("project thread sticky status bar", () => {
  it("mounts the status bar outside the scroll region in MessageThreadPanel", () => {
    const panel = readFileSync(join(here, "MessageThreadPanel.tsx"), "utf8");
    const body = readFileSync(
      join(here, "ThreadPanelDeclaredPlansBody.tsx"),
      "utf8",
    );
    const scrollStart = panel.indexOf("const threadScrollRegion");
    const wrapperMount = panel.indexOf("<ThreadPanelDeclaredPlansBody");
    const scrollUsage = panel.indexOf("{threadScrollRegion}", wrapperMount);
    const stickyMount = body.indexOf("<ProjectThreadWorkspacePanel");
    const childrenUsage = body.indexOf("{children}", stickyMount);
    assert.ok(scrollStart > 0, "scroll region declaration exists");
    assert.ok(wrapperMount > 0, "declared-plans body wraps the thread pane");
    assert.ok(stickyMount > 0, "status bar is mounted");
    assert.ok(
      wrapperMount > scrollStart,
      "status bar wrapper is after the scroll region is defined",
    );
    assert.ok(
      scrollUsage > wrapperMount,
      "scroll region is a child of the declared-plans body",
    );
    assert.ok(
      childrenUsage > stickyMount,
      "status bar is rendered before the scroll region child",
    );
    assert.equal(body.includes("ProjectThreadWorkspacePanel"), true);
    // Must not remount inside the head/message scroll content.
    const headBlock = panel.slice(
      panel.indexOf('data-testid="message-thread-head"'),
      panel.indexOf('data-testid="message-thread-replies"'),
    );
    assert.equal(
      headBlock.includes("ProjectThreadWorkspacePanel"),
      false,
      "status bar must not sit inside the scrollable thread head",
    );
    assert.equal(
      headBlock.includes("ThreadPanelDeclaredPlansBody"),
      false,
      "declared-plans wrapper must not sit inside the scrollable thread head",
    );
  });

  it("suppresses composer bot activity when the open thread has project context", () => {
    const chrome = readFileSync(
      join(here, "useMessageThreadPanelChrome.ts"),
      "utf8",
    );
    const panel = readFileSync(join(here, "MessageThreadPanel.tsx"), "utf8");
    assert.match(chrome, /projectThreadStickyBarOwnsAgentSignal/);
    assert.match(chrome, /stickyBarOwnsAgentSignal/);
    assert.match(chrome, /showComposerBotActivity/);
    assert.match(panel, /showComposerBotActivity && activityAccessoryContent/);
    // The rule must come from the shared helper, which is unit-tested against
    // the bar's real visibility. Re-deriving it inline here is how the two
    // drifted apart before: composer suppressed while the bar never rendered.
    assert.equal(
      chrome.includes("parseProjectThreadContext(threadHead.body)"),
      false,
      "suppression must not re-derive project context inline",
    );
    assert.equal(
      panel.includes("parseProjectThreadContext(threadHead.body)"),
      false,
      "suppression must not re-derive project context inline in the panel",
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
