import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { JSDOM } from "jsdom";
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
const AGENT = "a".repeat(64);
const agent = {
  pubkey: AGENT,
  name: "Agent",
  status: "stopped",
  backend: { type: "local" },
  respondTo: "owner-only",
  respondToAllowlist: [],
};
const channel = "11111111-1111-4111-8111-111111111111";
before(() => {
  Object.assign(globalThis, {
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
    localStorage: dom.window.localStorage,
    window: dom.window,
  });
  window.__TAURI_INTERNALS__ = {
    invoke: async (command) => {
      if (command === "sync_agents_to_active_huddle") return;
      throw new Error(`Unexpected command: ${command}`);
    },
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__;
});
after(() => dom.window.close());
function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function draft(extra = {}) {
  return {
    capturedChannelId: channel,
    capturedThreadContext: null,
    trimmed: "hello",
    mentionPubkeys: [AGENT],
    nonMemberPubkeys: [],
    savedContent: "hello",
    savedImeta: [],
    queuedAttachments: [],
    savedSpoileredAttachmentUrls: new Set(),
    sentDraftKey: null,
    recoveryDraftKey: null,
    savedMentionRefs: [],
    audienceGeneration: 0,
    audienceRevision: null,
    explicitAgentPubkeys: [],
    addressedAgentPubkeys: [],
    newlyPinnedAgentPubkeys: [],
    inlineAgentMentionPubkeys: [],
    ...extra,
  };
}
async function renderSend(options = {}) {
  const { renderHook, act } = await import("@testing-library/react");
  const { useMentionSendComplete } = await import(
    "./useMentionSendComplete.ts"
  );
  const { useEnsureAgentMentionsReady } = await import(
    "./useEnsureAgentMentionsReady.ts"
  );
  const { useActivePreparedLinkPreviews } = await import(
    "./useActivePreparedLinkPreviews.ts"
  );
  const { useEffect } = await import("react");
  const events = [];
  const wakes = [];
  const errors = [];
  const restored = [];
  const persisted = [];
  const contentRef = { current: "hello" };
  const mounted = options.mounted ?? { current: true };
  const getAgents = async () => new Map([[AGENT, agent]]);
  const noop = () => {};
  const rendered = renderHook(() => {
    const preparations = useActivePreparedLinkPreviews();
    useEffect(() => {
      if (options.preparation) preparations.add(options.preparation);
      return () => {
        mounted.current = false;
      };
    }, []);
    const ready = useEnsureAgentMentionsReady({
      getManagedAgentsByPubkey: getAgents,
      getPersonas: async () => [],
      memberPubkeys: options.member ? new Set() : new Set([AGENT]),
      attachAgentToChannel: async (input) => {
        events.push("membership-start");
        await options.membership;
        events.push("membership-ready");
        input.detachedStart(input.agent);
      },
    });
    return useMentionSendComplete({
      activePreparedLinkPreviews: preparations,
      channelIdRef: { current: channel },
      clearComposer: () => {
        events.push("clear");
        contentRef.current = "";
      },
      contentRef,
      drafts: {
        loadDraft: () => null,
        markDraftSent: noop,
        persistDraft: (...args) => persisted.push(args),
      },
      ensureManagedAgentMentionsReady: ready,
      detachedStart: (target, floor) => {
        events.push("wake");
        wakes.push({ target, floor });
        return true;
      },
      getManagedAgentsByPubkey: getAgents,
      hasUnsavedMedia: () => false,
      isCompleteSendPendingRef: { current: false },
      isMountedRef: mounted,
      mentions: {
        isAgentPubkey: () => true,
        restoreDraftMentionRefs: noop,
        revalidateMentionPubkeys: options.revalidate ?? (async (keys) => keys),
      },
      onSendRef: {
        current: async (...args) => {
          events.push("publish");
          await options.publish?.(...args);
          events.push("accepted");
        },
      },
      restoreQueuedAttachments: noop,
      richText: { setContent: noop },
      setContent: (content) => restored.push(content),
      setIsCompleteSendPending: noop,
      setNonMemberPromptError: (error) => errors.push(error),
      setPendingImeta: noop,
    });
  });
  return { rendered, act, events, wakes, errors, restored, persisted };
}
test("completion publishes before waking and does not await a cold launch", async () => {
  const publish = deferred();
  const send = await renderSend({ publish: () => publish.promise });
  let completion;
  await send.act(async () => {
    completion = send.rendered.result.current(draft(), [AGENT]);
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  assert.ok(send.events.includes("publish"));
  assert.equal(send.wakes.length, 0);
  await send.act(async () => {
    publish.resolve();
    await completion;
  });
  assert.deepEqual(send.events.slice(-2), ["accepted", "wake"]);
  assert.equal(send.wakes[0].target.pubkey, AGENT);
  send.rendered.unmount();
});
test("publication rejection never flushes prepared or persona wakes", async () => {
  const send = await renderSend({
    publish: async () => {
      throw new Error("relay rejected");
    },
  });
  await send.act(async () => {
    await send.rendered.result.current(
      draft({ agentsToWake: [{ agent, replayFloorUnix: 900 }] }),
      [AGENT],
    );
  });
  assert.ok(send.events.includes("publish"));
  assert.equal(send.wakes.length, 0);
  send.rendered.unmount();
});
test("membership completes before publication and its launch waits for acceptance", async () => {
  const membership = deferred();
  const send = await renderSend({
    member: true,
    membership: membership.promise,
  });
  let completion;
  await send.act(async () => {
    completion = send.rendered.result.current(draft(), [AGENT]);
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  assert.deepEqual(
    send.events.filter((event) => event !== "clear"),
    ["membership-start"],
  );
  await send.act(async () => {
    membership.resolve();
    await completion;
  });
  assert.ok(
    send.events.indexOf("membership-ready") < send.events.indexOf("publish"),
  );
  assert.ok(send.events.indexOf("accepted") < send.events.indexOf("wake"));
  send.rendered.unmount();
});
test("persona and readiness queues retain their earliest enqueue floor", async () => {
  const send = await renderSend();
  await send.act(async () => {
    await send.rendered.result.current(
      draft({ agentsToWake: [{ agent, replayFloorUnix: 900 }] }),
      [AGENT],
    );
  });
  assert.equal(send.wakes.length, 1);
  assert.equal(send.wakes[0].floor, 900);
  send.rendered.unmount();
});
test("authorization removed at publication excludes that queued wake", async () => {
  let checks = 0;
  const send = await renderSend({
    revalidate: async (keys) => (++checks === 1 ? keys : []),
  });
  await send.act(async () => {
    await send.rendered.result.current(draft(), [AGENT]);
  });
  assert.ok(send.events.includes("accepted"));
  assert.equal(send.wakes.length, 0);
  send.rendered.unmount();
});
test("unmounted preflight aborts publication and queued persona wakes", async () => {
  const send = await renderSend({ mounted: { current: false } });
  await send.act(async () => {
    await send.rendered.result.current(
      draft({ agentsToWake: [{ agent, replayFloorUnix: 900 }] }),
      [AGENT],
    );
  });
  assert.equal(send.events.includes("publish"), false);
  assert.equal(send.wakes.length, 0);
  send.rendered.unmount();
});

test("preview preparation and relay acceptance both precede the queued wake", async () => {
  const preview = deferred();
  const publish = deferred();
  const controller = new AbortController();
  let released = false;
  let published;
  const send = await renderSend({
    publish: (...args) => {
      published = args;
      return publish.promise;
    },
  });
  let completion;
  await send.act(async () => {
    completion = send.rendered.result.current(
      draft({
        preparedLinkPreviews: {
          promise: preview.promise,
          signal: controller.signal,
          release: () => {
            released = true;
          },
        },
      }),
      [AGENT],
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  assert.equal(send.events.includes("publish"), false);
  assert.equal(send.wakes.length, 0);
  await send.act(async () => {
    preview.resolve({ status: "ready", tags: [["link-preview", "none"]] });
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  assert.deepEqual(published[2], [["link-preview", "none"]]);
  assert.equal(
    published[5],
    true,
    "prepared snapshots use the atomic REST send",
  );
  assert.equal(send.wakes.length, 0);
  await send.act(async () => {
    publish.resolve();
    await completion;
  });
  assert.equal(send.wakes.length, 1);
  assert.ok(send.events.indexOf("accepted") < send.events.indexOf("wake"));
  assert.equal(released, true);
  send.rendered.unmount();
});

test("plain send clears during preview preparation and rechecks authorization before publishing", async () => {
  const preview = deferred();
  const controller = new AbortController();
  const checks = [];
  let previewReady = false;
  const send = await renderSend({
    revalidate: async (keys) => {
      checks.push(previewReady);
      return keys;
    },
  });
  let completion;
  await send.act(async () => {
    completion = send.rendered.result.current(
      draft({
        mentionPubkeys: [],
        preparedLinkPreviews: {
          promise: preview.promise,
          signal: controller.signal,
          release: () => {},
        },
      }),
      [],
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  assert.deepEqual(send.events, ["clear"]);
  assert.deepEqual(checks, [false]);
  await send.act(async () => {
    previewReady = true;
    preview.resolve({ status: "ready", tags: [["link-preview", "none"]] });
    await completion;
  });
  assert.deepEqual(checks, [false, true]);
  assert.deepEqual(send.events, ["clear", "publish", "accepted"]);
  assert.equal(send.wakes.length, 0);
  send.rendered.unmount();
});

test("accepted preview survives navigation while preflight and community changes cancel safely", async () => {
  const store = await import("../lib/linkPreviewPreparationStore.ts");
  for (const scenario of ["navigation", "preflight", "community"]) {
    const preview = deferred();
    const admission = deferred();
    const candidate = { href: `https://example.com/${scenario}` };
    store.__linkPreviewPreparationTest.jobs.set(candidate.href, {
      controller: new AbortController(),
      promise: preview.promise,
      fallbackTag: null,
      resolvedTag: null,
      settled: false,
      settledAt: null,
    });
    const preparation = store.prepareBackgroundLinkPreviews([candidate]);
    let published;
    const send = await renderSend({
      preparation,
      revalidate: async (keys) => {
        if (scenario === "preflight") await admission.promise;
        return keys;
      },
      publish: (...args) => {
        published = args;
      },
    });
    let completion;
    await send.act(async () => {
      completion = send.rendered.result.current(
        draft({
          mentionPubkeys: [],
          recoveryDraftKey: channel,
          preparedLinkPreviews: preparation,
        }),
        [],
      );
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    assert.deepEqual(send.events, scenario === "preflight" ? [] : ["clear"]);
    send.rendered.unmount();
    assert.equal(preparation.signal.aborted, scenario === "preflight");
    if (scenario === "community") store.resetLinkPreviewPreparations();
    await send.act(async () => {
      admission.resolve();
      preview.resolve(["link-preview", "none"]);
      await completion;
    });
    if (scenario === "navigation") {
      assert.equal(published[3], channel);
      assert.deepEqual(send.events, ["clear", "publish", "accepted"]);
    } else {
      assert.equal(published, undefined);
      assert.equal(preparation.signal.aborted, true);
    }
    assert.deepEqual(send.persisted, []);
    assert.deepEqual(send.restored, []);
    store.__linkPreviewPreparationTest.reset();
  }
});

test("cancelled plain preview restores a mounted draft without persisting into a switched community", async () => {
  for (const unmounted of [false, true]) {
    const preview = deferred();
    const controller = new AbortController();
    const mounted = { current: true };
    const send = await renderSend({ mounted });
    let completion;
    await send.act(async () => {
      completion = send.rendered.result.current(
        draft({
          mentionPubkeys: [],
          recoveryDraftKey: channel,
          preparedLinkPreviews: {
            promise: preview.promise,
            signal: controller.signal,
            release: () => {},
          },
        }),
        [],
      );
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    assert.deepEqual(send.events, ["clear"]);
    await send.act(async () => {
      mounted.current = !unmounted;
      controller.abort();
      preview.resolve({ status: "cancelled" });
      await completion;
    });
    assert.deepEqual(send.events, ["clear"]);
    assert.deepEqual(send.restored, unmounted ? [] : ["hello"]);
    assert.deepEqual(send.persisted, []);
    assert.equal(send.wakes.length, 0);
    send.rendered.unmount();
  }
});

test("cancelled preview preparation cannot publish or wake the agent", async () => {
  const controller = new AbortController();
  const send = await renderSend();
  let released = false;
  await send.act(async () => {
    await send.rendered.result.current(
      draft({
        preparedLinkPreviews: {
          promise: Promise.resolve({ status: "cancelled" }),
          signal: controller.signal,
          release: () => {
            released = true;
          },
        },
        agentsToWake: [{ agent, replayFloorUnix: 900 }],
      }),
      [AGENT],
    );
  });
  assert.equal(send.events.includes("publish"), false);
  assert.equal(send.wakes.length, 0);
  assert.equal(released, true);
  send.rendered.unmount();
});

test("workspace lookup failure leaves the text draft untouched and never publishes or wakes", async () => {
  const { relayClient } = await import("@/shared/api/relayClient");
  const originalFetch = relayClient.fetchEvents;
  const originalInvoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = async (command, args) =>
    command === "get_identity"
      ? { pubkey: "b".repeat(64), display_name: "Owner" }
      : originalInvoke(command, args);
  relayClient.fetchEvents = async () => {
    throw new Error("relay unavailable");
  };
  const send = await renderSend();
  try {
    await send.act(async () => {
      await send.rendered.result.current(
        draft({ explicitAgentPubkeys: [AGENT] }),
        [AGENT],
      );
    });
    assert.equal(send.events.includes("clear"), false);
    assert.equal(send.events.includes("publish"), false);
    assert.equal(send.wakes.length, 0);
    assert.ok(
      send.errors.some((message) =>
        message.includes("Could not resolve Project workspace"),
      ),
    );
  } finally {
    send.rendered.unmount();
    relayClient.fetchEvents = originalFetch;
    window.__TAURI_INTERNALS__.invoke = originalInvoke;
  }
});
