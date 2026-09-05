import assert from "node:assert/strict";
import { after, before, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
const dom = new JSDOM("<!doctype html>", { url: "http://localhost" });
const pubkey = "a".repeat(64);
const backend = { type: "provider", id: "test-provider", config: {} };
const agent = { pubkey, name: "Agent", status: "not_deployed", backend };
let calls = [];
let membershipError = false;
before(() => {
  globalThis.window = dom.window;
  window.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "create_managed_agent")
        return { agent, spawn_error: null };
      if (command === "add_channel_members")
        return {
          added: membershipError ? [] : [pubkey],
          errors: membershipError ? [{ pubkey, error: "access denied" }] : [],
        };
      if (command === "start_managed_agent")
        return { ...agent, status: "deployed" };
      throw new Error(`Unexpected command: ${command}`);
    },
  };
  globalThis.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__;
});
beforeEach(() => {
  calls = [];
  membershipError = false;
});
after(() => dom.window.close());
const input = {
  runtime: { id: "test", label: "Test", command: "test" },
  name: "Agent",
  backend,
  personaId: "persona",
  forceNewInstance: true,
};
test("provider persona creation defers deployment and queues only after membership accepts", async () => {
  const { createChannelManagedAgent } = await import("./channelAgents.ts");
  const queued = [];
  await createChannelManagedAgent("channel", {
    ...input,
    detachedStart: (ready) => {
      assert.equal(calls.at(-1).command, "add_channel_members");
      queued.push(ready);
    },
  });
  assert.equal(
    calls.find((call) => call.command === "create_managed_agent").args.input
      .spawnAfterCreate,
    false,
  );
  assert.equal(
    calls.some((call) => call.command === "start_managed_agent"),
    false,
  );
  assert.equal(queued.length, 1);
  assert.equal(queued[0].pubkey, pubkey);
});
test("failed membership never queues a provider persona or deploys it", async () => {
  const { createChannelManagedAgent } = await import("./channelAgents.ts");
  membershipError = true;
  const queued = [];
  await assert.rejects(
    createChannelManagedAgent("channel", {
      ...input,
      detachedStart: (ready) => queued.push(ready),
    }),
    /access denied/,
  );
  assert.equal(queued.length, 0);
  assert.equal(calls[0].args.input.spawnAfterCreate, false);
  assert.equal(
    calls.some((call) => call.command === "start_managed_agent"),
    false,
  );
});
test("ordinary channel attachment still starts synchronously", async () => {
  const { attachManagedAgentToChannel } = await import("./channelAgents.ts");
  const result = await attachManagedAgentToChannel("channel", { agent });
  assert.equal(result.started, true);
  assert.deepEqual(
    calls.map((call) => call.command),
    ["add_channel_members", "start_managed_agent"],
  );
});
