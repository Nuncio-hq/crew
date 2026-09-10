import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import {
  closeToolPane,
  getToolPaneSnapshot,
  openToolPane,
  resetToolPaneForTests,
  setToolPaneTab,
  setToolPaneView,
  setToolPanePoppedOut,
  invalidateThreadToolPaneScope,
  bindThreadToolPaneView,
  captureThreadToolPaneRemoval,
} from "./toolPaneStore.ts";

const scope = (root = "a", overrides = {}) => ({
  relayUrl: "wss://crew.example",
  viewerPubkey: "1".repeat(64),
  channelId: "11111111-1111-1111-1111-111111111111",
  rootEventId: root.repeat(64),
  ...overrides,
});
beforeEach(resetToolPaneForTests);

test("first explicit thread opening selects Context", () => {
  openToolPane(undefined, scope());
  assert.equal(getToolPaneSnapshot().tab, "context");
});

test("thread selection returns only on explicit reopening and never leaks to another root", () => {
  openToolPane(undefined, scope());
  setToolPaneTab("browser");
  closeToolPane();
  assert.equal(getToolPaneSnapshot().open, false);
  openToolPane(undefined, scope("b"));
  assert.equal(getToolPaneSnapshot().tab, "context");
  closeToolPane();
  openToolPane(undefined, scope());
  assert.equal(getToolPaneSnapshot().tab, "browser");
});

test("relay, viewer and channel each isolate thread selection", () => {
  for (const overrides of [
    { relayUrl: "wss://other.example" },
    { viewerPubkey: "2".repeat(64) },
    { channelId: "22222222-2222-2222-2222-222222222222" },
  ]) {
    resetToolPaneForTests();
    openToolPane("browser", scope());
    closeToolPane();
    openToolPane(undefined, scope("a", overrides));
    assert.equal(getToolPaneSnapshot().tab, "context");
  }
});

test("invalid thread root cannot open a resource pane", () => {
  openToolPane("browser", scope("invalid"));
  assert.equal(getToolPaneSnapshot().open, false);
});

test("community reset forgets thread selection", () => {
  openToolPane("browser", scope());
  resetToolPaneForTests();
  openToolPane(undefined, scope());
  assert.equal(getToolPaneSnapshot().tab, "context");
});

test("navigation and return restore selection while leaving presentation closed", () => {
  openToolPane("plans", scope());
  setToolPaneView(scope("b"));
  assert.deepEqual(getToolPaneSnapshot(), {
    open: false,
    poppedOut: false,
    tab: "context",
  });
  setToolPaneView(scope());
  assert.deepEqual(getToolPaneSnapshot(), {
    open: false,
    poppedOut: false,
    tab: "plans",
  });
});

test("128-record LRU evicts oldest selection, preserving explicitly revisited selection", () => {
  const numberedScope = (n) =>
    scope("a", { rootEventId: n.toString(16).padStart(64, "0") });
  for (let n = 0; n < 128; n++) openToolPane("plans", numberedScope(n));
  openToolPane(undefined, numberedScope(0));
  openToolPane("plans", numberedScope(128));
  openToolPane(undefined, numberedScope(1));
  assert.equal(getToolPaneSnapshot().tab, "context");
  openToolPane(undefined, numberedScope(0));
  assert.equal(getToolPaneSnapshot().tab, "plans");
});

test("canonical relay and viewer variants restore the same selection", () => {
  const original = scope("a", { viewerPubkey: "a".repeat(64) });
  openToolPane("plans", original);
  closeToolPane();
  openToolPane(undefined, {
    ...original,
    relayUrl: " WSS://CREW.EXAMPLE/ ",
    viewerPubkey: "A".repeat(64),
  });
  assert.equal(getToolPaneSnapshot().tab, "plans");
});

test("confirmed invalidation closes current presentation and removes selection", () => {
  openToolPane("plans", scope());
  invalidateThreadToolPaneScope(scope());
  assert.equal(getToolPaneSnapshot().open, false);
  openToolPane();
  assert.equal(getToolPaneSnapshot().open, false);
  setToolPaneView(scope("b"));
  openToolPane(undefined, scope());
  assert.equal(getToolPaneSnapshot().tab, "context");
});

test("thread presentation cannot pop out; returning to channel preserves channel tab", () => {
  openToolPane("browser");
  setToolPaneView(scope());
  openToolPane("plans");
  setToolPanePoppedOut(true);
  assert.equal(getToolPaneSnapshot().poppedOut, false);
  setToolPaneView(null);
  assert.deepEqual(getToolPaneSnapshot(), {
    open: false,
    poppedOut: false,
    tab: "browser",
  });
  openToolPane();
  setToolPaneView(null);
  assert.equal(getToolPaneSnapshot().open, true);
});

test("stale A unmount cannot close B or a newer A presentation", () => {
  const releaseOldA = bindThreadToolPaneView(scope());
  bindThreadToolPaneView(scope("b"));
  bindThreadToolPaneView(scope());
  openToolPane("plans");
  releaseOldA();
  assert.equal(getToolPaneSnapshot().open, true);
  assert.equal(getToolPaneSnapshot().tab, "plans");
});

test("confirmed root removal clears only that root and self revocation clears the channel", () => {
  const remove = captureThreadToolPaneRemoval(
    scope().relayUrl,
    scope().viewerPubkey,
  );
  openToolPane("plans", scope());
  openToolPane("activity", scope("b"));
  remove(scope().channelId, scope().rootEventId);
  assert.equal(getToolPaneSnapshot().open, true);
  openToolPane(undefined, scope());
  assert.equal(getToolPaneSnapshot().tab, "context");
  openToolPane(undefined, scope("b"));
  assert.equal(getToolPaneSnapshot().tab, "activity");
  remove(scope().channelId, undefined, "2".repeat(64));
  assert.equal(getToolPaneSnapshot().open, true);
  remove(scope().channelId, undefined, scope().viewerPubkey);
  assert.equal(getToolPaneSnapshot().open, false);
  openToolPane(undefined, scope("b"));
  assert.equal(getToolPaneSnapshot().tab, "context");
});

test("late removal from before a community reset cannot clear new selection", () => {
  const remove = captureThreadToolPaneRemoval(
    scope().relayUrl,
    scope().viewerPubkey,
  );
  resetToolPaneForTests();
  openToolPane("plans", scope());
  remove(scope().channelId);
  assert.equal(getToolPaneSnapshot().open, true);
  assert.equal(getToolPaneSnapshot().tab, "plans");
});
