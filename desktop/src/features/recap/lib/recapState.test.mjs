import assert from "node:assert/strict";
import test from "node:test";

import { initialRecapState, recapReducer } from "./recapState.ts";

test("late completion from an older generation cannot replace the current run", () => {
  let state = initialRecapState();
  state = recapReducer(state, {
    type: "generate_started",
    requestId: "generation-1",
  });
  state = recapReducer(state, {
    type: "generate_started",
    requestId: "generation-2",
  });
  const current = { text: "new", generationId: "generation-2" };
  state = recapReducer(state, {
    type: "generated",
    requestId: "generation-1",
    recap: current,
  });
  assert.equal(state.status, "generating");
  assert.equal(state.requestId, "generation-2");
  assert.equal(state.recap, null);
});

test("cancel fences a late provider result and preserves the previous recap", () => {
  const previous = { text: "old", generationId: "old-generation" };
  let state = { ...initialRecapState(), status: "current", recap: previous };
  state = recapReducer(state, {
    type: "generate_started",
    requestId: "generation-3",
  });
  state = recapReducer(state, {
    type: "cancel_requested",
    requestId: "generation-3",
  });
  state = recapReducer(state, {
    type: "generated",
    requestId: "generation-3",
    recap: { text: "discard me", generationId: "generation-3" },
  });
  assert.equal(state.status, "generating");
  assert.equal(state.cancelRequested, true);
  assert.deepEqual(state.recap, previous);
  state = recapReducer(state, {
    type: "cancelled",
    requestId: "generation-3",
  });
  assert.equal(state.status, "cancelled");
  assert.equal(state.requestId, null);
  assert.deepEqual(state.recap, previous);
});
