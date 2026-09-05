import assert from "node:assert/strict";
import { after, afterEach, before, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
const timers = new Map();
let nextTimer = 0;

before(() => {
  Object.assign(globalThis, {
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    HTMLDivElement: dom.window.HTMLDivElement,
    IS_REACT_ACT_ENVIRONMENT: true,
    window: dom.window,
  });
  window.setTimeout = (callback, delay) => {
    const id = ++nextTimer;
    timers.set(id, { callback, delay });
    return id;
  };
  window.clearTimeout = (id) => timers.delete(id);
});

afterEach(async () => {
  const { cleanup } = await import("@testing-library/react");
  cleanup();
  timers.clear();
  document.body.replaceChildren();
});
after(() => dom.window.close());

async function mount(onStartReached) {
  const { renderHook } = await import("@testing-library/react");
  const { useUpwardPaginationWheel } = await import(
    "./useUpwardPaginationWheel.ts"
  );
  const host = document.createElement("div");
  const scroller = document.createElement("div");
  host.append(scroller);
  document.body.append(host);
  Object.defineProperties(scroller, {
    scrollHeight: { configurable: true, value: 2_000 },
    clientHeight: { configurable: true, value: 500 },
  });
  const hostRef = { current: host };
  let wheelCalls = 0;
  const onWheel = () => {
    wheelCalls += 1;
  };
  const hook = renderHook(
    (callback) => useUpwardPaginationWheel(hostRef, onWheel, callback),
    { initialProps: onStartReached },
  );
  return {
    ...hook,
    scroller,
    wheelCalls: () => wheelCalls,
    wheel(deltaY, ctrlKey = false) {
      const event = new window.WheelEvent("wheel", {
        bubbles: true,
        cancelable: true,
        ctrlKey,
        deltaY,
      });
      scroller.dispatchEvent(event);
      return event.defaultPrevented;
    },
  };
}

function finishQuietWindow() {
  const pending = [...timers.values()];
  timers.clear();
  for (const timer of pending) {
    assert.equal(timer.delay, 80);
    timer.callback();
  }
}

test("next upward wheel retries a previously blocked hard top without a scroll event", async () => {
  let blockedCalls = 0;
  let eligibleCalls = 0;
  const view = await mount(() => {
    blockedCalls += 1;
    return false;
  });
  let scrollEvents = 0;
  view.scroller.addEventListener("scroll", () => {
    scrollEvents += 1;
  });
  assert.equal(blockedCalls, 0, "mount must not start pagination");
  assert.equal(view.wheel(-40), false);
  assert.equal(blockedCalls, 1);
  view.rerender(() => {
    eligibleCalls += 1;
    return true;
  });
  assert.equal(eligibleCalls, 0, "eligibility alone must not auto-page");
  assert.equal(view.wheel(-40), true);
  assert.equal(eligibleCalls, 1);
  assert.equal(scrollEvents, 0);
  assert.equal(view.scroller.scrollTop, 0);
});

test("callback updates preserve the pending momentum release timer", async () => {
  const view = await mount(() => true);
  assert.equal(view.wheel(-40), true);
  const pendingTimer = [...timers.keys()];
  assert.equal(pendingTimer.length, 1);
  view.rerender(() => false);
  assert.deepEqual([...timers.keys()], pendingTimer);
  finishQuietWindow();
  assert.equal(view.wheel(-40), false, "quiet window releases suppression");
});

test("blocked paging keeps current momentum suppressed until quiet or downward input", async () => {
  const view = await mount(() => true);
  assert.equal(view.wheel(-40), true);
  view.rerender(() => false);
  assert.equal(view.wheel(-40), true);
  assert.equal(timers.size, 1);
  assert.equal(view.wheel(40), false);
  assert.equal(timers.size, 0);
  assert.equal(view.wheel(-40), false);
});

test("zoom, downward input and wheel away from the top never request older history", async () => {
  let calls = 0;
  const view = await mount(() => {
    calls += 1;
    return true;
  });
  assert.equal(view.wheel(-40, true), false);
  assert.equal(view.wheelCalls(), 0, "Ctrl+wheel preserves browser zoom");
  assert.equal(view.wheel(40), false);
  view.scroller.scrollTop = 201;
  assert.equal(view.wheel(-40), false);
  assert.equal(calls, 0);
  assert.equal(timers.size, 0);
});

test("short timelines can request history without suppressing wheel input", async () => {
  let calls = 0;
  const view = await mount(() => {
    calls += 1;
    return true;
  });
  Object.defineProperty(view.scroller, "scrollHeight", { value: 800 });
  assert.equal(view.wheel(-40), false);
  assert.equal(calls, 1);
  assert.equal(timers.size, 0);
});
