import assert from "node:assert/strict";
import test from "node:test";

import { RelayTransportLeaseAuthority } from "./relayTransportLease.ts";

test("a stale transport failure cannot reset or replay onto its replacement", async () => {
  let current = { wsId: 1, generation: 1 };
  let rejectSend;
  let resets = 0;
  let reconnects = 0;
  const sends = [];
  const authority = new RelayTransportLeaseAuthority(
    () => current,
    () => {
      resets += 1;
    },
  );
  const operation = authority.sendWithReconnectRetry(
    ["REQ", "sub-a"],
    "send failed",
    (_payload, lease) => {
      sends.push(lease);
      return new Promise((_resolve, reject) => {
        rejectSend = reject;
      });
    },
    async () => {
      reconnects += 1;
    },
  );

  current = { wsId: 2, generation: 2 };
  rejectSend(new Error("old socket failed"));
  await assert.rejects(operation, /old socket failed/);
  assert.equal(resets, 0);
  assert.equal(reconnects, 0);
  assert.deepEqual(sends, [{ wsId: 1, generation: 1 }]);
});

test("a current transport failure reconnects and retries on the new lease", async () => {
  let current = { wsId: 1, generation: 1 };
  const sends = [];
  const authority = new RelayTransportLeaseAuthority(
    () => current,
    () => {
      current = { wsId: null, generation: 2 };
    },
  );

  await authority.sendWithReconnectRetry(
    ["REQ", "sub-a"],
    "send failed",
    async (_payload, lease) => {
      sends.push(lease);
      if (lease.wsId === 1) throw new Error("socket failed");
    },
    async () => {
      current = { wsId: 2, generation: 2 };
    },
  );

  assert.deepEqual(sends, [
    { wsId: 1, generation: 1 },
    { wsId: 2, generation: 2 },
  ]);
});
