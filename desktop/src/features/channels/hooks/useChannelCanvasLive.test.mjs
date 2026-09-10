import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
before(() =>
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  }),
);
after(() => dom.window.close());

async function mount(
  t,
  { deferredSubscribe = false, deferredRead = false } = {},
) {
  const { act, renderHook } = await import("@testing-library/react");
  const React = await import("react");
  const { QueryClient, QueryClientProvider, useQuery } = await import(
    "@tanstack/react-query"
  );
  const { relayClient } = await import("@/shared/api/relayClient");
  const { getCanvas } = await import("@/shared/api/canvas");
  const { useChannelCanvasLive } = await import("./useChannelCanvasLive.ts");
  const state = {
    head: "original",
    reads: 0,
    failRead: false,
    generation: 1,
    failScope: false,
    disposed: 0,
    deferredRead,
  };
  const token = {
    scope: { owner: "a".repeat(64), community: "http://localhost" },
    workspace_generation: 1,
    identity_generation: 1,
  };
  let releaseRead;
  const readGate = new Promise((resolve) => {
    releaseRead = resolve;
  });
  window.__TAURI_INTERNALS__ = {
    invoke: async (command) => {
      if (command === "owner_operation_scope") {
        if (state.failScope) throw new Error("identity unavailable");
        return { ...token, identity_generation: state.generation };
      }
      if (command === "get_canvas") {
        state.reads++;
        const capturedHead = state.head;
        if (state.deferredRead) {
          state.deferredRead = false;
          await readGate;
        }
        if (state.failRead) throw new Error("read unavailable");
        return {
          content: "# Agreement",
          event_id: capturedHead,
          definitions: [],
          routing: [],
          assignments: [],
          dev_mcp_granted: null,
          crew_parse_error: null,
        };
      }
      throw new Error(`Unexpected ${command}`);
    },
  };
  let onEvent, release;
  const subscribeGate = new Promise((resolve) => {
    release = resolve;
  });
  const original = relayClient.subscribeLive;
  relayClient.subscribeLive = async (filter, callback) => {
    assert.deepEqual(filter, { kinds: [40100], "#h": ["channel"], limit: 1 });
    onEvent = callback;
    if (deferredSubscribe) await subscribeGate;
    return async () => {
      state.disposed++;
    };
  };
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const hook = renderHook(
    () => {
      const canvas = useQuery({
        queryKey: ["channel-canvas", "channel"],
        queryFn: () => getCanvas("channel"),
      });
      const error = useChannelCanvasLive("channel", "scope-one");
      return { canvas, error };
    },
    {
      wrapper: ({ children }) =>
        React.createElement(QueryClientProvider, { client }, children),
    },
  );
  await act(async () => {});
  return {
    state,
    client,
    result: hook.result,
    act,
    release,
    releaseRead,
    emit: (kind = 40100, channel = "channel") =>
      onEvent({ kind, tags: [["h", channel]] }),
    async tick() {
      await act(async () => {
        t.mock.timers.tick(250);
      });
    },
    unmount: hook.unmount,
    cleanup() {
      hook.unmount();
      client.clear();
      relayClient.subscribeLive = original;
      t.mock.timers.reset();
    },
  };
}

test("live canvas bursts coalesce a current-head read and transient failures preserve cached data", async (t) => {
  const h = await mount(t);
  try {
    await h.tick();
    const before = h.state.reads;
    h.state.head = "newer";
    await h.act(async () => {
      for (let i = 0; i < 50; i++) h.emit();
    });
    await h.tick();
    assert.equal(h.state.reads, before + 1);
    assert.equal(
      h.client.getQueryData(["channel-canvas", "channel"]).eventId,
      "newer",
    );
    await h.act(async () => {
      h.emit(9);
      h.emit(40100, "another-channel");
    });
    await h.tick();
    assert.equal(h.state.reads, before + 1);
    h.state.failRead = true;
    await h.act(async () => {
      h.emit();
    });
    await h.tick();
    assert.equal(
      h.client.getQueryData(["channel-canvas", "channel"]).eventId,
      "newer",
    );
    assert.match(h.result.current.error, /refresh failed/i);
  } finally {
    h.cleanup();
  }
});

test("a subscription completing after unmount releases its listener", async (t) => {
  const h = await mount(t, { deferredSubscribe: true });
  try {
    h.unmount();
    await h.act(async () => {
      h.release();
    });
    assert.equal(h.state.disposed, 1);
  } finally {
    h.cleanup();
  }
});

test("scope capture failure after subscription opens still releases its listener", async (t) => {
  const h = await mount(t, { deferredSubscribe: true });
  try {
    h.state.failScope = true;
    await h.act(async () => {
      h.release();
    });
    assert.equal(h.state.disposed, 1);
  } finally {
    h.cleanup();
  }
});

test("one live event during an older in-flight query requires a subsequent current-head read", async (t) => {
  const h = await mount(t, { deferredRead: true });
  try {
    assert.equal(h.state.reads, 1);
    h.state.head = "after-event";
    await h.act(async () => {
      h.emit();
    });
    await h.tick();
    await h.act(async () => {
      h.releaseRead();
    });
    await h.tick();
    assert.equal(
      h.state.reads,
      2,
      "the pre-event promise cannot satisfy the event's current-head read",
    );
    assert.equal(
      h.client.getQueryData(["channel-canvas", "channel"]).eventId,
      "after-event",
    );
    await h.tick();
    assert.equal(
      h.state.reads,
      2,
      "the follow-up must be bounded without another event",
    );
  } finally {
    h.releaseRead();
    h.cleanup();
  }
});

test("events queued while the joined read is settling produce one bounded follow-up", async (t) => {
  const h = await mount(t, { deferredRead: true });
  try {
    assert.equal(h.state.reads, 1);
    h.state.head = "after-event";
    await h.act(async () => {
      h.emit();
    });
    await h.tick();
    await h.act(async () => {
      h.emit();
      h.emit();
      h.releaseRead();
    });
    await h.tick();
    assert.equal(h.state.reads, 2);
    await h.tick();
    assert.equal(
      h.state.reads,
      2,
      "a burst during the joined read must not create a timer loop",
    );
    assert.equal(
      h.client.getQueryData(["channel-canvas", "channel"]).eventId,
      "after-event",
    );
  } finally {
    h.releaseRead();
    h.cleanup();
  }
});

test("scope retirement cancels a queued follow-up after the joined read", async (t) => {
  const h = await mount(t, { deferredRead: true });
  try {
    h.state.head = "after-event";
    await h.act(async () => {
      h.emit();
    });
    await h.tick();
    h.unmount();
    await h.act(async () => {
      h.releaseRead();
    });
    await h.tick();
    assert.equal(h.state.reads, 1);
    assert.equal(h.state.disposed, 1);
  } finally {
    h.releaseRead();
    h.cleanup();
  }
});

test("native scope change retires the live callback without repeated old-scope reads", async (t) => {
  const h = await mount(t);
  try {
    await h.tick();
    const before = h.state.reads;
    h.state.generation++;
    await h.act(async () => {
      h.emit();
    });
    await h.tick();
    assert.equal(h.state.disposed, 1);
    await h.act(async () => {
      h.emit();
    });
    await h.tick();
    assert.equal(h.state.reads, before);
  } finally {
    h.cleanup();
  }
});
