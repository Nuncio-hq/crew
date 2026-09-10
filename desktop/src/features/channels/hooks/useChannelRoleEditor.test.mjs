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

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

async function mount({
  pending = true,
  pendingOutcome = "not_committed",
  pendingDraft = null,
  pendingEntries = null,
  store: suppliedStore = null,
} = {}) {
  const { act, renderHook } = await import("@testing-library/react");
  const { useChannelRoleEditor } = await import("./useChannelRoleEditor.ts");
  const token = {
    scope: { owner: "a".repeat(64), community: "http://localhost" },
    workspace_generation: 1,
    identity_generation: 1,
  };
  const progress = {
    operation_id: "operation-one",
    reconciled: pendingOutcome === "superseded",
    canvas_event_id: "saved-head",
    current_event_id: "saved-head",
    outcome: pendingOutcome,
    automatic_retry_at: null,
    manual_retry_required: true,
    draft: pendingDraft,
  };
  const operationFor = (value) => ({
    version: 1,
    scope: token.scope,
    id: value.operation_id,
    kind: "channel-crew-config",
    resource_key: "channel",
    revision: 1,
    created_at: 1,
    updated_at: 1,
    status: value.outcome === "superseded" ? "superseded" : "pending",
    reconciled: value.reconciled,
    payload: {},
  });
  const store = suppliedStore ?? {
    records: (pending ? (pendingEntries ?? [progress]) : []).map((value) => ({
      progress: value,
      operation: operationFor(value),
    })),
    removed: [],
  };
  const requests = [];
  const applied = [];
  const calls = [];
  window.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      calls.push(command);
      if (command === "owner_operation_scope") return token;
      if (command === "get_canvas")
        return {
          content: "# Latest",
          event_id: "latest-head",
          definitions: [
            { role_label: "Review", definition: "Inspect patches" },
          ],
          stored_assignments: {},
          stored_routing: {},
          stored_capabilities: {},
          contact_pubkey: null,
          crew_authority: "owner",
          crew_parse_state: "valid",
          routing: [],
          assignments: [],
          dev_mcp_granted: null,
          crew_parse_error: null,
        };
      if (command === "list_relay_agents") return [];
      if (command === "list_channel_crew_operations")
        return { token, value: store.records.map((record) => record.progress) };
      if (command === "owner_operation_load") {
        const id = args?.id;
        const record = store.records.find((entry) => entry.operation.id === id);
        if (!record) throw new Error("Recovery operation is missing");
        return { token, value: record.operation };
      }
      if (command === "owner_operation_remove") {
        if (store.failRemove) throw new Error("Recovery storage is busy");
        const index = store.records.findIndex(
          (entry) =>
            entry.operation.id === args.id &&
            entry.operation.revision === args.revision,
        );
        if (index < 0) throw new Error("Recovery operation changed");
        store.removed.push(args);
        store.records.splice(index, 1);
        return { token, value: null };
      }
      if (
        [
          "get_channel_crew_operation",
          "retry_channel_crew_config",
          "save_channel_crew_config",
        ].includes(command)
      ) {
        const request = deferred();
        requests.push({ command, ...request });
        return request.promise;
      }
      throw new Error(`Unexpected command ${command}`);
    },
  };
  const hook = renderHook(() =>
    useChannelRoleEditor("channel", (head) => applied.push(head)),
  );
  await act(async () => {});
  assert.ok(hook.result.current.draft, "real hook finished its initial load");
  return {
    ...hook,
    act,
    requests,
    calls,
    applied,
    store,
    reply: (outcome) => ({ token, value: { ...progress, outcome } }),
  };
}

test("reopened recovery restores the journaled draft, including superseded saves", async () => {
  for (const pendingOutcome of ["not_committed", "superseded"]) {
    const h = await mount({
      pendingOutcome,
      pendingDraft: {
        definitions: [{ label: "Submitted", definition: "Updated boundary" }],
        assignments: {},
        contact: null,
        renames: { Review: "Submitted" },
        preserved_assignments: {},
        remove_routing: [],
        remove_capabilities: [],
      },
    });
    try {
      assert.equal(h.result.current.operation, "operation-one");
      assert.equal(h.result.current.draft.roles[0].label, "Submitted");
      assert.equal(
        h.result.current.draft.roles[0].definition,
        "Updated boundary",
      );
      assert.equal(h.result.current.draft.roles[0].originalLabel, "Review");
      assert.equal(
        h.result.current.conflict,
        pendingOutcome === "superseded",
        "superseded recovery must reopen in explicit review mode",
      );
    } finally {
      h.unmount();
    }
  }
});

test("recovery selection prefers an unresolved row over old terminal history", async () => {
  const terminal = {
    operation_id: "00000000-0000-0000-0000-000000000001",
    reconciled: true,
    canvas_event_id: "old-head",
    current_event_id: "old-head",
    outcome: "superseded",
    automatic_retry_at: null,
    manual_retry_required: false,
    draft: {
      definitions: [{ label: "Old draft", definition: "Old boundary" }],
      assignments: {},
      contact: null,
      renames: {},
      preserved_assignments: {},
      remove_routing: [],
      remove_capabilities: [],
    },
  };
  const unresolved = {
    ...terminal,
    operation_id: "00000000-0000-0000-0000-000000000002",
    reconciled: false,
    outcome: "canvas_committed_announcement_pending",
    draft: {
      ...terminal.draft,
      definitions: [{ label: "Current draft", definition: "Current boundary" }],
    },
  };
  const h = await mount({ pendingEntries: [terminal, unresolved] });
  try {
    assert.equal(h.result.current.operation, unresolved.operation_id);
    assert.equal(h.result.current.progress.outcome, unresolved.outcome);
    assert.equal(h.result.current.draft.roles[0].label, "Current draft");
    assert.equal(h.result.current.conflict, false);
  } finally {
    h.unmount();
  }
});

test("late status cannot restore an old operation after reviewed draft replacement", async () => {
  const h = await mount({ pendingOutcome: "superseded" });
  try {
    let first;
    await h.act(async () => {
      first = h.result.current.refresh();
    });
    let second;
    await h.act(async () => {
      second = h.result.current.refresh();
    });
    assert.equal(h.requests.length, 2);
    await h.act(async () => {
      h.requests[1].resolve(h.reply("superseded"));
      await second;
    });
    await h.act(async () => {
      await h.result.current.loadLatest();
    });
    await h.act(async () => {
      await h.result.current.replaceDraft();
    });
    await h.act(async () => {
      h.result.current.setDraft((draft) => ({
        ...draft,
        roles: draft.roles.map((role) => ({
          ...role,
          label: "New reviewed draft",
        })),
      }));
    });
    assert.equal(h.result.current.operation, null);
    await h.act(async () => {
      h.requests[0].resolve(h.reply("applied"));
      await first;
    });
    assert.equal(
      h.applied.length,
      0,
      "late Applied must not close a newer draft",
    );
    assert.equal(
      h.result.current.operation,
      null,
      "late status must not restore the retired operation",
    );
    assert.equal(h.result.current.draft.roles[0].label, "New reviewed draft");
  } finally {
    h.unmount();
  }
});

test("an older status response cannot overwrite the newer manual retry result", async () => {
  const h = await mount();
  try {
    let first, second;
    await h.act(async () => {
      first = h.result.current.refresh();
    });
    await h.act(async () => {
      second = h.result.current.refresh(true);
    });
    assert.equal(h.requests[0].command, "get_channel_crew_operation");
    assert.equal(h.requests[1].command, "retry_channel_crew_config");
    await h.act(async () => {
      h.requests[1].resolve(h.reply("canvas_committed_announcement_pending"));
      await second;
    });
    await h.act(async () => {
      h.requests[0].resolve(h.reply("not_committed"));
      await first;
    });
    assert.equal(
      h.result.current.progress.outcome,
      "canvas_committed_announcement_pending",
    );
  } finally {
    h.unmount();
  }
});

test("same-tick saves claim the action before awaiting scope capture", async () => {
  const h = await mount({ pending: false });
  try {
    let first, second;
    await h.act(async () => {
      first = h.result.current.save();
      second = h.result.current.save();
    });
    assert.equal(
      h.requests.length,
      1,
      "one user save must dispatch one native command",
    );
    await h.act(async () => {
      h.requests[0].resolve({
        ...h.reply("not_committed"),
        value: { result: "conflict", current_event_id: "newer" },
      });
      await Promise.all([first, second]);
    });
  } finally {
    for (const request of h.requests)
      request.resolve({
        ...h.reply("not_committed"),
        value: { result: "conflict", current_event_id: "newer" },
      });
    h.unmount();
  }
});

test("a retired operation cannot surface a late read failure in the replacement draft", async () => {
  const h = await mount({ pendingOutcome: "superseded" });
  try {
    assert.equal(
      h.result.current.conflict,
      true,
      "reopened superseded recovery exposes review",
    );
    let request;
    await h.act(async () => {
      request = h.result.current.refresh();
    });
    await h.act(async () => {
      await h.result.current.loadLatest();
    });
    await h.act(async () => {
      await h.result.current.replaceDraft();
    });
    await h.act(async () => {
      h.requests[0].reject(new Error("stale read failed"));
      await request;
    });
    assert.equal(h.result.current.error, null);
    assert.equal(h.result.current.operation, null);
  } finally {
    h.unmount();
  }
});

test("same-tick manual retries claim one action while status reads remain read-only", async () => {
  const h = await mount();
  try {
    let first, second, read;
    await h.act(async () => {
      first = h.result.current.refresh(true);
      second = h.result.current.refresh(true);
      read = h.result.current.refresh();
    });
    assert.equal(h.requests.length, 1);
    assert.equal(h.requests[0].command, "retry_channel_crew_config");
    await h.act(async () => {
      h.requests[0].resolve(h.reply("not_committed"));
      await Promise.all([first, second, read]);
    });
    assert.equal(h.result.current.busy, false);
  } finally {
    h.unmount();
  }
});

test("discard removes only the superseded journal before a later reopen", async () => {
  const h = await mount({ pendingOutcome: "superseded" });
  const store = h.store;
  store.unrelated = { id: "unrelated-owner-operation" };
  try {
    await h.act(async () => {
      await h.result.current.loadLatest();
    });
    await h.act(async () => {
      await h.result.current.replaceDraft();
    });
    assert.deepEqual(
      store.removed.map(({ id, revision }) => ({ id, revision })),
      [{ id: "operation-one", revision: 1 }],
    );
  } finally {
    h.unmount();
  }
  assert.equal(store.records.length, 0, "the discarded journal is removed");
  assert.equal(
    store.unrelated.id,
    "unrelated-owner-operation",
    "discard does not sweep unrelated owner operations",
  );
  const reopened = await mount({ store });
  try {
    assert.equal(reopened.result.current.operation, null);
    assert.equal(reopened.result.current.progress, null);
  } finally {
    reopened.unmount();
  }
});

test("failed durable discard keeps the draft and recovery action available", async () => {
  const h = await mount({ pendingOutcome: "superseded" });
  const store = h.store;
  store.failRemove = true;
  try {
    await h.act(async () => {
      await h.result.current.loadLatest();
    });
    const before = h.result.current.draft.roles[0].label;
    await h.act(async () => {
      await h.result.current.replaceDraft();
    });
    assert.equal(h.result.current.operation, "operation-one");
    assert.equal(h.result.current.draft.roles[0].label, before);
    assert.match(h.result.current.error, /Recovery storage is busy/);
    assert.equal(store.records.length, 1);
    assert.equal(store.removed.length, 0);
  } finally {
    h.unmount();
  }
});
