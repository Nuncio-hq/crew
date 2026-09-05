import assert from "node:assert/strict";
import { after, afterEach, before, test } from "node:test";
import { JSDOM } from "jsdom";
import {
  invalidateThreadForgePullRequestStore,
  resetThreadForgePullRequestStore,
  useThreadForgePullRequest,
} from "./threadForgePullRequestStore.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
const requests = [];
let act;
let renderHook;
let cleanup;

before(async () => {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: dom.window,
  });
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: dom.window.navigator,
  });
  Object.assign(globalThis, {
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  });
  dom.window.__TAURI_INTERNALS__ = {
    invoke(command, payload) {
      assert.ok(
        ["get_thread_forge_pr_detail", "get_thread_forge_pr_diff"].includes(
          command,
        ),
      );
      return new Promise((resolve) =>
        requests.push({ command, payload, resolve }),
      );
    },
  };
  ({ act, renderHook, cleanup } = await import("@testing-library/react"));
});

afterEach(() => {
  cleanup();
  resetThreadForgePullRequestStore();
  requests.length = 0;
});
after(() => dom.window.close());

function details() {
  return requests.filter(
    ({ command }) => command === "get_thread_forge_pr_detail",
  );
}
function diffs() {
  return requests.filter(
    ({ command }) => command === "get_thread_forge_pr_diff",
  );
}

// Complete one real request pair at a time. An accidental reload creates a new
// pending pair, so the count assertion fails without an infinite promise loop.
async function finishPair(index) {
  await act(async () => {
    details()[index].resolve({
      availability: "available",
      rateLimitedUntil: null,
      detail: { number: details()[index].payload.number },
    });
    diffs()[index].resolve({
      availability: "available",
      rateLimitedUntil: null,
      diff: null,
    });
  });
}

const initialProps = {
  ref: { owner: "Nuncio-hq", name: "crew", number: 123 },
  worktreePath: "/workspace/crew",
  baseRef: "main",
};
function mount(props = initialProps) {
  return renderHook(
    ({ ref, worktreePath, baseRef }) =>
      // ThreadPrHub constructs this value anew for every render.
      useThreadForgePullRequest(ref ? { ...ref } : null, worktreePath, baseRef),
    { initialProps: props },
  );
}

test("one mutation invalidation and refresh settle without repeated PR requests", async () => {
  const view = mount();
  assert.equal(details().length, 1);
  await finishPair(0);
  assert.equal(view.result.current.snapshot.status, "ready");
  const initialRefresh = view.result.current.refresh;
  view.rerender({ ...initialProps });
  assert.equal(view.result.current.refresh, initialRefresh);
  assert.equal(details().length, 1);

  let refreshed;
  act(() => {
    invalidateThreadForgePullRequestStore();
    // The comment action invalidates then explicitly refreshes. Both should
    // share the same in-flight request instead of starting another pair.
    refreshed = view.result.current.refresh();
  });
  assert.equal(details().length, 2);
  await finishPair(1);
  await refreshed;
  view.rerender({ ...initialProps });
  assert.equal(details().length, 2);
  assert.equal(diffs().length, 2);
  assert.equal(view.result.current.snapshot.status, "ready");
});

test("identity changes load their PR while refresh uses the latest worktree and base", async () => {
  const view = mount({ ...initialProps, ref: null });
  assert.equal(details().length, 0);
  view.rerender(initialProps);
  await finishPair(0);

  const nextRef = { owner: "another-owner", name: "another-repo", number: 456 };
  const nextProps = { ...initialProps, ref: nextRef };
  view.rerender(nextProps);
  assert.deepEqual(details()[1].payload, nextRef);
  await finishPair(1);
  assert.equal(view.result.current.snapshot.detail.detail.number, 456);

  const nextWorktree = {
    ...nextProps,
    worktreePath: "/workspace/other-worktree",
  };
  view.rerender(nextWorktree);
  assert.equal(diffs()[2].payload.worktreePath, nextWorktree.worktreePath);
  await finishPair(2);

  const nextBase = { ...nextWorktree, baseRef: "release" };
  view.rerender(nextBase);
  let refreshed;
  act(() => {
    refreshed = view.result.current.refresh();
  });
  assert.equal(details().length, 4);
  assert.deepEqual(diffs()[3].payload, {
    ...nextRef,
    worktreePath: nextWorktree.worktreePath,
    baseRef: "release",
  });
  await finishPair(3);
  await refreshed;
  assert.equal(details().length, 4);
});
