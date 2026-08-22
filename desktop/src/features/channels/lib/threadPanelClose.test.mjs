import assert from "node:assert/strict";
import test from "node:test";

const { resolveThreadPanelClose } = await import("./threadPanelClose.ts");

function build(options) {
  const calls = [];
  const resolved = resolveThreadPanelClose({
    onDismissThread: () => calls.push("dismiss"),
    onMinimizeThread: () => calls.push("minimize"),
    ...options,
  });
  resolved.onClose();
  return { calls, resolved };
}

test("focus drawer with a split pane available minimizes instead of dismissing", () => {
  const { calls, resolved } = build({
    isFocusDrawer: true,
    useSplitAuxiliaryPane: true,
  });

  assert.deepEqual(calls, ["minimize"]);
  assert.equal(resolved.closeLabel, "Minimize thread");
});

test("split and single-panel presentations keep the full dismiss", () => {
  for (const options of [
    { isFocusDrawer: false, useSplitAuxiliaryPane: true },
    { isFocusDrawer: false, useSplitAuxiliaryPane: false },
    { isFocusDrawer: true, useSplitAuxiliaryPane: false },
  ]) {
    const { calls, resolved } = build(options);

    assert.deepEqual(calls, ["dismiss"]);
    assert.equal(resolved.closeLabel, undefined);
  }
});
