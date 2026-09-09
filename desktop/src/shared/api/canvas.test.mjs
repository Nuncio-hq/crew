import assert from "node:assert/strict";
import test from "node:test";

import { getCanvas } from "./canvas.ts";

test("getCanvas retains the native head and editable role metadata", async () => {
  const previous = globalThis.window;
  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async (command, args) => {
        assert.equal(command, "get_canvas");
        assert.deepEqual(args, { channelId: "channel" });
        return {
          content: "founder prose",
          event_id: "canvas-head",
          updated_at: 123,
          author: "owner",
          routing: [],
          assignments: [],
          definitions: [
            { role_label: "Code Review", definition: "Inspect only" },
          ],
          contact_pubkey: "contact",
          crew_authority: "owner",
          crew_parse_state: "valid",
          dev_mcp_granted: null,
          crew_parse_error: null,
        };
      },
    },
  };
  try {
    const canvas = await getCanvas("channel");
    assert.equal(canvas.eventId, "canvas-head");
    assert.deepEqual(canvas.definitions, [
      { roleLabel: "Code Review", definition: "Inspect only" },
    ]);
    assert.equal(canvas.contactPubkey, "contact");
    assert.equal(canvas.crewAuthority, "owner");
    assert.equal(canvas.crewParseState, "valid");
  } finally {
    globalThis.window = previous;
  }
});

test("getCanvas retains foreign and invalid states without stale role metadata", async () => {
  const previous = globalThis.window;
  try {
    for (const [authority, parseState, parseError] of [
      ["absent", "absent", null],
      ["foreign", "valid", null],
      ["owner", "invalid", "malformed Crew block"],
    ]) {
      globalThis.window = {
        __TAURI_INTERNALS__: {
          invoke: async () => ({
            content: "",
            event_id: null,
            routing: [],
            definitions: [],
            assignments: [],
            contact_pubkey: null,
            crew_authority: authority,
            crew_parse_state: parseState,
            dev_mcp_granted: null,
            crew_parse_error: parseError,
          }),
        },
      };
      const canvas = await getCanvas("channel");
      assert.equal(canvas.eventId, null);
      assert.deepEqual(canvas.definitions, []);
      assert.deepEqual(canvas.assignments, []);
      assert.equal(canvas.contactPubkey, null);
      assert.equal(canvas.crewAuthority, authority);
      assert.equal(canvas.crewParseState, parseState);
      assert.equal(canvas.crewParseError, parseError);
    }
  } finally {
    globalThis.window = previous;
  }
});
